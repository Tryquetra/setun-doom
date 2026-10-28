//! Two-pass assembler for the `.tas` dialect emitted by R2R.
//!
//! Supported directives: `.text`, `.data`, `.rodata`, `.bss`, `.sbss`, `.section`,
//! `.globl`, `.type`, `.size`, `.align`, `.string`, `.asciz`, `.byte`, `.half`, `.word`,
//! `.zero`, `.space`, `.file`, `.ident`, `.attribute`, `.option`.
//!
//! Symbolic operands produce relocations resolved after the first pass. Text symbols resolve to
//! instruction slots, data symbols to tryte/byte addresses starting at [`DATA_BASE`].

use std::collections::HashMap;

use crate::rebel6::cpu::Cpu;
use crate::rebel6::encoding::{encode, spec, Format, Instr, Op};
use crate::rebel6::word::POW3;

/// Fallback data base used when the image has no instructions.
pub const DATA_BASE: i64 = 0x10_0000;

/// Default data memory size in trytes/bytes.
pub const DEFAULT_DATA_SIZE: usize = 8 * 1024 * 1024;

/// Default stack top.
pub const DEFAULT_STACK_TOP: i64 = 0x40_0000;

/// Assembled image.
pub struct Image {
    /// Encoded instruction memory.
    pub instrs: Vec<u64>,
    /// Data bytes.
    pub data: Vec<i16>,
    /// Data base address.
    pub data_base: i64,
    /// Entry instruction index.
    pub entry: i64,
    /// Symbol table, text symbols in instruction slots and data symbols in addresses.
    pub symbols: HashMap<String, i64>,
}

impl Image {
    /// Copies the data image into a machine.
    pub fn load_into(&self, cpu: &mut Cpu) {
        for (i, &cell) in self.data.iter().enumerate() {
            let addr = self.data_base + i as i64;
            if let Ok(index) = usize::try_from(addr) {
                if index < cpu.data.len() {
                    cpu.data.cells_mut()[index] = cell;
                }
            }
        }
        cpu.set_pc(self.entry);
    }

    /// Creates a machine sized for this image.
    #[must_use]
    pub fn machine(&self) -> Cpu {
        let size = (self.data_base as usize + self.data.len()).max(DEFAULT_DATA_SIZE);
        let mut cpu = Cpu::new(self.instrs.clone(), size);
        self.load_into(&mut cpu);
        cpu
    }
}

/// Assembly error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmError {
    /// Line number (1-based).
    pub line: usize,
    /// Message.
    pub message: String,
}

impl core::fmt::Display for AsmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for AsmError {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Section {
    Text,
    Data,
}

/// `(offset, base register, optional symbol addend)`.
type MemoryOperand = (i64, i64, Option<(String, i64)>);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum RelocKind {
    Pcrel24,
    Pcrel12,
    Abs,
    Disp12,
}

#[derive(Clone, Debug)]
struct InstrReloc {
    index: usize,
    kind: RelocKind,
    symbol: String,
    addend: i64,
}

#[derive(Clone, Debug)]
struct DataReloc {
    offset: usize,
    symbol: String,
    addend: i64,
}

struct Assembler {
    section: Section,
    instrs: Vec<Instr>,
    data: Vec<i16>,
    labels: HashMap<String, (Section, i64)>,
    instr_relocs: Vec<InstrReloc>,
    data_relocs: Vec<DataReloc>,
    line: usize,
}

/// Assembles `.tas` source into an image.
///
/// # Errors
///
/// Returns [`AsmError`] on syntax errors, unknown mnemonics, unknown symbols or out-of-range
/// displacements.
pub fn assemble(source: &str) -> Result<Image, AsmError> {
    let mut asm = Assembler {
        section: Section::Text,
        instrs: Vec::new(),
        data: Vec::new(),
        labels: HashMap::new(),
        instr_relocs: Vec::new(),
        data_relocs: Vec::new(),
        line: 0,
    };
    for (number, raw) in source.lines().enumerate() {
        asm.line = number + 1;
        asm.parse_line(raw)?;
    }
    asm.finish()
}

impl Assembler {
    fn error<T>(&self, message: impl Into<String>) -> Result<T, AsmError> {
        Err(AsmError {
            line: self.line,
            message: message.into(),
        })
    }

