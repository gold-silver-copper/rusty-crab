use crate::profile::Profile;
use crate::{inventory, lexical, model::*};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

pub fn rustc() -> Command {
    let mut command = Command::new(Path::new(env!("CRAB_SYSROOT")).join("bin/rustc"));
    command.env_remove("RUSTC_BOOTSTRAP");
    command
}

pub fn configuration(profile: Profile) -> Result<Configuration> {
    let output = rustc().arg("-vV").output()?;
    ensure!(output.status.success(), "cannot identify pinned compiler");
    let version = String::from_utf8(output.stdout)?;
    let field = |key: &str| -> Result<String> {
        version
            .lines()
            .find_map(|s| s.strip_prefix(key))
            .map(str::to_owned)
            .with_context(|| format!("missing compiler identity field {key}"))
    };
    ensure!(
        field("commit-hash: ")? == COMMIT,
        "compiler revision differs from inventory"
    );
    let cfg = rustc().arg("--print=cfg").args(profile.flags()).output()?;
    ensure!(
        cfg.status.success(),
        "cannot obtain compiler conditional-compilation settings"
    );
    let mut cfg: Vec<_> = String::from_utf8(cfg.stdout)?
        .lines()
        .map(str::to_owned)
        .collect();
    cfg.sort();
    Ok(Configuration {
        compiler_commit: COMMIT.into(),
        release: field("release: ")?,
        edition: "2024".into(),
        target: field("host: ")?,
        profile: profile.name().into(),
        crate_type: "bin".into(),
        cfg,
        features: vec![],
        flags: profile.flags(),
    })
}

pub fn source_tree(root: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            ensure!(
                !entry.file_type()?.is_symlink(),
                "source-tree symlinks need an explicit containment policy: {}",
                path.display()
            );
            if entry.file_type()?.is_dir() {
                if entry.file_name() != "target" {
                    walk(root, &path, out)?;
                }
            } else {
                let rel = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, hash(fs::read(path)?));
            }
        }
        Ok(())
    }
    let mut hashes = BTreeMap::new();
    walk(root, root, &mut hashes)?;
    ensure!(
        hashes.contains_key("src/main.rs"),
        "audited project needs src/main.rs"
    );
    Ok(hashes)
}

pub fn validate_inputs(root: &Path, inputs: &crate::inputs::FileInputs) -> Result<()> {
    ensure!(
        inputs.inconsistent.is_empty(),
        "compiler observed inconsistent inputs: {}",
        inputs.inconsistent.join("; ")
    );
    for (path, expected) in &inputs.files {
        ensure!(
            hash(fs::read(root.join(path))?) == *expected,
            "compiler input changed since inspection: {path}"
        );
    }
    for (path, expected) in &inputs.probes {
        ensure!(
            root.join(path).exists() == *expected,
            "compiler file probe changed since inspection: {path}"
        );
    }
    Ok(())
}

pub fn validate_environment(environment: &BTreeMap<String, Option<String>>) -> Result<()> {
    for (name, expected) in environment {
        // All compiler subprocesses remove this variable, even when Cargo supplied
        // the auditor's narrowly scoped bootstrap setting to the parent process.
        let actual = if name == "RUSTC_BOOTSTRAP" {
            None
        } else {
            match std::env::var(name) {
                Ok(value) => Some(hash(value)),
                Err(std::env::VarError::NotPresent) => None,
                Err(std::env::VarError::NotUnicode(_)) => {
                    anyhow::bail!("compiler environment input is no longer UTF-8: {name}")
                }
            }
        };
        ensure!(
            &actual == expected,
            "compiler environment input changed since inspection: {name}"
        );
    }
    Ok(())
}

pub fn merge_environment(
    into: &mut BTreeMap<String, Option<String>>,
    other: BTreeMap<String, Option<String>>,
) -> Result<()> {
    for (name, value) in other {
        if let Some(previous) = into.insert(name.clone(), value.clone()) {
            ensure!(
                previous == value,
                "compiler environment input changed between invocations: {name}"
            );
        }
    }
    Ok(())
}

