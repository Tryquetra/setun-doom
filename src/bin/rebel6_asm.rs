//! Assembles a `.tas` file and reports the image layout.
//!
//! Usage: `rebel6-asm <file.tas> [out.img]`

use std::env;
use std::fs;
use std::process::ExitCode;

use setun_doom::rebel6::asm::assemble;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: rebel6-asm <file.tas> [out.img]");
        return ExitCode::from(2);
    };
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("cannot read {path}: {err}");
            return ExitCode::from(1);
        }
    };
    let image = match assemble(&source) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("assembly failed: {err}");
            return ExitCode::from(1);
        }
    };
    println!(
        "instructions={} data={} bss={} data_base={} entry={} symbols={}",
        image.instrs.len(),
        image.data.len(),
        image.bss_len,
        image.data_base,
        image.entry,
        image.symbols.len()
    );
    if let Some(out) = args.get(2) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"R6IM");
        bytes.extend_from_slice(&(image.instrs.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&(image.data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&image.data_base.to_le_bytes());
        bytes.extend_from_slice(&image.entry.to_le_bytes());
        for instr in &image.instrs {
            bytes.extend_from_slice(&instr.to_le_bytes());
        }
        for cell in &image.data {
            bytes.extend_from_slice(&cell.to_le_bytes());
        }
        if let Err(err) = fs::write(out, bytes) {
            eprintln!("cannot write {out}: {err}");
            return ExitCode::from(1);
        }
        println!("wrote {out}");
    }
    ExitCode::SUCCESS
}