    fn parse_line(&mut self, raw: &str) -> Result<(), AsmError> {
        let mut rest = strip_comment(raw).trim().to_string();
        if rest.is_empty() {
            return Ok(());
        }
        while let Some(colon) = rest.find(':') {
            let label = rest[..colon].trim();
            if label.is_empty() || label.contains(char::is_whitespace) {
                break;
            }
            if !is_label(label) {
                break;
            }
            let location = match self.section {
                Section::Text => self.instrs.len() as i64,
                Section::Data => self.data.len() as i64,
            };
            self.labels
                .insert(label.to_string(), (self.section, location));
            rest = rest[colon + 1..].trim().to_string();
            if rest.is_empty() {
                return Ok(());
            }
        }
        if rest.starts_with('.') {
            self.parse_directive(&rest)
        } else {
            self.parse_instruction(&rest)
        }
    }

    fn parse_directive(&mut self, line: &str) -> Result<(), AsmError> {
        let (name, args) = split_mnemonic(line);
        let args = args.trim();
        match name {
            ".text" => self.section = Section::Text,
            ".data" | ".rodata" | ".bss" | ".sbss" => self.section = Section::Data,
            ".section" => {
                let lower = args.to_ascii_lowercase();
                self.section = if lower.contains(".text") {
                    Section::Text
                } else {
                    Section::Data
                };
            }
            ".globl" | ".global" | ".type" | ".size" | ".file" | ".ident" | ".attribute"
            | ".option" | ".set" | ".local" | ".weak" | ".p2align" | ".balign" => {}
            ".align" => {
                if self.section == Section::Data {
                    let n = parse_number(args).unwrap_or(0).clamp(0, 16) as u32;
                    let align = 1usize << n;
                    while !self.data.len().is_multiple_of(align) {
                        self.data.push(0);
                    }
                }
            }
            ".string" | ".asciz" | ".ascii" => {
                let bytes = parse_string(args)?;
                for b in bytes {
                    self.data.push(i16::from(b));
                }
                if name != ".ascii" {
                    self.data.push(0);
                }
            }
            ".byte" => {
                for part in split_operands(args) {
                    let value = parse_number(part.trim())
                        .ok_or_else(|| self.err(format!("invalid .byte value `{part}`")))?;
                    self.data.push((value & 0xff) as i16);
                }
            }
            ".half" | ".2byte" => {
                for part in split_operands(args) {
                    let value = parse_number(part.trim())
                        .ok_or_else(|| self.err(format!("invalid .half value `{part}`")))?;
                    self.data.push((value & 0xff) as i16);
                    self.data.push(((value >> 8) & 0xff) as i16);
                }
            }
            ".word" | ".4byte" => {
                for part in split_operands(args) {
                    let part = part.trim();
                    if let Some(value) = parse_number(part) {
                        for shift in [0, 8, 16, 24] {
                            self.data.push(((value >> shift) & 0xff) as i16);
                        }
                    } else {
                        let (symbol, addend) = parse_symbol(part)?;
                        let offset = self.data.len();
                        self.data.extend([0, 0, 0, 0]);
                        self.data_relocs.push(DataReloc {
                            offset,
                            symbol,
                            addend,
                        });
                    }
                }
            }
            ".zero" | ".space" => {
                let n = parse_number(args).unwrap_or(0).max(0) as usize;
                self.data.resize(self.data.len() + n, 0);
            }
            _ => return self.error(format!("unknown directive `{name}`")),
        }
        Ok(())
    }

