//! Intermediate Representation (IR) types and structures for the Setun-Doom LLVM backend.

use std::collections::HashMap;

/// An operand or value in an LLVM IR instruction.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LlvmValue {
    /// Local SSA variable or virtual register (e.g. `"%1"`, `"%res"`).
    Variable(String),
    /// Integer literal constant (e.g. `42`, `-10`).
    ConstantInt(i64),
    /// Global symbol reference (e.g. `"@my_global"`).
    Global(String),
    /// Basic block label (e.g. `"entry"`, `"return"`).
    Label(String),
}

impl LlvmValue {
    pub fn is_variable(&self) -> bool {
        matches!(self, Self::Variable(_))
    }
}

/// Integer comparison predicate (`icmp`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IcmpCond {
    Eq,
    Ne,
    Sgt,
    Sge,
    Slt,
    Sle,
}

impl IcmpCond {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "eq" => Some(Self::Eq),
            "ne" => Some(Self::Ne),
            "sgt" => Some(Self::Sgt),
            "sge" => Some(Self::Sge),
            "slt" => Some(Self::Slt),
            "sle" => Some(Self::Sle),
            _ => None,
        }
    }
}

/// Basic LLVM IR instruction subset emitted for arithmetic and control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LlvmInst {
    /// `%dst = add i32 %a, %b`
    Add {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = sub i32 %a, %b`
    Sub {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = mul i32 %a, %b`
    Mul {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = sdiv i32 %a, %b`
    Sdiv {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = srem i32 %a, %b`
    Srem {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = and i32 %a, %b`
    And {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = or i32 %a, %b`
    Or {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = xor i32 %a, %b`
    Xor {
        dst: String,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `%dst = shl i32 %val, %shift`
    Shl {
        dst: String,
        val: LlvmValue,
        shift: LlvmValue,
    },
    /// `%dst = ashr i32 %val, %shift` (or lshr)
    Ashr {
        dst: String,
        val: LlvmValue,
        shift: LlvmValue,
    },
    /// `%dst = icmp cond i32 %a, %b`
    Icmp {
        dst: String,
        cond: IcmpCond,
        lhs: LlvmValue,
        rhs: LlvmValue,
    },
    /// `br label %target`
    BrUncond {
        target: String,
    },
    /// `br i1 %cond, label %if_true, label %if_false`
    BrCond {
        cond: LlvmValue,
        if_true: String,
        if_false: String,
    },
    /// `ret i32 %val` or `ret void`
    Ret {
        val: Option<LlvmValue>,
    },
    /// Direct assignment, sign-extension or truncation (`sext`, `trunc`, `or 0`)
    Copy {
        dst: String,
        src: LlvmValue,
    },
    /// `%dst = phi i32 [ %val1, %label1 ], [ %val2, %label2 ]`
    Phi {
        dst: String,
        incomings: Vec<(LlvmValue, String)>,
    },
    /// `%dst = alloca i32, align 4`
    Alloca {
        dst: String,
    },
    /// `%dst = load i32, ptr %ptr`
    Load {
        dst: String,
        ptr: LlvmValue,
    },
    /// `store i32 %val, ptr %ptr`
    Store {
        val: LlvmValue,
        ptr: LlvmValue,
    },
    /// `%dst = select i1 %cond, i32 %val_true, i32 %val_false`
    Select {
        dst: String,
        cond: LlvmValue,
        val_true: LlvmValue,
        val_false: LlvmValue,
    },
    /// Call instruction: `%dst = [tail] call [type] @callee(args...)` or `call void @callee(args...)`
    Call {
        dst: Option<String>,
        callee: String,
        args: Vec<LlvmValue>,
    },
    /// Address computation: `%dst = getelementptr inbounds %ty, ptr %base, i64 %offset`
    GetElementPtr {
        dst: String,
        base: LlvmValue,
        offset: LlvmValue,
    },
}

/// Global variable or constant definition in the `.data` / `.rodata` / `.bss` segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlvmGlobal {
    pub name: String,
    pub is_constant: bool,
    pub words: Vec<i64>,
}

/// Basic block containing a linear sequence of instructions terminated by a branch or return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlvmBlock {
    pub label: String,
    pub instructions: Vec<LlvmInst>,
}

/// Function within the LLVM module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlvmFunction {
    pub name: String,
    pub params: Vec<String>,
    pub blocks: Vec<LlvmBlock>,
}

/// Module parsed from LLVM textual IR (`.ll`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LlvmModule {
    pub functions: HashMap<String, LlvmFunction>,
    pub globals: HashMap<String, LlvmGlobal>,
}