pub fn run_worker(
    root: &Path,
    source: &Path,
    observations: &Path,
    artifact: &Path,
    mode: &str,
    profile: Profile,
) -> Result<(std::process::Output, CompilerObservation)> {
    let output = Command::new(std::env::current_exe()?)
        .env_remove("RUSTC_BOOTSTRAP")
        .arg(mode)
        .arg(profile.name())
        .arg(root)
        .arg(source)
        .arg(observations)
        .arg(artifact)
        .output()
        .context("running compiler auditor subprocess")?;
    let observation: CompilerObservation = read_json(observations).with_context(|| {
        format!(
            "compiler worker failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    ensure!(
        observation.schema == OBSERVATION_SCHEMA && observation.compiler_commit == COMMIT,
        "unexpected compiler observation identity"
    );
    ensure!(
        observation.configuration == configuration(profile)?,
        "compiler worker configuration differs from requested profile"
    );
    validate_inputs(root, &observation.inputs)?;
    validate_environment(&observation.environment)?;
    Ok((output, observation))
}

pub fn run(
    repository: &Path,
    project: &Path,
    inventory: &Inventory,
    execute: bool,
    profile: Profile,
) -> Result<Audit> {
    validate_inventory(inventory)?;
    let fresh = inventory::extract(repository)?;
    ensure!(
        *inventory == fresh,
        "inventory differs from current compiler sources or interpretation rules; re-extract and review"
    );
    let config = configuration(profile)?;
    let configuration_id = canonical_hash(&config)?;
    let root = project.canonicalize()?;
    let sources = source_tree(&root)?;
    let mut evidence = Vec::new();
    let mut issues = Vec::new();
    for path in sources.keys().filter(|p| p.ends_with(".rs")) {
        let source = fs::read_to_string(root.join(path))?;
        let (observations, missing) = lexical::observe(path, &source);
        evidence.extend(observations);
        for line in missing {
            issues.push(format!("Missing explanatory comment at {path}:{line}"));
        }
    }
    let annotations_valid = issues.is_empty();
    let temp = tempfile::tempdir()?;
    let observations = temp.path().join("observations.json");
    let (output, observation) = run_worker(
        &root,
        &root.join("src/main.rs"),
        &observations,
        &temp.path().join("pangram.rmeta"),
        "__compiler_worker",
        profile,
    )?;
    let mut accepted = output.status.success() && observation.accepted;
    let mut compiler_inputs = observation.inputs;
    let mut compiler_environment = observation.environment;
    if !accepted {
        issues.push(
            "Compiler rejected the audited program; syntactic acceptance evidence is invalid"
                .into(),
        );
        issues.push(
            String::from_utf8_lossy(&output.stderr)
                .replace(&root.to_string_lossy().to_string(), "$PROJECT")
                .replace(&temp.path().to_string_lossy().to_string(), "$WORK"),
        );
    }
    if accepted {
        evidence.extend(observation.evidence);
    }
    let (negative_evidence, mut validations) = crate::fixtures::observe(
        &root,
        temp.path(),
        &mut compiler_inputs,
        &mut compiler_environment,
        profile,
    )?;
    evidence.extend(negative_evidence);
    let mut ran = false;
    if !profile.runs() && accepted {
        let object = temp.path().join("pangram.o");
        let (compiled, generated) = run_worker(
            &root,
            &root.join("src/main.rs"),
            &temp.path().join("object-observations.json"),
            &object,
            "__object_worker",
            profile,
        )?;
        compiler_inputs.merge(generated.inputs);
        merge_environment(&mut compiler_environment, generated.environment)?;
        let passed = compiled.status.success()
            && generated.accepted
            && fs::metadata(&object).is_ok_and(|m| m.len() > 0);
        validations.push(Validation {
            id: "compiler.object".into(),
            stage: "codegen".into(),
            passed,
            detail: "Emit a nonempty object file; linking and execution are outside this profile's validation contract".into(),
        });
        if !passed {
            accepted = false;
            issues.push(format!(
                "Object generation failed: {}",
                String::from_utf8_lossy(&compiled.stderr)
                    .replace(&root.to_string_lossy().to_string(), "$PROJECT")
                    .replace(&temp.path().to_string_lossy().to_string(), "$WORK")
            ));
        }
    }
    if execute && profile.runs() && accepted {
        let executable = temp.path().join("pangram");
        let (compiled, linked) = run_worker(
            &root,
            &root.join("src/main.rs"),
            &temp.path().join("link-observations.json"),
            &executable,
            "__link_worker",
            profile,
        )?;
        compiler_inputs.merge(linked.inputs);
        merge_environment(&mut compiler_environment, linked.environment)?;
        if compiled.status.success() && linked.accepted {
            let result = Command::new(&executable)
                .current_dir(temp.path())
                .output()?;
            ran = result.status.success() && result.stdout == b"pangram: ok\n";
        }
        if !ran {
            issues.push(
                "Runtime validation failed: expected successful exit and exactly 'pangram: ok'"
                    .into(),
            );
        }
    }
    ensure!(
        source_tree(&root)? == sources,
        "source tree changed during audit"
    );
    validate_inputs(&root, &compiler_inputs)?;
    validate_environment(&compiler_environment)?;
    for e in &mut evidence {
        e.configuration.clone_from(&configuration_id);
        // Validate offsets against precisely the source bytes included in the audit.
        ensure!(
            sources.contains_key(&e.file),
            "observation references unhashed source {}",
            e.file
        );
        let len = fs::metadata(root.join(&e.file))?.len() as usize;
        ensure!(
            e.start <= e.end && e.end <= len,
            "invalid compiler source span"
        );
    }
    evidence.sort();
    evidence.dedup();
    let mut by_requirement: BTreeMap<String, Vec<Evidence>> = BTreeMap::new();
    for e in evidence {
        by_requirement
            .entry(e.requirement.clone())
            .or_default()
            .push(e);
    }
    let outcomes = inventory
        .requirements
        .iter()
        .map(|r| {
            let all = by_requirement.remove(&r.id).unwrap_or_default();
            let qualified: Vec<_> = all
                .into_iter()
                .filter(|e| {
                    e.origin == "source"
                        && r.detector.as_deref() == Some(&e.detector)
                        && (r.required_witnesses.is_empty()
                            || e.witness
                                .as_ref()
                                .is_some_and(|w| r.required_witnesses.contains_key(w)))
                        && ((r.dimension == "lexical" && e.stage == "lexical_presence")
                            || (accepted
                                && r.dimension == "syntactic"
                                && ["expanded_ast", "resolved_ast", "compiler_expansion"]
                                    .contains(&e.stage.as_str()))
                            || (r.dimension == "semantic" && e.stage == "expected_rejection")
                            || (accepted && r.dimension == "semantic" && e.stage == "typechecked"))
                })
                .collect();
            let missing_witnesses: Vec<_> = r
                .required_witnesses
                .keys()
                .filter(|w| !qualified.iter().any(|e| e.witness.as_ref() == Some(w)))
                .cloned()
                .collect();
            let status = match r.applicability.as_str() {
                "inapplicable" => "inapplicable",
                "unclassified" => "unclassified",
                _ if r.detector.is_none() => "unsupported",
                _ if qualified.is_empty() || !missing_witnesses.is_empty() => "missing",
                _ => "covered",
            };
            Outcome {
                requirement: r.clone(),
                status: status.into(),
                evidence: qualified,
                missing_witnesses,
            }
        })
        .collect();
    issues.extend(inventory.limitations.clone());
    Ok(Audit {
        schema: AUDIT_SCHEMA,
        auditor_hash: env!("CRAB_AUDITOR_HASH").into(),
        configuration: config,
        inventory_hash: canonical_hash(inventory)?,
        source_hashes: sources,
        compiler_inputs,
        compiler_environment,
        accepted,
        ran,
        annotations_valid,
        issues,
        validations,
        outcomes,
    })
}

pub fn complete(audit: &Audit) -> bool {
    audit.accepted
        && match Profile::parse(&audit.configuration.profile) {
            Ok(Profile::Stable) => audit.ran,
            Ok(Profile::Freestanding) => {
                !audit.ran
                    && audit
                        .validations
                        .iter()
                        .any(|v| v.id == "compiler.object" && v.stage == "codegen" && v.passed)
            }
            Err(_) => false,
        }
        && audit.annotations_valid
        && audit.issues.is_empty()
        && audit.validations.iter().all(|v| v.passed)
        && !audit.outcomes.is_empty()
        && audit
            .outcomes
            .iter()
            .all(|o| ["covered", "inapplicable"].contains(&o.status.as_str()))
}

pub fn summary(audit: &Audit) -> String {
    let mut counts: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for o in &audit.outcomes {
        *counts
            .entry(&o.requirement.dimension)
            .or_default()
            .entry(&o.status)
            .or_default() += 1;
    }
    let mut text = format!(
        "Rust {} ({}) / {} / edition {} / profile {}\n",
        audit.configuration.release,
        audit.configuration.compiler_commit,
        audit.configuration.target,
        audit.configuration.edition,
        audit.configuration.profile
    );
    for (dimension, statuses) in counts {
        text.push_str(&format!(
            "{dimension}: {}\n",
            statuses
                .into_iter()
                .map(|(s, n)| format!("{n} {s}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let grammar: Vec<_> = audit
        .outcomes
        .iter()
        .filter(|o| {
            o.requirement.id.starts_with("grammar.") && !o.requirement.required_witnesses.is_empty()
        })
        .collect();
    if !grammar.is_empty() {
        let required: usize = grammar
            .iter()
            .map(|o| o.requirement.required_witnesses.len())
            .sum();
        let observed: usize = grammar
            .iter()
            .map(|o| o.requirement.required_witnesses.len() - o.missing_witnesses.len())
            .sum();
        text.push_str(&format!("Extracted Reference grammar: {observed}/{required} local forms across {} productions; unsupported notation remains a separate gap.\n", grammar.len()));
    }
    text.push_str(&format!(
        "Compiler accepted: {}; runtime passed: {}; annotations valid: {}\n",
        audit.accepted, audit.ran, audit.annotations_valid
    ));
    if audit.configuration.profile == "stable_freestanding" {
        text.push_str("Validation contract: metadata and object generation only; no linking or runtime claim.\n");
    }
    for validation in &audit.validations {
        text.push_str(&format!(
            "{}: {} ({})\n",
            validation.id,
            if validation.passed {
                "passed"
            } else {
                "failed"
            },
            validation.detail
        ));
    }
    for issue in &audit.issues {
        text.push_str(&format!("Gap: {issue}\n"));
    }
    text.push_str(if complete(audit) {
        "Inventory coverage is complete for this configuration.\n"
    } else {
        "Coverage is INCOMPLETE. Unsupported and unclassified requirements remain gaps.\n"
    });
    text
}