    fn parse_instruction(&mut self, line: &str) -> Result<(), AsmError> {
        let (mnemonic, operand_text) = split_mnemonic(line);
        let operands: Vec<&str> = split_operands(operand_text)
            .into_iter()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let mnemonic = mnemonic.to_ascii_lowercase();

        if let Some(expanded) = self.expand_pseudo(&mnemonic, &operands)? {
            for instr in expanded {
                self.push_instr(instr);
            }
            return Ok(());
        }

        let (op, fill, shift_sign) = parse_op(&mnemonic)
            .ok_or_else(|| self.err(format!("unknown mnemonic `{mnemonic}`")))?;
        let mut instr = Instr::new(op);
        let mut reloc: Option<(RelocKind, String, i64)> = None;
        match spec(op).0 {
            Format::R => {
                if op == Op::StiT {
                    instr.rd1 = self.register(operands.first().copied())?;
                    instr.rs1 = self.register(operands.get(1).copied())?;
                } else if op == Op::Mv2T {
                    instr.rd1 = self.register(operands.first().copied())?;
                    instr.rd2 = self.register(operands.get(1).copied())?;
                    instr.rs1 = self.register(operands.get(2).copied())?;
                    instr.rs2 = self.register(operands.get(3).copied())?;
                } else {
                    instr.rd1 = self.register(operands.first().copied())?;
                    instr.rs1 = self.register(operands.get(1).copied())?;
                    instr.rs2 = self.register(operands.get(2).copied())?;
                    instr.fill = fill;
                }
            }
            Format::I => {
                instr.rd1 = self.register(operands.first().copied())?;
                instr.fill = fill;
                if matches!(
                    op,
                    Op::LwT
                        | Op::LhT
                        | Op::LtT
                        | Op::JalrT
                        | Op::Lw
                        | Op::Lh
                        | Op::Lb
                        | Op::Lhu
                        | Op::Lbu
                ) {
                    let (offset, base, symbol) = self.memory_operand(operands.get(1).copied())?;
                    instr.rs1 = base;
                    if let Some((symbol, addend)) = symbol {
                        reloc = Some((RelocKind::Disp12, symbol, addend));
                        instr.imm = 0;
                    } else {
                        instr.imm = if shift_sign { -offset } else { offset };
                    }
                } else {
                    instr.rs1 = self.register(operands.get(1).copied())?;
                    let immediate = operands.get(2).copied().unwrap_or("0");
                    instr.imm = if let Some(value) = parse_imm32(immediate) {
                        if shift_sign {
                            -value
                        } else {
                            value
                        }
                    } else {
                        let (symbol, addend) = parse_symbol(immediate)?;
                        reloc = Some((RelocKind::Disp12, symbol, addend));
                        0
                    };
                }
            }
            Format::B => {
                if matches!(
                    op,
                    Op::Sw | Op::Sh | Op::Sb | Op::SwT | Op::ShTStore | Op::StT
                ) {
                    instr.rs2 = self.register(operands.first().copied())?;
                    let (offset, base, symbol) = self.memory_operand(operands.get(1).copied())?;
                    instr.rs1 = base;
                    if let Some((symbol, addend)) = symbol {
                        reloc = Some((RelocKind::Disp12, symbol, addend));
                        instr.imm = 0;
                    } else {
                        instr.imm = offset;
                    }
                } else if matches!(op, Op::BcgsT | Op::BcegT) {
                    instr.rs1 = self.register(operands.first().copied())?;
                    instr.rs2 = self.register(operands.get(1).copied())?;
                    instr.imm = parse_imm32(operands.get(2).copied().unwrap_or("0"))
                        .ok_or_else(|| self.err("invalid off1"))?;
                    instr.imm2 = parse_imm32(operands.get(3).copied().unwrap_or("0"))
                        .ok_or_else(|| self.err("invalid off2"))?;
                } else {
                    instr.rs1 = self.register(operands.first().copied())?;
                    instr.rs2 = self.register(operands.get(1).copied())?;
                    let target = operands.get(2).copied().unwrap_or("0");
                    if let Some(value) = parse_imm32(target) {
                        instr.imm = value;
                    } else {
                        let (symbol, addend) = parse_symbol(target)?;
                        reloc = Some((RelocKind::Pcrel12, symbol, addend));
                    }
                }
            }
            Format::G => {
                instr.rd1 = self.register(operands.first().copied())?;
                let value = operands.get(1).copied().unwrap_or("0");
                if let Some((kind, symbol, addend)) = parse_address_operand(value)? {
                    reloc = Some((kind, symbol, addend));
                } else if let Some(number) = parse_imm32(value) {
                    instr.imm = number;
                } else {
                    let (symbol, addend) = parse_symbol(value)?;
                    let kind = if matches!(op, Op::JalT | Op::AipcT) {
                        RelocKind::Pcrel24
                    } else {
                        RelocKind::Abs
                    };
                    reloc = Some((kind, symbol, addend));
                }
            }
            Format::X => {
                instr.rd1 = self.register(operands.first().copied())?;
                instr.rd2 = self.register(operands.get(1).copied())?;
                instr.imm = parse_imm32(operands.get(2).copied().unwrap_or("0"))
                    .ok_or_else(|| self.err("invalid imm6a"))?;
                instr.imm2 = parse_imm32(operands.get(3).copied().unwrap_or("0"))
                    .ok_or_else(|| self.err("invalid imm6b"))?;
            }
            Format::D => {
                instr.rd1 = self.register(operands.first().copied())?;
                instr.rs1 = self.register(operands.get(1).copied())?;
                instr.rs2 = self.register(operands.get(2).copied())?;
                instr.rs3 = self.register(operands.get(3).copied())?;
            }
            Format::Y => {
                instr.rs1 = self.register(operands.first().copied())?;
                let value = operands.get(1).copied().unwrap_or("0");
                if let Some(number) = parse_imm32(value) {
                    instr.imm = number;
                } else {
                    let (symbol, addend) = parse_symbol(value)?;
                    reloc = Some((RelocKind::Abs, symbol, addend));
                }
            }
        }
        let index = self.push_instr(instr);
        if let Some((kind, symbol, addend)) = reloc {
            self.instr_relocs.push(InstrReloc {
                index,
                kind,
                symbol,
                addend,
            });
        }
        Ok(())
    }

