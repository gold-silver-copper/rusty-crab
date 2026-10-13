use quote::ToTokens;
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process::Command};
#[path = "src/compiler_source.rs"]
mod compiler_source;

fn main() {
    let mut files: Vec<_> = fs::read_dir("src")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.extend(
        [
            "build.rs",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ]
        .map(PathBuf::from),
    );
    files.sort();
    let mut fingerprint = Sha256::new();
    println!("cargo:rerun-if-changed=src");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        fingerprint.update(path.to_string_lossy().as_bytes());
        fingerprint.update([0]);
        fingerprint.update(fs::read(path).unwrap());
        fingerprint.update([0]);
    }
    println!(
        "cargo:rustc-env=CRAB_AUDITOR_HASH={:x}",
        fingerprint.finalize()
    );
    let version = Command::new(env::var_os("RUSTC").unwrap())
        .arg("-vV")
        .output()
        .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(
        version
            .lines()
            .any(|line| line == "commit-hash: b940084d7eb6a299eb4bfeb8e34901bc051e7ac4"),
        "the compiler-private auditor must be built with the pinned compiler revision"
    );
    let out = Command::new(env::var_os("RUSTC").unwrap())
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let sysroot = String::from_utf8(out.stdout).unwrap();
    println!("cargo:rustc-link-search=native={}/lib", sysroot.trim());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", sysroot.trim());
    println!("cargo:rustc-env=CRAB_SYSROOT={}", sysroot.trim());
    let sources = [
        (
            "ast",
            "vendor/rust/compiler/rustc_ast/src/ast.rs",
            "rustc_ast::ast",
        ),
        (
            "lex",
            "vendor/rust/compiler/rustc_lexer/src/lib.rs",
            "rustc_lexer",
        ),
    ];
    let mut generated = String::from(
        "// Generated exhaustive discriminant classifiers from pinned compiler source.\n",
    );
    for (prefix, path, module) in sources {
        println!("cargo:rerun-if-changed={path}");
        let file = syn::parse_file(&fs::read_to_string(path).unwrap()).unwrap();
        for item in file.items {
            let syn::Item::Enum(item) = item else {
                continue;
            };
            if !matches!(item.vis, syn::Visibility::Public(_)) || !item.generics.params.is_empty() {
                continue;
            }
            let name = item.ident.to_string();
            // These public enum definitions are accessible through compiler-private crates.
            generated.push_str(&format!("#[allow(dead_code)] fn classify_{prefix}_{name}(value: &{module}::{name}) -> &'static str {{ match value {{\n"));
            for variant in item.variants {
                let tail = match variant.fields {
                    syn::Fields::Unit => "",
                    syn::Fields::Named(_) => " { .. }",
                    syn::Fields::Unnamed(_) => "(..)",
                };
                let variant = variant.ident.to_token_stream().to_string();
                generated.push_str(&format!(
                    "{module}::{name}::{variant}{tail} => \"{prefix}.{name}.{variant}\",\n"
                ));
            }
            generated.push_str("} }\n");
        }
    }
    let keywords_path = "vendor/rust/compiler/rustc_span/src/symbol.rs";
    println!("cargo:rerun-if-changed={keywords_path}");
    generated.push_str("const KEYWORDS: &[(&str, &str)] = &[\n");
    for keyword in compiler_source::keywords(&fs::read_to_string(keywords_path).unwrap()).unwrap() {
        generated.push_str(&format!(
            "({:?}, {:?}),\n",
            keyword.spelling.value(),
            format!("keyword.{}", keyword.name)
        ));
    }
    generated.push_str("];\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("classifiers.rs"),
        generated,
    )
    .unwrap();
}
