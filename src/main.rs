#![feature(rustc_private)]
#![allow(non_snake_case)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_lexer;
extern crate rustc_middle;
extern crate rustc_parse;
extern crate rustc_session;
extern crate rustc_span;

include!(concat!(env!("OUT_DIR"), "/classifiers.rs"));

mod audit;
mod compiler;
mod compiler_source;
mod features;
mod fixtures;
mod generate;
mod grammar;
mod grammar_observer;
mod inputs;
mod inventory;
mod lexical;
mod model;
mod profile;
mod reference;
mod semantic;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if !args.first().is_some_and(|s| s == "__compiler_worker") {
        if let Some(command) = args.first().map(String::as_str) {
            if ["generate", "audit", "report"].contains(&command) {
                return project_command(repository, command, &args[1..]);
            }
        }
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["inventory", "extract"] => {
            let inventory = inventory::extract(repository)?;
            model::write_json(std::path::Path::new("artifacts/inventory.json"), &inventory)?;
            println!("Extracted {} requirements", inventory.requirements.len());
        }
        ["inventory", "diff", old, new] => {
            let a = model::read_json(std::path::Path::new(old))?;
            let b = model::read_json(std::path::Path::new(new))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&inventory::diff(&a, &b))?
            );
        }
        [
            mode @ ("__compiler_worker"
            | "__diagnostic_worker"
            | "__link_worker"
            | "__object_worker"),
            profile,
            root,
            source,
            output,
            artifact,
        ] => {
            let status = compiler::worker(
                std::path::Path::new(root),
                std::path::Path::new(source),
                std::path::Path::new(output),
                std::path::Path::new(artifact),
                mode,
                profile::Profile::parse(profile)?,
            )?;
            if status != std::process::ExitCode::SUCCESS {
                std::process::exit(1);
            }
        }
        ["--help"] | [] => println!(
            "rusty-crab inventory extract\nrusty-crab inventory diff OLD NEW\nrusty-crab generate|audit|report [--profile stable|stable_freestanding] [--project DIR] [--artifacts DIR] [--inventory FILE] [--strict] [--no-run]"
        ),
        _ => anyhow::bail!("unknown command; run with --help"),
    }
    Ok(())
}

fn project_command(
    repository: &std::path::Path,
    command: &str,
    args: &[String],
) -> anyhow::Result<()> {
    use std::path::PathBuf;
    let mut project: Option<PathBuf> = None;
    let mut artifacts: Option<PathBuf> = None;
    let mut profile = profile::Profile::Stable;
    let mut inventory_path = PathBuf::from("artifacts/inventory.json");
    let mut strict = false;
    let mut execute = true;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => {
                project = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--project needs a directory"))?
                        .into(),
                )
            }
            "--artifacts" => {
                artifacts = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--artifacts needs a directory"))?
                        .into(),
                )
            }
            "--inventory" => {
                inventory_path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--inventory needs a file"))?
                    .into()
            }
            "--profile" => {
                profile = profile::Profile::parse(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--profile needs a name"))?,
                )?
            }
            "--strict" => strict = true,
            "--no-run" => execute = false,
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    let project = project.unwrap_or_else(|| PathBuf::from("generated").join(profile.name()));
    let artifacts = artifacts.unwrap_or_else(|| {
        if profile == profile::Profile::Stable {
            PathBuf::from("artifacts")
        } else {
            PathBuf::from("artifacts").join(profile.name())
        }
    });
    if command == "generate" {
        generate::generate(repository, &project, profile)?;
        println!("Generated annotated project at {}", project.display());
        return Ok(());
    }
    let inventory: model::Inventory = model::read_json(&inventory_path)?;
    let report = if command == "audit" {
        let report = audit::run(repository, &project, &inventory, execute, profile)?;
        model::write_json(&artifacts.join("audit.json"), &report)?;
        std::fs::write(artifacts.join("report.txt"), detailed_report(&report))?;
        report
    } else {
        let report: model::Audit = model::read_json(&artifacts.join("audit.json"))?;
        anyhow::ensure!(
            report.auditor_hash == env!("CRAB_AUDITOR_HASH"),
            "audit was produced by a different auditor implementation; rerun audit"
        );
        anyhow::ensure!(
            inventory == inventory::extract(repository)?,
            "inventory is stale"
        );
        anyhow::ensure!(
            report.schema == model::AUDIT_SCHEMA
                && report.configuration == audit::configuration(profile)?,
            "audit configuration is stale"
        );
        anyhow::ensure!(
            report.inventory_hash == model::canonical_hash(&inventory)?,
            "audit inventory is stale"
        );
        anyhow::ensure!(
            report.source_hashes == audit::source_tree(&project)?,
            "audit source hashes are stale; rerun audit"
        );
        audit::validate_inputs(&project, &report.compiler_inputs)?;
        audit::validate_environment(&report.compiler_environment)?;
        report
    };
    print!("{}", audit::summary(&report));
    if strict {
        anyhow::ensure!(audit::complete(&report), "strict completeness check failed");
    }
    anyhow::ensure!(report.accepted, "compiler validation failed");
    if execute && profile.runs() {
        anyhow::ensure!(report.ran, "runtime validation failed or was not performed");
    }
    Ok(())
}

fn detailed_report(report: &model::Audit) -> String {
    let mut text = audit::summary(report);
    text.push_str("\nRequirement evidence\n");
    for outcome in &report.outcomes {
        text.push_str(&format!(
            "{} {}: {}\n",
            outcome.status, outcome.requirement.id, outcome.requirement.reason
        ));
        for e in &outcome.evidence {
            text.push_str(&format!(
                "  {}:{}:{} bytes {}..{} [{}; {}; {}]\n",
                e.file, e.line, e.column, e.start, e.end, e.stage, e.detector, e.origin
            ));
            if let Some(witness) = &e.witness {
                text.push_str(&format!("    Form: {witness}\n"));
            }
        }
        if !outcome.missing_witnesses.is_empty() {
            text.push_str(&format!(
                "  Missing forms: {}\n",
                outcome.missing_witnesses.join(", ")
            ));
            for form in &outcome.missing_witnesses {
                if let Some(description) = outcome.requirement.required_witnesses.get(form) {
                    text.push_str(&format!("    {form}: {description}\n"));
                }
            }
        }
    }
    text
}