    fn expand_pseudo(
        &self,
        mnemonic: &str,
        operands: &[&str],
    ) -> Result<Option<Vec<Instr>>, AsmError> {
        let reg = |index: usize| self.register(operands.get(index).copied());
        let result = match mnemonic {
            "nop" => vec![Instr::i(Op::AddiT, 0, 0, 0)],
            "mv" => vec![Instr::i(Op::AddiT, reg(0)?, reg(1)?, 0)],
            "li" => vec![Instr::g(
                Op::LiT,
                reg(0)?,
                self.immediate(operands.get(1).copied())?,
            )],
            "j" => vec![Instr::g(Op::JalT, 0, 0)],
            "jr" => vec![Instr::i(Op::JalrT, 0, reg(0)?, 0)],
            "ret" => vec![Instr::i(Op::JalrT, 0, 1, 0)],
            "call" => vec![Instr::g(Op::JalT, 1, 0)],
            "tail" => vec![Instr::g(Op::JalT, 0, 0)],
            "beqz" => vec![Instr::b(Op::BeqT, reg(0)?, 0, 0)],
            "bnez" => vec![Instr::b(Op::BneT, reg(0)?, 0, 0)],
            "blez" => vec![Instr::b(Op::BgeT, 0, reg(0)?, 0)],
            "bgez" => vec![Instr::b(Op::BgeT, reg(0)?, 0, 0)],
            "bltz" => vec![Instr::b(Op::BltT, reg(0)?, 0, 0)],
            "bgtz" => vec![Instr::b(Op::BltT, 0, reg(0)?, 0)],
            "bgt" => vec![Instr::b(Op::BltT, reg(1)?, reg(0)?, 0)],
            "ble" => vec![Instr::b(Op::BgeT, reg(1)?, reg(0)?, 0)],
            "bgtu" => vec![Instr::b(Op::Bltu, reg(1)?, reg(0)?, 0)],
            "bleu" => vec![Instr::b(Op::Bgeu, reg(1)?, reg(0)?, 0)],
            "not" => vec![Instr::i(Op::Xori, reg(0)?, reg(1)?, -1)],
            "neg" => vec![Instr::r(Op::Sub, reg(0)?, 0, reg(1)?)],
            "seqz" => vec![Instr::i(Op::Sltiu, reg(0)?, reg(1)?, 1)],
            "snez" => vec![Instr::r(Op::Sltu, reg(0)?, 0, reg(1)?)],
            "sltz" => vec![Instr::r(Op::SltT, reg(0)?, reg(1)?, 0)],
            "sgtz" => vec![Instr::r(Op::SltT, reg(0)?, 0, reg(1)?)],
            "fence" => vec![Instr::new(Op::FenceT)],
            "ecall" => vec![Instr::new(Op::EcallT)],
            "ebreak" => vec![Instr::new(Op::EbreakT)],
            _ => return Ok(None),
        };
        Ok(Some(result))
    }

