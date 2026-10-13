use crate::model::{read_json, write_json};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Serialize, Deserialize)]
pub struct Line {
    pub code: String,
    pub explanation: String,
}

#[derive(Serialize, Deserialize)]
pub struct Example {
    pub id: String,
    pub intended_requirements: Vec<String>,
    pub prerequisites: Vec<String>,
    pub configuration: String,
    pub lines: Vec<Line>,
}

pub fn generate(repository: &Path, project: &Path, profile: crate::profile::Profile) -> Result<()> {
    if profile == crate::profile::Profile::Freestanding {
        return generate_freestanding(repository, project);
    }
    let examples: Vec<Example> = read_json(&repository.join("catalog/stable.json"))?;
    ensure!(!examples.is_empty(), "empty example catalog");
    fs::create_dir_all(project.join("src"))?;
    let mut seen = BTreeSet::new();
    let mut expected_files = BTreeSet::new();
    let mut main = String::from("//! A generated, compiler-audited Rust language showcase.\n");
    main.push_str("#![allow(dead_code, unused_imports, unused_variables, unused_parens, redundant_semicolons)] // Permit deliberately illustrative forms.\n");
    main.push_str(
        "#![warn(unused_must_use)] // Request warnings for unused results of must-use values.\n",
    );
    main.push_str("#![deny(unsafe_op_in_unsafe_fn)] // Require unsafe operations to have explicit unsafe blocks.\n");
    main.push_str("#![forbid(unused_unsafe)] // Forbid unnecessary unsafe blocks in this crate.\n");
    main.push_str(
        "#![recursion_limit = \"128\"] // Set the compiler recursion limit for this crate.\n",
    );
    main.push_str("#![type_length_limit = \"1000000\"] // Declare a bound on type substitutions; enforcement depends on compiler settings.\n");
    main.push_str("#![crate_name = \"pangram\"] // Give the crate the same name used by its manifest and compiler invocation.\n");
    main.push_str("#![crate_type = \"bin\"] // Declare this crate as an executable binary.\n");
    for example in &examples {
        ensure!(
            example.configuration == "stable",
            "unexpected catalog profile"
        );
        let ident: syn::Ident = syn::parse_str(&example.id)?;
        ensure!(seen.insert(example.id.clone()), "duplicate example ID");
        for dep in &example.prerequisites {
            ensure!(
                seen.contains(dep),
                "catalog must be topologically ordered: {} needs {dep}",
                example.id
            );
        }
        let mut source = String::new();
        for line in &example.lines {
            ensure!(
                !line.code.contains(['\n', '\r']) && !line.explanation.contains(['\n', '\r']),
                "catalog rows must each render to one physical line"
            );
            ensure!(
                line.explanation
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .count()
                    >= 3,
                "missing explanation in {}",
                example.id
            );
            source.push_str(&format!("{} // {}\n", line.code, line.explanation));
        }
        syn::parse_file(&source)?;
        ensure!(
            crate::lexical::observe("catalog", &source).1.is_empty(),
            "rendered example has lines without lexical comments"
        );
        let file = format!("{}.rs", example.id);
        expected_files.insert(file.clone());
        fs::write(project.join("src").join(file), source)?;
        main.push_str(&format!(
            "mod {ident}; // Load the {} examples from an external module.\n",
            example.id
        ));
    }
    main.push_str("fn main() { // The executable entry point runs each demonstration.\n");
    for example in &examples {
        let ident: syn::Ident = syn::parse_str(&example.id)?;
        let call = quote::quote!(#ident::demonstrate(););
        main.push_str(&format!(
            "    {call} // Run the {} demonstration and its assertions.\n",
            example.id
        ));
    }
    main.push_str(
        "    println!(\"pangram: ok\"); // Print the deterministic runtime verification marker.\n",
    );
    main.push_str("} // Close the executable entry point.\n");
    syn::parse_file(&main)?;
    fs::write(project.join("src/main.rs"), main)?;
    expected_files.insert("main.rs".into());
    for entry in fs::read_dir(project.join("src"))? {
        let entry = entry?;
        ensure!(
            expected_files.contains(&entry.file_name().to_string_lossy().into_owned()),
            "unexpected existing file in generated source directory: {}; select an empty destination or reconcile it",
            entry.path().display()
        );
    }
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"pangram\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    // Intent is navigation metadata only. The auditor does not use this as proof.
    write_json(&project.join("example-intent.json"), &examples)?;
    generate_negative(repository, project)
}

fn generate_negative(repository: &Path, project: &Path) -> Result<()> {
    let negative: Vec<Example> = read_json(&repository.join("catalog/negative.json"))?;
    fs::create_dir_all(project.join("compile-fail"))?;
    for example in negative {
        ensure!(
            example.configuration == "compile-fail",
            "unexpected negative-fixture configuration"
        );
        let _: syn::Ident = syn::parse_str(&example.id)?;
        let mut source = String::new();
        for line in example.lines {
            ensure!(
                !line.code.contains(['\n', '\r']) && !line.explanation.contains(['\n', '\r']),
                "negative rows must be single lines"
            );
            source.push_str(&format!("{} // {}\n", line.code, line.explanation));
        }
        ensure!(
            crate::lexical::observe("fixture", &source).1.is_empty(),
            "negative fixture lacks annotations"
        );
        fs::write(
            project
                .join("compile-fail")
                .join(format!("{}.rs", example.id)),
            source,
        )?;
    }
    Ok(())
}

fn generate_freestanding(repository: &Path, project: &Path) -> Result<()> {
    let examples: Vec<Example> = read_json(&repository.join("catalog/stable_freestanding.json"))?;
    ensure!(
        examples.len() == 1 && examples[0].configuration == "stable_freestanding",
        "freestanding catalog must contain its single crate example"
    );
    let mut source = String::new();
    for line in &examples[0].lines {
        ensure!(
            !line.code.contains(['\n', '\r']) && !line.explanation.contains(['\n', '\r']),
            "catalog rows must each render to one physical line"
        );
        source.push_str(&format!("{} // {}\n", line.code, line.explanation));
    }
    syn::parse_file(&source)?;
    ensure!(
        crate::lexical::observe("catalog", &source).1.is_empty(),
        "freestanding source lacks explanatory comments"
    );
    fs::create_dir_all(project.join("src"))?;
    for entry in fs::read_dir(project.join("src"))? {
        let entry = entry?;
        ensure!(
            entry.file_name() == "main.rs",
            "unexpected existing source file: {}; choose an empty destination",
            entry.path().display()
        );
    }
    fs::write(project.join("src/main.rs"), source)?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"pangram\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[profile.dev]\npanic = \"abort\"\n\n[profile.release]\npanic = \"abort\"\n\n[workspace]\n",
    )?;
    write_json(&project.join("example-intent.json"), &examples)?;
    generate_negative(repository, project)
}
