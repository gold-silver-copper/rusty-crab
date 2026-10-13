use crate::{audit, model::*};
use anyhow::{Result, ensure};
use std::{fs, path::Path};

pub struct NegativeSpec {
    pub name: &'static str,
    pub requirement: &'static str,
    pub diagnostic: &'static str,
    pub description: &'static str,
}

pub const NEGATIVE: &[NegativeSpec] = &[
    NegativeSpec {
        name: "use_after_move",
        requirement: "semantic.use_after_move_rejected",
        diagnostic: "E0382",
        description: "The compiler rejects use of a value after its ownership has moved",
    },
    NegativeSpec {
        name: "conflicting_borrows",
        requirement: "semantic.conflicting_borrows_rejected",
        diagnostic: "E0502",
        description: "The compiler rejects an incompatible mutable borrow while a shared borrow remains live",
    },
];

pub fn observe(
    root: &Path,
    work: &Path,
    inputs: &mut crate::inputs::FileInputs,
    environment: &mut std::collections::BTreeMap<String, Option<String>>,
    profile: crate::profile::Profile,
) -> Result<(Vec<Evidence>, Vec<Validation>)> {
    let mut evidence = Vec::new();
    let mut validations = Vec::new();
    for spec in NEGATIVE {
        let file = format!("compile-fail/{}.rs", spec.name);
        let path = root.join(&file);
        if !path.exists() {
            validations.push(Validation {
                id: spec.requirement.into(),
                stage: "expected_rejection".into(),
                passed: false,
                detail: format!("Missing fixture {file}; expected {}", spec.diagnostic),
            });
            continue;
        }
        let (output, observation) = audit::run_worker(
            root,
            &path,
            &work.join(format!("{}-observations.json", spec.name)),
            &work.join(format!("{}.rmeta", spec.name)),
            "__diagnostic_worker",
            profile,
        )?;
        inputs.merge(observation.inputs);
        audit::merge_environment(environment, observation.environment)?;
        let mut coded_errors = Vec::new();
        let mut witnesses = Vec::new();
        let mut other_primary_errors = false;
        for line in String::from_utf8(output.stderr)?.lines() {
            let diagnostic: serde_json::Value = serde_json::from_str(line)?;
            if diagnostic["level"] != "error" {
                continue;
            }
            if let Some(code) = diagnostic["code"]["code"].as_str() {
                coded_errors.push(code.to_owned());
            }
            let spans = diagnostic["spans"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing diagnostic spans"))?;
            if diagnostic["code"].is_null() && spans.iter().any(|s| s["is_primary"] == true) {
                other_primary_errors = true;
            }
            if diagnostic["code"]["code"] != spec.diagnostic {
                continue;
            }
            for span in spans.iter().filter(|s| s["is_primary"] == true) {
                let reported = span["file_name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing diagnostic file"))?;
                if Path::new(reported).canonicalize()? != path.canonicalize()? {
                    continue;
                }
                let number = |key| -> Result<usize> {
                    Ok(span[key]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("missing diagnostic coordinate {key}"))?
                        as usize)
                };
                witnesses.push(Evidence {
                    requirement: spec.requirement.into(),
                    file: file.clone(),
                    start: number("byte_start")?,
                    end: number("byte_end")?,
                    line: number("line_start")?,
                    column: number("column_start")?,
                    stage: "expected_rejection".into(),
                    detector: "rustc_diagnostic_v1".into(),
                    origin: "source".into(),
                    configuration: String::new(),
                    macro_backtrace: Vec::new(),
                    witness: None,
                });
            }
        }
        let passed = !output.status.success()
            && !other_primary_errors
            && coded_errors == [spec.diagnostic]
            && !witnesses.is_empty();
        validations.push(Validation {
            id: spec.requirement.into(),
            stage: "expected_rejection".into(),
            passed,
            detail: format!(
                "Expected exactly {}; observed codes: {}",
                spec.diagnostic,
                coded_errors.join(", ")
            ),
        });
        if passed {
            evidence.extend(witnesses);
        }
    }
    Ok((evidence, validations))
}

pub fn requirements(repository: &Path) -> Result<Vec<Requirement>> {
    let mut requirements = Vec::new();
    for spec in NEGATIVE {
        let path = format!(
            "compiler/rustc_error_codes/src/error_codes/{}.md",
            spec.diagnostic
        );
        let contents = fs::read(repository.join("vendor/rust").join(&path))?;
        ensure!(!contents.is_empty(), "empty diagnostic provenance");
        requirements.push(Requirement {
            id: spec.requirement.into(),
            description: spec.description.into(),
            dimension: "semantic".into(),
            origin: "manually_specified".into(),
            provenance: Provenance {
                path,
                line: 1,
                revision: COMMIT.into(),
                shape_hash: hash(&contents),
            },
            applicability: "applicable".into(),
            reason: format!(
                "Selected negative semantic witness based on pinned compiler explanation {}",
                spec.diagnostic
            ),
            detector: Some("rustc_diagnostic_v1".into()),
            required_witnesses: Default::default(),
        });
    }
    Ok(requirements)
}