    fn immediate(&self, operand: Option<&str>) -> Result<i64, AsmError> {
        let operand = operand.unwrap_or("0").trim();
        parse_imm32(operand).ok_or_else(|| self.err(format!("invalid immediate `{operand}`")))
    }

    fn register(&self, operand: Option<&str>) -> Result<i64, AsmError> {
        let name = operand.unwrap_or("").trim();
        parse_register(name).ok_or_else(|| self.err(format!("invalid register `{name}`")))
    }

    fn memory_operand(&self, operand: Option<&str>) -> Result<MemoryOperand, AsmError> {
        let text = operand.unwrap_or("").trim();
        if let Some(open) = text.find('(') {
            if !text.ends_with(')') {
                return self.error(format!("invalid memory operand `{text}`"));
            }
            let offset = text[..open].trim();
            let base = text[open + 1..text.len() - 1].trim();
            let base = parse_register(base)
                .ok_or_else(|| self.err(format!("invalid base register `{base}`")))?;
            if offset.is_empty() {
                return Ok((0, base, None));
            }
            if let Some(number) = parse_imm32(offset) {
                return Ok((number, base, None));
            }
            let (symbol, addend) = parse_symbol(offset)?;
            return Ok((0, base, Some((symbol, addend))));
        }
        if let Some(number) = parse_imm32(text) {
            return Ok((number, 0, None));
        }
        let (symbol, addend) = parse_symbol(text)?;
        Ok((0, 0, Some((symbol, addend))))
    }

    fn push_instr(&mut self, instr: Instr) -> usize {
        self.instrs.push(instr);
        self.instrs.len() - 1
    }

    fn err(&self, message: impl Into<String>) -> AsmError {
        AsmError {
            line: self.line,
            message: message.into(),
        }
    }

    fn finish(mut self) -> Result<Image, AsmError> {
        // R2R lays data immediately after the instruction image, rounded up to four trytes.
        let instruction_trytes = ((self.instrs.len() as i64 * 32) + 5) / 6;
        let data_base = if self.instrs.is_empty() {
            DATA_BASE
        } else {
            instruction_trytes + 4 - (instruction_trytes % 4)
        };
        for (section, location) in self.labels.values_mut() {
            if *section == Section::Data {
                *location += data_base;
            }
        }
        let entry_name = if self.labels.contains_key("main") {
            "main"
        } else if self.labels.contains_key("_start") {
            "_start"
        } else {
            ""
        };
        let entry = self
            .labels
            .get(entry_name)
            .filter(|(section, _)| *section == Section::Text)
            .map_or(0, |(_, location)| *location);

        let mut symbols = HashMap::new();
        for (name, (section, location)) in &self.labels {
            let _ = section;
            symbols.insert(name.clone(), *location);
        }

        for reloc in &self.instr_relocs {
            let (section, target) =
                self.labels
                    .get(&reloc.symbol)
                    .copied()
                    .ok_or_else(|| AsmError {
                        line: self.line,
                        message: format!("undefined symbol `{}`", reloc.symbol),
                    })?;
            let target = target + reloc.addend;
            let pc = reloc.index as i64;
            let value = match reloc.kind {
                RelocKind::Pcrel24 | RelocKind::Pcrel12 => {
                    if reloc.kind == RelocKind::Pcrel12 && section != Section::Text {
                        return Err(AsmError {
                            line: self.line,
                            message: format!(
                                "PC-relative branch to data symbol `{}`",
                                reloc.symbol
                            ),
                        });
                    }
                    target - pc
                }
                RelocKind::Abs | RelocKind::Disp12 => target,
            };
            self.instrs[reloc.index].imm = value;
        }

        for reloc in &self.data_relocs {
            let (_, target) = self
                .labels
                .get(&reloc.symbol)
                .copied()
                .ok_or_else(|| AsmError {
                    line: self.line,
                    message: format!("undefined symbol `{}`", reloc.symbol),
                })?;
            let value = target + reloc.addend;
            for shift in 0..4 {
                self.data[reloc.offset + shift] = ((value >> (shift * 8)) & 0xff) as i16;
            }
        }

        for instr in &self.instrs {
            let (format, _, _) = spec(instr.op);
            let limit = match format {
                Format::B => POW3[12],
                Format::I => POW3[12],
                _ => POW3[24],
            };
            if instr.imm.abs() > (limit - 1) / 2 {
                return Err(AsmError {
                    line: self.line,
                    message: format!(
                        "immediate {} out of range for {}",
                        instr.imm,
                        instr.op.name()
                    ),
                });
            }
        }

        Ok(Image {
            instrs: self.instrs.iter().map(encode).collect(),
            data: self.data,
            data_base,
            entry,
            symbols,
        })
    }
}

fn is_label(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'))
}

fn strip_comment(line: &str) -> &str {
    let mut in_quote = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '"' && (i == 0 || bytes[i - 1] != b'\\') {
            in_quote = !in_quote;
        }
        if !in_quote {
            if c == '#' || c == ';' {
                return &line[..i];
            }
            if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                return &line[..i];
            }
        }
        i += 1;
    }
    line
}

