//! Compiles a C file or LLVM IR file into REBEL-6 `.tas` assembly.
//!
//! Usage: `rebel6-cc <input.c|input.ll> [-o output.tas] [-e entry] [-O level]`

use std::env;
use std::fs;
use std::process::{Command, ExitCode};

use setun_doom::codegen::{LlvmParser, Rebel6CodeGenerator};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut input = None;
    let mut output = String::from("a.tas");
    let mut entry = String::from("main");
    let mut level = String::from("1");

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "-o" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("-o needs a path");
                    return ExitCode::from(2);
                };
                output = value.clone();
                index += 1;
            }
            "-e" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("-e needs a symbol");
                    return ExitCode::from(2);
                };
                entry = value.clone();
                index += 1;
            }
            "-O" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("-O needs a level");
                    return ExitCode::from(2);
                };
                level = value.clone();
                index += 1;
            }
            flag if flag.starts_with('-') => {
                eprintln!("unknown option {flag}");
                return ExitCode::from(2);
            }
            path => {
                if input.is_some() {
                    eprintln!("one input file at a time");
                    return ExitCode::from(2);
                }
                input = Some(path.to_string());
            }
        }
        index += 1;
    }

    let Some(input) = input else {
        eprintln!("usage: rebel6-cc <input.c|input.ll> [-o output.tas] [-e entry] [-O level]");
        return ExitCode::from(2);
    };

    let source = match fs::read_to_string(&input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {input}: {error}");
            return ExitCode::from(1);
        }
    };

    let ir = if input.ends_with(".ll") {
        source
    } else {
        let result = Command::new("clang")
            .args([&format!("-O{level}"), "-S", "-emit-llvm", &input, "-o", "-"])
            .output();
        match result {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
            Ok(output) => {
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                return ExitCode::from(1);
            }
            Err(error) => {
                eprintln!("cannot run clang: {error}");
                return ExitCode::from(1);
            }
        }
    };

    let module = match LlvmParser::parse(&ir) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("cannot parse the IR: {error}");
            return ExitCode::from(1);
        }
    };

    let assembly = match Rebel6CodeGenerator::compile_module(&module, &entry) {
        Ok(assembly) => assembly,
        Err(error) => {
            eprintln!("cannot compile: {error}");
            return ExitCode::from(1);
        }
    };

    if let Err(error) = fs::write(&output, assembly) {
        eprintln!("cannot write {output}: {error}");
        return ExitCode::from(1);
    }
    println!("wrote {output} (entry {entry})");
    ExitCode::SUCCESS
}
