//! CLI tool to compile and link C/LLVM IR files into native .setun binary executables.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use setun_doom::codegen::{LlvmParser, SetunLinker};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: setun-ld <input.c / input.ll>... [-e <entry_symbol>] -o <output.setun>");
        std::process::exit(1);
    }

    let mut input_files = Vec::new();
    let mut include_dirs = Vec::new();
    let mut output_file = "a.setun".to_string();
    let mut entry_symbol = "main".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                if i + 1 < args.len() {
                    output_file = args[i + 1].clone();
                    i += 1;
                }
            }
            "-e" => {
                if i + 1 < args.len() {
                    entry_symbol = args[i + 1].clone();
                    i += 1;
                }
            }
            "-I" => {
                if i + 1 < args.len() {
                    include_dirs.push(args[i + 1].clone());
                    i += 1;
                }
            }
            arg if arg.starts_with("-I") => {
                include_dirs.push(arg[2..].to_string());
            }
            arg if !arg.starts_with('-') => {
                input_files.push(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    if input_files.is_empty() {
        eprintln!("Error: no input files provided.");
        std::process::exit(1);
    }

    println!(
        "[setun-ld] Linking {} module(s) targeting entry symbol '{}'...",
        input_files.len(),
        entry_symbol
    );

    let mut linker = SetunLinker::new();
    let tmp_dir = env::temp_dir();

    for input in &input_files {
        let p = Path::new(input);
        let ll_path = if p.extension().is_some_and(|ext| ext == "c" || ext == "cpp") {
            let file_stem = p
                .file_stem()
                .expect("Input file path must contain a valid filename")
                .to_str()
                .expect("UTF-8 filename required");
            let out_ll = tmp_dir.join(format!("{}.ll", file_stem));

            println!(
                "  -> Compiling '{}' via host Clang (-O1 -emit-llvm)...",
                input
            );
            let mut clang_cmd = Command::new("clang");
            clang_cmd.args(["-O1", "-S", "-emit-llvm", "-fno-builtin"]);
            clang_cmd.arg("-Isrc/runtime");
            for inc in &include_dirs {
                clang_cmd.arg(format!("-I{}", inc));
            }
            clang_cmd.args([
                input,
                "-o",
                out_ll.to_str().expect("UTF-8 output path required"),
            ]);
            let status = clang_cmd
                .status()
                .expect("Failed to execute host clang command");

            if !status.success() {
                eprintln!("Error: Clang compilation failed for '{}'", input);
                std::process::exit(1);
            }
            out_ll
        } else {
            p.to_path_buf()
        };

        let ll_content = fs::read_to_string(&ll_path)
            .unwrap_or_else(|_| panic!("Failed to read LLVM IR file '{:?}'", ll_path));

        let module = LlvmParser::parse(&ll_content)
            .unwrap_or_else(|e| panic!("Failed to parse LLVM IR in '{:?}': {}", ll_path, e));

        linker.add_module(module);
    }

    let image = linker
        .link(&entry_symbol)
        .unwrap_or_else(|e| panic!("Linking error: {}", e));

    let binary_bytes = image.to_bytes();
    fs::write(&output_file, &binary_bytes)
        .unwrap_or_else(|e| panic!("Failed to write output binary '{}': {}", output_file, e));

    println!(
        "[setun-ld] Successfully generated native binary '{}' ({} words, {} bytes) Entry PC = {}",
        output_file,
        image.code_words.len(),
        binary_bytes.len(),
        image.entry_pc
    );
}