fn split_mnemonic(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(index) => (&line[..index], line[index..].trim()),
        None => (line, ""),
    }
}

fn split_operands(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if c == '"' && (i == 0 || bytes[i - 1] != b'\\') {
            in_quote = !in_quote;
        }
        if in_quote {
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn parse_number(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix('\'') {
        let mut chars = rest.chars();
        let c = chars.next()?;
        return Some(c as i64);
    }
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let value = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()? as i64
    } else if let Some(bin) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        u64::from_str_radix(bin, 2).ok()? as i64
    } else if let Some(oct) = digits.strip_prefix("0o") {
        u64::from_str_radix(oct, 8).ok()? as i64
    } else {
        digits.parse::<u64>().ok()? as i64
    };
    Some(if negative { -value } else { value })
}

/// Parses an instruction immediate, sign-extending 32-bit bit patterns like RV32I.
fn parse_imm32(text: &str) -> Option<i64> {
    let value = parse_number(text)?;
    if value > i64::from(i32::MAX) && value <= i64::from(u32::MAX) {
        Some(i64::from(value as u32 as i32))
    } else {
        Some(value)
    }
}

fn parse_register(name: &str) -> Option<i64> {
    let name = name.trim();
    if let Some(rest) = name.strip_prefix('x') {
        if let Ok(index) = rest.parse::<i64>() {
            if (0..32).contains(&index) {
                return Some(index);
            }
        }
    }
    Some(match name {
        "zero" => 0,
        "ra" => 1,
        "sp" => 2,
        "gp" => 3,
        "tp" => 4,
        "t0" => 5,
        "t1" => 6,
        "t2" => 7,
        "s0" | "fp" => 8,
        "s1" => 9,
        "a0" => 10,
        "a1" => 11,
        "a2" => 12,
        "a3" => 13,
        "a4" => 14,
        "a5" => 15,
        "a6" => 16,
        "a7" => 17,
        "s2" => 18,
        "s3" => 19,
        "s4" => 20,
        "s5" => 21,
        "s6" => 22,
        "s7" => 23,
        "s8" => 24,
        "s9" => 25,
        "s10" => 26,
        "s11" => 27,
        "t3" => 28,
        "t4" => 29,
        "t5" => 30,
        "t6" => 31,
        _ => return None,
    })
}

fn parse_symbol(text: &str) -> Result<(String, i64), AsmError> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("%pcrel(")
        .or_else(|| text.strip_prefix("%pc_rel("))
        .and_then(|s| s.strip_suffix(')'))
    {
        return parse_symbol(inner);
    }
    if let Some(inner) = text
        .strip_prefix("%hi(")
        .or_else(|| text.strip_prefix("%lo("))
        .or_else(|| text.strip_prefix("%pcrel_hi("))
        .or_else(|| text.strip_prefix("%pcrel_lo("))
        .and_then(|s| s.strip_suffix(')'))
    {
        return parse_symbol(inner);
    }
    let (symbol, addend) = if let Some(pos) = text.rfind(['+', '-']) {
        if pos > 0 {
            let (sym, num) = text.split_at(pos);
            (sym.trim(), parse_number(num).unwrap_or(0))
        } else {
            (text, 0)
        }
    } else {
        (text, 0)
    };
    if symbol.is_empty() {
        return Err(AsmError {
            line: 0,
            message: format!("invalid symbol `{text}`"),
        });
    }
    Ok((symbol.to_string(), addend))
}

