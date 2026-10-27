use std::path::Path;
use std::process::Command;

fn main() {
    let has_std = std::env::var("CARGO_FEATURE_STD").is_ok();
    if !has_std {
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let vendor_dir = Path::new(&manifest_dir).join("vendor");
    let archive_path = vendor_dir.join("libdoomgeneric_core.a");
    let build_setun_dir = vendor_dir.join("doomgeneric/doomgeneric/build_setun");

    if !archive_path.exists() && build_setun_dir.exists() {
        let mut o_files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&build_setun_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("o")
                    && path.file_name().and_then(|s| s.to_str()) != Some("doomgeneric_setun.o")
                {
                    o_files.push(path);
                }
            }
        }
        if !o_files.is_empty() {
            let status = Command::new("ar")
                .arg("rcs")
                .arg(&archive_path)
                .args(&o_files)
                .status();
            if let Err(e) = status {
                eprintln!("cargo:warning=Failed to archive libdoomgeneric_core.a: {}", e);
            }
        }
    }

    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=static=doomgeneric_core");
    println!("cargo:rustc-link-lib=m");
}