fn parse_address_operand(text: &str) -> Result<Option<(RelocKind, String, i64)>, AsmError> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("%pcrel(")
        .or_else(|| text.strip_prefix("%pc_rel("))
        .and_then(|s| s.strip_suffix(')'))
    {
        let (symbol, addend) = parse_symbol(inner)?;
        return Ok(Some((RelocKind::Pcrel24, symbol, addend)));
    }
    if let Some(inner) = text
        .strip_prefix("%hi(")
        .or_else(|| text.strip_prefix("%lo("))
        .and_then(|s| s.strip_suffix(')'))
    {
        let (symbol, addend) = parse_symbol(inner)?;
        return Ok(Some((RelocKind::Abs, symbol, addend)));
    }
    Ok(None)
}

fn parse_string(text: &str) -> Result<Vec<u8>, AsmError> {
    let text = text.trim();
    let start = text.find('"').ok_or_else(|| AsmError {
        line: 0,
        message: format!("invalid string `{text}`"),
    })?;
    let end = text.rfind('"').ok_or_else(|| AsmError {
        line: 0,
        message: format!("invalid string `{text}`"),
    })?;
    if end <= start {
        return Ok(Vec::new());
    }
    let body = &text[start + 1..end];
    let mut out = Vec::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c as u8);
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('t') => out.push(b'\t'),
            Some('r') => out.push(b'\r'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('x') => {
                let hi = chars.next().unwrap_or('0');
                let lo = chars.next().unwrap_or('0');
                let value = u8::from_str_radix(&format!("{hi}{lo}"), 16).unwrap_or(0);
                out.push(value);
            }
            Some(other) => out.push(other as u8),
            None => {}
        }
    }
    Ok(out)
}

fn parse_op(name: &str) -> Option<(Op, i8, bool)> {
    let (op, fill, sign) = match name {
        "add.t" => (Op::AddT, 0, false),
        "sub.t" => (Op::SubT, 0, false),
        "slt.t" => (Op::SltT, 0, false),
        "or.t" => (Op::OrT, 0, false),
        "xor.t" => (Op::XorT, 0, false),
        "and.t" => (Op::AndT, 0, false),
        "cmp.t" => (Op::CmpT, 0, false),
        "sti.t" => (Op::StiT, 0, false),
        "mv2.t" => (Op::Mv2T, 0, false),
        "min.t" => (Op::MinT, 0, false),
        "max.t" => (Op::MaxT, 0, false),
        "addi.t" => (Op::AddiT, 0, false),
        "rot.t" => (Op::RotT, 0, false),
        "rotr.t" => (Op::RotrT, 0, false),
        "slti.t" => (Op::SltiT, 0, false),
        "ori.t" => (Op::OriT, 0, false),
        "xori.t" => (Op::XoriT, 0, false),
        "andi.t" => (Op::AndiT, 0, false),
        "lw.t" => (Op::LwT, 0, false),
        "lh.t" => (Op::LhT, 0, false),
        "lt.t" => (Op::LtT, 0, false),
        "jalr.t" => (Op::JalrT, 0, false),
        "bcgs.t" => (Op::BcgsT, 0, false),
        "bceg.t" => (Op::BcegT, 0, false),
        "beq.t" => (Op::BeqT, 0, false),
        "bne.t" => (Op::BneT, 0, false),
        "blt.t" => (Op::BltT, 0, false),
        "bge.t" => (Op::BgeT, 0, false),
        "sw.t" => (Op::SwT, 0, false),
        "sh.t" => (Op::ShTStore, 0, false),
        "st.t" => (Op::StT, 0, false),
        "majv.t" => (Op::MajvT, 0, false),
        "minv.t" => (Op::MinvT, 0, false),
        "li2.t" => (Op::Li2T, 0, false),
        "fence.t" => (Op::FenceT, 0, false),
        "wfi.t" => (Op::WfiT, 0, false),
        "tret.t" => (Op::TretT, 0, false),
        "ebreak.t" => (Op::EbreakT, 0, false),
        "ecall.t" => (Op::EcallT, 0, false),
        "lwa.t" => (Op::LwaT, 0, false),
        "li.t" => (Op::LiT, 0, false),
        "swa.t" => (Op::SwaT, 0, false),
        "jal.t" => (Op::JalT, 0, false),
        "aipc.t" => (Op::AipcT, 0, false),
        "shn.t" | "sln.t" => (Op::ShT, -1, false),
        "shz.t" | "slz.t" => (Op::ShT, 0, false),
        "shp.t" | "slp.t" => (Op::ShT, 1, false),
        "shin.t" | "slin.t" => (Op::ShiT, -1, false),
        "shiz.t" | "sliz.t" => (Op::ShiT, 0, false),
        "ship.t" | "slip.t" => (Op::ShiT, 1, false),
        "srin.t" => (Op::ShiT, -1, true),
        "sriz.t" => (Op::ShiT, 0, true),
        "srip.t" => (Op::ShiT, 1, true),
        "add" => (Op::Add, 0, false),
        "sub" => (Op::Sub, 0, false),
        "sll" => (Op::Sll, 0, false),
        "srl" => (Op::Srl, 0, false),
        "sra" => (Op::Sra, 0, false),
        "sltu" => (Op::Sltu, 0, false),
        "slt" => (Op::SltT, 0, false),
        "slti" => (Op::SltiT, 0, false),
        "or" => (Op::Or, 0, false),
        "xor" => (Op::Xor, 0, false),
        "and" => (Op::And, 0, false),
        "addi" => (Op::Addi, 0, false),
        "slli" => (Op::Slli, 0, false),
        "srli" => (Op::Srli, 0, false),
        "srai" => (Op::Srai, 0, false),
        "sltiu" => (Op::Sltiu, 0, false),
        "ori" => (Op::Ori, 0, false),
        "xori" => (Op::Xori, 0, false),
        "andi" => (Op::Andi, 0, false),
        "lw" => (Op::Lw, 0, false),
        "lh" => (Op::Lh, 0, false),
        "lb" => (Op::Lb, 0, false),
        "lhu" => (Op::Lhu, 0, false),
        "lbu" => (Op::Lbu, 0, false),
        "bltu" => (Op::Bltu, 0, false),
        "bgeu" => (Op::Bgeu, 0, false),
        "sw" => (Op::Sw, 0, false),
        "sh" => (Op::Sh, 0, false),
        "sb" => (Op::Sb, 0, false),
        _ => return None,
    };
    Some((op, fill, sign))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebel6::cpu::RunResult;

    #[test]
    fn assembles_and_runs_a_small_program() {
        let source = r#"
            .text
            .globl main
        main:
            li.t x1, 5
            addi.t x2, x1, 7
            li.t x3, 0
            beq.t x3, x0, done
            li.t x2, 99
        done:
            ecall
        "#;
        let image = assemble(source).unwrap();
        assert_eq!(image.entry, 0);
        let mut cpu = image.machine();
        assert_eq!(cpu.run(32), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[2].raw(), 12);
    }

    #[test]
    fn resolves_pcrel_and_absolute_symbols() {
        let source = r#"
            .text
        main:
            aipc.t x1, %pcrel(target)
            jal.t x2, target
            ecall
        target:
            li.t x3, 7
            ecall
            .data
        value:
            .word 0x12345678
        "#;
        let image = assemble(source).unwrap();
        assert_eq!(image.symbols["main"], 0);
        assert_eq!(image.symbols["target"], 3);
        assert_eq!(image.symbols["value"], image.data_base);
        let mut cpu = image.machine();
        assert_eq!(cpu.run(32), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[1].raw(), 3);
        assert_eq!(cpu.regs[2].raw(), 2);
        assert_eq!(cpu.regs[3].raw(), 7);
    }

    #[test]
    fn emits_data_and_loads_it() {
        let source = r#"
            .text
        main:
            li.t x1, value
            lw x2, 0(x1)
            ecall
            .data
        value:
            .word 0xDEADBEEF
        "#;
        let image = assemble(source).unwrap();
        let mut cpu = image.machine();
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[2].to_i32() as u32, 0xdead_beef);
    }
}
