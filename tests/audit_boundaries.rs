use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rusty-crab"))
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn check(project: &Path, artifacts: &Path) -> Value {
    let output = binary()
        .current_dir(root())
        .args(["audit", "--no-run", "--project"])
        .arg(project)
        .arg("--artifacts")
        .arg(artifacts)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(artifacts.join("audit.json")).unwrap()).unwrap()
}

fn profile_command(command: &str, profile: &str, project: &Path, artifacts: &Path) -> Command {
    let mut cmd = binary();
    cmd.current_dir(root())
        .arg(command)
        .args(["--profile", profile, "--project"])
        .arg(project)
        .arg("--artifacts")
        .arg(artifacts);
    cmd
}

#[test]
fn freestanding_profile_generates_an_object_without_a_runtime_claim() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let profile = "stable_freestanding";
    success(
        profile_command("generate", profile, &project, &artifacts)
            .output()
            .unwrap(),
    );
    let source = fs::read(project.join("src/main.rs")).unwrap();
    success(
        profile_command("generate", profile, &project, &artifacts)
            .output()
            .unwrap(),
    );
    assert_eq!(source, fs::read(project.join("src/main.rs")).unwrap());
    let object = dir.path().join("standalone.o");
    success(
        Command::new("rustc")
            .current_dir(root())
            .env_remove("RUSTC_BOOTSTRAP")
            .args([
                "--edition=2024",
                "--crate-name=pangram",
                "--crate-type=bin",
                "-Cpanic=abort",
                "--emit=obj",
            ])
            .arg(project.join("src/main.rs"))
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap(),
    );
    assert!(fs::metadata(object).unwrap().len() > 0);
    success(
        profile_command("audit", profile, &project, &artifacts)
            .output()
            .unwrap(),
    );
    let before = fs::read(artifacts.join("audit.json")).unwrap();
    success(
        profile_command("audit", profile, &project, &artifacts)
            .arg("--no-run")
            .output()
            .unwrap(),
    );
    assert_eq!(before, fs::read(artifacts.join("audit.json")).unwrap());
    let report: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(report["accepted"], true);
    assert_eq!(report["ran"], false);
    assert_eq!(report["annotations_valid"], true);
    assert_eq!(report["configuration"]["profile"], profile);
    assert!(
        report["configuration"]["flags"]
            .as_array()
            .unwrap()
            .contains(&Value::from("-Cpanic=abort"))
    );
    assert!(
        report["configuration"]["cfg"]
            .as_array()
            .unwrap()
            .contains(&Value::from("panic=\"abort\""))
    );
    let object = report["validations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "compiler.object")
        .unwrap();
    assert_eq!(object["stage"], "codegen");
    assert_eq!(object["passed"], true);
    for name in ["no_std", "no_main", "no_implicit_prelude", "panic_handler"] {
        assert_eq!(status(&report, &format!("attribute.{name}")), "covered");
    }
    for name in [
        "semantic.use_after_move_rejected",
        "semantic.conflicting_borrows_rejected",
    ] {
        assert_eq!(status(&report, name), "covered");
    }
    success(
        profile_command("report", profile, &project, &artifacts)
            .output()
            .unwrap(),
    );
    let strict = profile_command("report", profile, &project, &artifacts)
        .arg("--strict")
        .output()
        .unwrap();
    assert!(!strict.status.success());
    assert!(String::from_utf8_lossy(&strict.stderr).contains("strict completeness check failed"));
    let wrong = profile_command("report", "stable", &project, &artifacts)
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("configuration is stale"));
}

#[test]
fn actual_profile_flags_control_active_syntax_and_rejection_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "#[cfg(panic = \"abort\")] fn selected() { loop { break; } } // Only the abort configuration compiles this loop.\n",
            "#[cfg(panic = \"unwind\")] fn selected() {} // The normal stable configuration compiles an empty function.\n",
            "fn main() { selected(); } // The same source is valid under both reviewed profiles.\n",
        ),
    );
    fs::create_dir_all(project.join("compile-fail")).unwrap();
    fs::write(project.join("compile-fail/use_after_move.rs"), concat!(
        "#[cfg(panic = \"abort\")] fn main() { let x = String::new(); let y = x; let _ = x.len(); } // Only abort mode contains the intended ownership error.\n",
        "#[cfg(panic = \"unwind\")] fn main() {} // Under the other configuration this fixture is accepted.\n",
    )).unwrap();
    let stable = check(&project, &artifacts);
    assert_eq!(status(&stable, "ast.ExprKind.Loop"), "missing");
    assert_eq!(
        status(&stable, "semantic.use_after_move_rejected"),
        "missing"
    );
    success(
        profile_command("audit", "stable_freestanding", &project, &artifacts)
            .output()
            .unwrap(),
    );
    let free: Value =
        serde_json::from_slice(&fs::read(artifacts.join("audit.json")).unwrap()).unwrap();
    assert_eq!(status(&free, "ast.ExprKind.Loop"), "covered");
    assert_eq!(status(&free, "semantic.use_after_move_rejected"), "covered");
    let witness = |r: &Value| -> String {
        r["outcomes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["requirement"]["id"] == "lex.TokenKind.Ident")
            .unwrap()["evidence"][0]["configuration"]
            .as_str()
            .unwrap()
            .into()
    };
    assert_ne!(witness(&stable), witness(&free));
}

#[test]
fn freestanding_object_generation_failure_cannot_pass_as_metadata_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "core::arch::global_asm!(\".error \\\"deliberate object failure\\\"\"); // This assembler directive fails only when an object is emitted.\n",
            "fn main() { loop { break; } } // Rust syntax and type checking succeed before assembly.\n",
        ),
    );
    let metadata = check(&project, &artifacts);
    assert_eq!(metadata["accepted"], true);
    assert_eq!(status(&metadata, "ast.ExprKind.Loop"), "covered");
    let output = profile_command("audit", "stable_freestanding", &project, &artifacts)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: Value =
        serde_json::from_slice(&fs::read(artifacts.join("audit.json")).unwrap()).unwrap();
    assert_eq!(report["accepted"], false);
    assert_eq!(report["ran"], false);
    assert_eq!(status(&report, "ast.ExprKind.Loop"), "missing");
    assert!(
        report["validations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == "compiler.object" && v["passed"] == false)
    );
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("deliberate object failure"))
    );
}

fn status<'a>(report: &'a Value, id: &str) -> &'a str {
    report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["requirement"]["id"] == id)
        .unwrap()["status"]
        .as_str()
        .unwrap()
}

fn put(project: &Path, source: &str) {
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/main.rs"), source).unwrap();
}

#[test]
fn deleting_the_only_loop_loses_coverage_even_with_forged_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { loop { break; } } // A source-written loop exits immediately.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "ast.ExprKind.Loop"), "covered");
    let evidence = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["requirement"]["id"] == "ast.ExprKind.Loop")
        .unwrap()["evidence"]
        .as_array()
        .unwrap();
    assert_eq!(evidence[0]["file"], "src/main.rs");
    assert_eq!(evidence[0]["start"], 12);
    assert_eq!(evidence[0]["line"], 1);
    put(
        &project,
        "fn main() { let _ = \"loop { break; }\"; } // Claimed feature: ast.ExprKind.Loop.\n",
    );
    fs::write(
        project.join("example-intent.json"),
        r#"{"covered":["ast.ExprKind.Loop"]}"#,
    )
    .unwrap();
    assert_eq!(
        status(&check(&project, &artifacts), "ast.ExprKind.Loop"),
        "missing"
    );
}

#[test]
fn cfg_and_macro_expansions_do_not_masquerade_as_source_loops() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "#[cfg(any())] fn disabled() { loop {} } // This function is inactive.\nmacro_rules! generated { () => { loop { break; } } } // Generate a loop during expansion.\nfn main() { generated!(); } // Invoke the macro without writing a loop here.\n",
    );
    assert_eq!(
        status(&check(&project, &artifacts), "ast.ExprKind.Loop"),
        "missing"
    );
}

#[test]
fn external_module_inside_project_is_observed_but_outside_source_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "mod inside; // Load a source module from inside this project.\nfn main() { inside::run(); } // Execute the module function.\n",
    );
    fs::write(
        project.join("src/inside.rs"),
        "pub fn run() { loop { break; } } // This local module contains a loop.\n",
    )
    .unwrap();
    assert_eq!(
        status(&check(&project, &artifacts), "ast.ExprKind.Loop"),
        "covered"
    );
    fs::remove_file(project.join("src/inside.rs")).unwrap();
    fs::write(
        dir.path().join("outside.rs"),
        "pub fn run() { loop { break; } } // This external input is outside the audited tree.\n",
    )
    .unwrap();
    put(
        &project,
        "#[path = \"../../outside.rs\"] mod outside; // Load a source file outside the audited tree.\nfn main() { outside::run(); } // Its source must not count toward this project's coverage.\n",
    );
    assert_eq!(
        status(&check(&project, &artifacts), "ast.ExprKind.Loop"),
        "missing"
    );
    fs::write(
        dir.path().join("outside.rs"),
        "pub fn run() {} // An external compilation input changed after auditing.\n",
    )
    .unwrap();
    let output = binary()
        .current_dir(root())
        .args(["report", "--no-run", "--project"])
        .arg(&project)
        .arg("--artifacts")
        .arg(&artifacts)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiler input changed"));
}

#[test]
fn stale_inputs_and_unknown_inventory_entries_fail() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(&project, "fn main() {} // A valid minimal entry point.\n");
    check(&project, &artifacts);
    put(
        &project,
        "fn main() { let _ = 1; } // The source has changed since the audit.\n",
    );
    let output = binary()
        .current_dir(root())
        .args(["report", "--no-run", "--project"])
        .arg(&project)
        .arg("--artifacts")
        .arg(&artifacts)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("stale"));
    let original: Value =
        serde_json::from_slice(&fs::read(root().join("artifacts/inventory.json")).unwrap())
            .unwrap();
    for mutate_commit in [true, false] {
        let mut inventory = original.clone();
        if mutate_commit {
            inventory["compiler_commit"] = "unrecognized compiler revision".into();
        } else {
            let mut candidate = inventory["requirements"][0].clone();
            candidate["id"] = "zz.unknown.CompilerVariant".into();
            inventory["requirements"]
                .as_array_mut()
                .unwrap()
                .push(candidate);
        }
        let path = dir.path().join("inventory.json");
        fs::write(&path, serde_json::to_vec(&inventory).unwrap()).unwrap();
        let output = binary()
            .current_dir(root())
            .args(["audit", "--strict", "--no-run", "--project"])
            .arg(&project)
            .arg("--artifacts")
            .arg(&artifacts)
            .arg("--inventory")
            .arg(path)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generation_and_auditing_are_repeatable_and_runtime_passes() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let generate = || {
        binary()
            .current_dir(root())
            .args(["generate", "--project"])
            .arg(&project)
            .output()
            .unwrap()
    };
    success(generate());
    let before = fs::read(project.join("src/expressions.rs")).unwrap();
    success(generate());
    assert_eq!(
        before,
        fs::read(project.join("src/expressions.rs")).unwrap()
    );
    // The auditor's configured lint cap must not hide errors in ordinary usage.
    let standalone = Command::new("cargo")
        .current_dir(root())
        .args(["run", "--offline", "--quiet", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .env_remove("RUSTC_BOOTSTRAP")
        .output()
        .unwrap();
    assert_eq!(standalone.stdout, b"pangram: ok\n");
    success(standalone);
    let audit = || {
        binary()
            .current_dir(root())
            .args(["audit", "--project"])
            .arg(&project)
            .arg("--artifacts")
            .arg(&artifacts)
            .output()
            .unwrap()
    };
    success(audit());
    let first = fs::read(artifacts.join("audit.json")).unwrap();
    success(audit());
    assert_eq!(first, fs::read(artifacts.join("audit.json")).unwrap());
    let report: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(report["accepted"], true);
    assert_eq!(report["ran"], true);
    assert_eq!(report["annotations_valid"], true);
    let interpreted: Vec<_> = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| {
            ["rustc_feature_syntax_v1", "rustc_reference_v1"]
                .iter()
                .any(|d| o["requirement"]["detector"] == *d)
        })
        .collect();
    assert!(!interpreted.is_empty());
    for outcome in interpreted {
        assert_eq!(
            outcome["status"], "covered",
            "{}",
            outcome["requirement"]["id"]
        );
        for evidence in outcome["evidence"].as_array().unwrap() {
            let expected = if [
                "grammar.IdentifierPattern",
                "grammar.PathPattern",
                "feature.bindings_after_at",
            ]
            .iter()
            .any(|id| outcome["requirement"]["id"] == *id)
            {
                "resolved_ast"
            } else {
                "expanded_ast"
            };
            assert_eq!(evidence["stage"], expected);
            assert_eq!(evidence["origin"], "source");
            assert!(evidence["witness"].is_string());
        }
    }
    let strict = binary()
        .current_dir(root())
        .args(["report", "--strict", "--project"])
        .arg(&project)
        .arg("--artifacts")
        .arg(&artifacts)
        .output()
        .unwrap();
    assert!(
        !strict.status.success(),
        "the unfinished full inventory must never claim completion"
    );
}

#[test]
fn accepted_feature_requires_every_named_form_and_actual_source_use() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { let mut x = 1; let _ = &raw const x; } // Only the const raw-borrow form occurs.\n",
    );
    let first = check(&project, &artifacts);
    let entry = first["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "feature.raw_ref_op")
        .unwrap();
    assert_eq!(entry["status"], "missing");
    assert_eq!(entry["missing_witnesses"], serde_json::json!(["mut"]));
    assert_eq!(entry["evidence"][0]["witness"], "const");

    put(
        &project,
        "fn main() { let mut x = 1; let _ = &raw const x; let _ = &raw mut x; } // Both raw-borrow forms are active source syntax.\n",
    );
    assert_eq!(
        status(&check(&project, &artifacts), "feature.raw_ref_op"),
        "covered"
    );

    put(
        &project,
        "#[cfg(any())] fn absent() { let mut x = 1; let _ = &raw mut x; } // Inactive code is not a witness.\nmacro_rules! raw_mut { () => {{ let mut x = 1; let _ = &raw mut x; }}; } // Expanded raw borrowing is not source-written use.\nfn main() { let mut x = 1; let _ = &raw const x; let _ = &mut x; raw_mut!(); } // An ordinary mutable borrow does not replace the missing raw form.\n",
    );
    fs::write(
        project.join("example-intent.json"),
        r#"{"feature.raw_ref_op":{"mut":"covered"}}"#,
    )
    .unwrap();
    let last = check(&project, &artifacts);
    assert_eq!(status(&last, "feature.raw_ref_op"), "missing");
    let entry = last["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "feature.raw_ref_op")
        .unwrap();
    assert_eq!(entry["missing_witnesses"], serde_json::json!(["mut"]));
}

#[test]
fn accepted_syntax_features_distinguish_nearby_but_different_constructs() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "struct Empty; enum E { Unit } // Unit forms do not demonstrate empty braced forms.\n",
            "struct Named { value: i32 } struct Pair(i32, i32); // A named struct is not a struct enum variant.\n",
            "const VALUE: i32 = 2; // A const item is not an inline const expression.\n",
            "fn fallible() -> Result<i32, ()> { Ok(1) } // A Result-returning function need not use question mark.\n",
            "fn main() { // Open a collection of deliberately similar negative controls.\n",
            "let mut x = 1; let _ = &x; let _ = &mut x; // Ordinary references are not raw borrows.\n",
            "let _ = Named { value: x }; let _ = [x][0]; // Explicit initialization and bracket indexing have different grammar.\n",
            "let (_, _) = (1, 2); let Pair(_, _) = Pair(1, 2); // Tuple patterns do not necessarily contain rest patterns.\n",
            "let [_, ..] = [1, 2]; let _ = (x); // A slice rest and an expression grouping are not tuple rest and grouped patterns.\n",
            "let _ = 0..2; let _ = match x { 0 | 1 => 1, _ => 0 }; // An exclusive range expression is not a range pattern.\n",
            "if true && { let _ = x; true } {} // A local declaration inside a Boolean block is not a let-chain.\n",
            "while false && { let _ = x; true } {} // A plain while condition is not while-let or a let-chain.\n",
            "let _ = if true { 1 } else { 2 }; // An else on an if expression is not a let-else declaration.\n",
            "let _ = \"if let Some(a) | Some(b) = x && true; ..=2; 1..=2; a.0; Ok(1)?\"; // Feature spellings inside this string are inert.\n",
            "} // Close the negative-control program.\n",
        ),
    );
    let report = check(&project, &artifacts);
    for outcome in report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["requirement"]["detector"] == "rustc_feature_syntax_v1")
    {
        assert_eq!(
            outcome["status"], "missing",
            "{}",
            outcome["requirement"]["id"]
        );
    }
}

#[test]
fn included_binary_and_text_files_outside_project_invalidate_saved_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let bytes = dir.path().join("bytes.bin");
    let text = dir.path().join("text.txt");
    fs::write(&bytes, [0xff, 0, 42]).unwrap();
    fs::write(&text, "original").unwrap();
    put(
        &project,
        "const BYTES: &[u8] = include_bytes!(\"../../bytes.bin\"); // Read non-UTF-8 bytes outside the source tree.\nconst TEXT: &str = include_str!(\"../../text.txt\"); // Read a separate external UTF-8 input.\nfn main() {} // These inputs affect compilation even without runtime execution.\n",
    );
    let report_command = || {
        binary()
            .current_dir(root())
            .args(["report", "--no-run", "--project"])
            .arg(&project)
            .arg("--artifacts")
            .arg(&artifacts)
            .output()
            .unwrap()
    };
    for path in [&bytes, &text] {
        let report = check(&project, &artifacts);
        let files = report["compiler_inputs"]["files"].as_object().unwrap();
        assert!(files.keys().any(|p| p.ends_with("bytes.bin")));
        assert!(files.keys().any(|p| p.ends_with("text.txt")));
        success(report_command());
        fs::write(path, b"changed").unwrap();
        let output = report_command();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("compiler input changed"));
    }
}

#[test]
fn environment_inputs_track_values_absence_and_the_effective_compiler_environment() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "const INPUT: &str = env!(\"CRAB_TEST_REQUIRED_INPUT\"); // A present environment input affects this compilation.\nconst OPTIONAL: Option<&str> = option_env!(\"CRAB_TEST_OPTIONAL_INPUT\"); // Absence is also an observed input state.\nconst BOOTSTRAP: Option<&str> = option_env!(\"RUSTC_BOOTSTRAP\"); // The audited compiler always receives a cleared bootstrap setting.\n#[cfg(any())] const DISABLED: &str = env!(\"CRAB_TEST_DISABLED_INPUT\"); // An inactive macro does not read the environment.\nfn main() {} // No runtime input is needed for this test.\n",
    );
    let command = |action, required: &str, optional: Option<&str>| {
        let mut command = binary();
        command
            .current_dir(root())
            .args([action, "--no-run", "--project"])
            .arg(&project)
            .arg("--artifacts")
            .arg(&artifacts)
            .env("CRAB_TEST_REQUIRED_INPUT", required)
            .env("RUSTC_BOOTSTRAP", "rusty_crab")
            .env_remove("CRAB_TEST_OPTIONAL_INPUT");
        if let Some(value) = optional {
            command.env("CRAB_TEST_OPTIONAL_INPUT", value);
        }
        command
    };
    success(command("audit", "first", None).output().unwrap());
    let bytes = fs::read(artifacts.join("audit.json")).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    let environment = report["compiler_environment"].as_object().unwrap();
    assert_eq!(
        environment["CRAB_TEST_REQUIRED_INPUT"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_ne!(environment["CRAB_TEST_REQUIRED_INPUT"], "first");
    assert!(environment["CRAB_TEST_OPTIONAL_INPUT"].is_null());
    assert!(environment["RUSTC_BOOTSTRAP"].is_null());
    assert!(!environment.contains_key("CRAB_TEST_DISABLED_INPUT"));
    success(
        command("report", "first", None)
            .env("CRAB_TEST_DISABLED_INPUT", "irrelevant")
            .output()
            .unwrap(),
    );
    for (required, optional) in [("second", None), ("first", Some("present"))] {
        let output = command("report", required, optional).output().unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("compiler environment input changed")
        );
    }
}

#[test]
fn negative_fixture_inputs_are_also_part_of_the_saved_audit() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    success(
        binary()
            .current_dir(root())
            .args(["generate", "--project"])
            .arg(&project)
            .output()
            .unwrap(),
    );
    fs::write(dir.path().join("negative.bin"), b"first").unwrap();
    let fixture = project.join("compile-fail/use_after_move.rs");
    let mut source = fs::read_to_string(&fixture).unwrap();
    source.push_str("const INPUT: &[u8] = include_bytes!(\"../../negative.bin\"); // Even a rejected example depends on this external file.\n");
    source.push_str("const ENV: &str = env!(\"CRAB_TEST_NEGATIVE_INPUT\"); // The negative example has its own environment dependency.\n");
    fs::write(fixture, source).unwrap();
    let command = |action, value: &str| {
        let mut command = binary();
        command
            .current_dir(root())
            .args([action, "--no-run", "--project"])
            .arg(&project)
            .arg("--artifacts")
            .arg(&artifacts)
            .env("CRAB_TEST_NEGATIVE_INPUT", value);
        command
    };
    success(command("audit", "first").output().unwrap());
    let report: Value =
        serde_json::from_slice(&fs::read(artifacts.join("audit.json")).unwrap()).unwrap();
    assert_eq!(
        status(&report, "semantic.use_after_move_rejected"),
        "covered"
    );
    success(command("report", "first").output().unwrap());
    let env_changed = command("report", "second").output().unwrap();
    assert!(!env_changed.status.success());
    assert!(
        String::from_utf8_lossy(&env_changed.stderr).contains("compiler environment input changed")
    );
    fs::write(dir.path().join("negative.bin"), b"second").unwrap();
    let file_changed = command("report", "first").output().unwrap();
    assert!(!file_changed.status.success());
    assert!(String::from_utf8_lossy(&file_changed.stderr).contains("compiler input changed"));
}

#[test]
fn feature_contexts_cannot_be_assembled_from_unrelated_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "trait AsyncTrait { async fn value() -> u32; } // A trait async function is not a free or inherent function.\n",
            "struct S; impl AsyncTrait for S { async fn value() -> u32 { 1 } } // A trait implementation is still not an inherent implementation.\n",
            "trait Generic<T> { type Fixed; } // A generic trait does not make its associated type generic.\n",
            "fn bound<I: Iterator<Item = u8>>(_: I) {} // An equality binding is not an associated type bound.\n",
            "const fn ordinary_const() {} // A const function without extern does not demonstrate const extern syntax.\n",
            "extern \"C\" fn ordinary_extern() {} // A separate extern function does not supply the missing qualifier.\n",
            "fn main() { let _closure = || async { 1 }; } // A plain closure returning an async block is not an async closure.\n",
        ),
    );
    let report = check(&project, &artifacts);
    for id in [
        "feature.generic_associated_types",
        "feature.associated_type_bounds",
        "feature.async_closure",
        "feature.const_extern_fn",
    ] {
        assert_eq!(status(&report, id), "missing", "{id}");
    }
    let entry = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "feature.async_await")
        .unwrap();
    assert_eq!(entry["status"], "missing");
    assert_eq!(
        entry["missing_witnesses"],
        serde_json::json!(["await", "free_fn", "inherent_fn"])
    );
}

#[test]
fn forwarded_qualifier_tokens_do_not_prove_a_source_function_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "macro_rules! function { ($c:tt $e:tt) => { $c unsafe $e \"C\" fn generated() {} }; } // The macro supplies the function declaration and unsafe qualifier.\n",
            "function!(const extern); // Forwarded keyword tokens are not a source-written const extern function.\n",
            "fn main() {} // No direct const-qualified function declaration appears in this file.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "feature.min_const_fn"), "missing");
    assert_eq!(status(&report, "ast.Const.Yes"), "missing");
    let entry = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "feature.const_extern_fn")
        .unwrap();
    assert_eq!(
        entry["missing_witnesses"],
        serde_json::json!(["safe", "unsafe"])
    );
}

fn missing_forms(report: &Value, id: &str) -> Vec<String> {
    report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == id)
        .unwrap()
        .get("missing_witnesses")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |forms| {
            forms
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect()
        })
}

#[test]
fn break_value_features_follow_the_named_destination_across_nested_constructs() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { let _ = 'destination: { loop { break 'destination 1; } }; } // The break crosses a loop but returns a value from the outer block.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.loop_break_value"),
        ["labeled", "unlabeled"]
    );
    assert_eq!(
        missing_forms(&report, "feature.label_break_value"),
        ["unit"]
    );
    put(
        &project,
        "fn main() { let _ = 'destination: loop { 'inner: { break 'destination 2; } }; } // The break crosses a labeled block but returns a value from the outer loop.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.loop_break_value"),
        ["unlabeled"]
    );
    assert_eq!(
        missing_forms(&report, "feature.label_break_value"),
        ["unit", "value"]
    );
}

#[test]
fn nested_pattern_features_require_nested_bindings_or_alternatives_and_slice_rests() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "struct Marker; // This name resolves to a constructor rather than a binding.\n",
            "fn main() { match Some(Marker) { whole @ Some(Marker) => (), None => () }; match 0 { 0 | 1 => (), _ => () }; if let 0 | 1 = 0 {} let [a, b] = [1, 2]; let (..) = (1, 2); } // An outer binding, top-level alternatives, and a tuple rest do not establish the more specific features.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.bindings_after_at"),
        ["subpattern_binding"]
    );
    assert_eq!(missing_forms(&report, "feature.or_patterns"), ["nested"]);
    assert_eq!(
        missing_forms(&report, "feature.slice_patterns"),
        ["bound", "unbound"]
    );
    put(
        &project,
        "fn main() { let whole @ Some(inner) = Some(1) else { return; }; match Some(1) { Some(0 | 1) => (), _ => () }; let [first, ..] = [1, 2]; } // Each nested form now occurs in its actual grammatical and resolved context.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "feature.bindings_after_at"), "covered");
    assert_eq!(status(&report, "feature.or_patterns"), "covered");
    assert_eq!(missing_forms(&report, "feature.slice_patterns"), ["bound"]);
    put(
        &project,
        concat!(
            "struct Holder<const N: usize>(i32); // A pattern path can contain a const argument with its own match expression.\n",
            "fn main() { let Holder::<{ match 0 { 0 | 1 => 1, _ => 0 } }>(field) = Holder::<1>(0); } // These alternatives are top-level arms of the inner expression, not nested alternatives in the outer pattern.\n",
        ),
    );
    assert_eq!(
        missing_forms(&check(&project, &artifacts), "feature.or_patterns"),
        ["nested"]
    );
}

#[test]
fn numeric_struct_fields_and_restricted_visibility_need_their_specific_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "pub struct Pair(pub i32, pub i32); // Public tuple fields have neither numeric field names nor restricted visibility.\npub fn main() { let pair = Pair(1, 2); let _ = pair.0; } // Tuple indexing does not exercise numeric names in braced construction or patterns.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.relaxed_adts"),
        ["expression", "pattern"]
    );
    assert_eq!(
        missing_forms(&report, "feature.pub_restricted"),
        ["crate", "in_path", "self", "super"]
    );
    put(
        &project,
        "pub(crate) struct Pair(i32, i32); // Only the crate restriction is written here.\nfn main() { let Pair { 0: left, 1: right } = Pair { 0: 1, 1: 2 }; } // Numeric field names now occur in both braced forms.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "feature.relaxed_adts"), "covered");
    assert_eq!(
        missing_forms(&report, "feature.pub_restricted"),
        ["in_path", "self", "super"]
    );
}

#[test]
fn destructuring_assignment_requires_a_compound_assignee_not_a_declaration_or_rhs() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let prelude = "struct Pair(i32, i32); // A tuple constructor supplies similar-looking value expressions.\n";
    put(
        &project,
        &format!(
            "{prelude}fn main() {{ let (mut a, mut b) = (1, 2); a = 3; b = 4; let _ = Pair(a, b); let _ = [a, b]; let _ = ..; }} // Bindings and ordinary assignments do not destructure into existing places.\n"
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.destructuring_assignment"),
        ["array", "constructor", "discard", "rest", "struct", "tuple"]
    );
    put(
        &project,
        "fn main() { let (mut a, mut b) = (0, 0); [a, b] = [1, 2]; } // Only the array form occurs on the left side of an assignment.\n",
    );
    assert_eq!(
        missing_forms(
            &check(&project, &artifacts),
            "feature.destructuring_assignment"
        ),
        ["constructor", "discard", "rest", "struct", "tuple"]
    );
}

#[test]
fn guard_and_attribute_features_do_not_borrow_evidence_from_nested_or_adjacent_contexts() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "#[allow(unused_variables)] fn outer<T>(argument: i32) {} // An item attribute annotates neither its parameter nor its type parameter.\n",
            "struct Container { #[allow(dead_code)] value: i32 } // A field declaration attribute is not a struct expression field attribute.\n",
            "fn main() { let _ = Container { value: 1 }; match Some(1) { value if { if let Some(_) = value { true } else { false } } => (), _ => () } } // An if-let inside a guard's block is not an if-let guard.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "feature.if_let_guard"),
        ["chain", "simple"]
    );
    assert_eq!(missing_forms(&report, "feature.param_attrs"), ["parameter"]);
    assert_eq!(
        missing_forms(&report, "feature.generic_param_attrs"),
        ["const", "lifetime", "type"]
    );
    assert_eq!(
        missing_forms(&report, "feature.struct_field_attributes"),
        ["field"]
    );
    put(
        &project,
        concat!(
            "struct Container<#[allow(dead_code)] T> { value: T } // Only the type generic parameter is annotated.\n",
            "fn outer(#[allow(unused_variables)] argument: i32) {} // This attribute belongs directly to a formal parameter.\n",
            "fn main() { let _ = Container { #[allow(unused_variables)] value: 1 }; } // This attribute belongs directly to a struct expression field.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "feature.param_attrs"), "covered");
    assert_eq!(
        status(&report, "feature.struct_field_attributes"),
        "covered"
    );
    assert_eq!(
        missing_forms(&report, "feature.generic_param_attrs"),
        ["const", "lifetime"]
    );
}

#[test]
fn identifier_patterns_require_compiler_name_resolution_not_identifier_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "const token: i32 = 1; // Deliberate lowercase spelling cannot turn a constant into a binding.\n",
            "fn main() { match 1 { token => (), _ => () } } // The compiler resolves the identifier to the constant.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert!(missing_forms(&report, "grammar.IdentifierPattern").contains(&"production".into()));
    assert_eq!(status(&report, "grammar.PathPattern"), "covered");
    put(
        &project,
        "fn main() { match 1 { token => () } } // Without the constant declaration, the same pattern spelling introduces a fresh binding.\n",
    );
    let report = check(&project, &artifacts);
    assert!(!missing_forms(&report, "grammar.IdentifierPattern").contains(&"production".into()));
    assert_eq!(
        missing_forms(&report, "grammar.PathPattern"),
        ["production"]
    );
    let outcome = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "grammar.IdentifierPattern")
        .unwrap();
    for evidence in outcome["evidence"].as_array().unwrap() {
        assert_eq!(evidence["stage"], "resolved_ast");
        let source = fs::read_to_string(project.join(evidence["file"].as_str().unwrap())).unwrap();
        assert_eq!(
            &source[evidence["start"].as_u64().unwrap() as usize
                ..evidence["end"].as_u64().unwrap() as usize],
            "token"
        );
    }
}

#[test]
fn documentary_names_self_and_shorthand_fields_are_not_standalone_pattern_productions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "type Pointer = fn(documented: u32, _: u32); // These are optional parameter labels, not patterns.\n",
            "struct Field { value: u32 } // A field name is not a local pattern.\n",
            "impl Field { fn show(&self) {} } // The self parameter has its own grammar.\n",
            "fn main() { let Field { value } = Field { value: 1 }; } // Field shorthand contains an identifier token but no standalone IdentifierPattern production.\n",
        ),
    );
    let report = check(&project, &artifacts);
    for name in ["IdentifierPattern", "PathPattern", "WildcardPattern"] {
        assert!(missing_forms(&report, &format!("grammar.{name}")).contains(&"production".into()));
    }
    put(
        &project,
        concat!(
            "struct Field { value: u32 } // Use an explicit field subpattern in the entry point.\n",
            "fn ignore(_: u32) {} // An ordinary function parameter can contain a genuine wildcard pattern.\n",
            "fn main() { let Field { value: renamed } = Field { value: 1 }; ignore(renamed); } // The explicit subpattern binds its own identifier.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert!(!missing_forms(&report, "grammar.IdentifierPattern").contains(&"production".into()));
    assert_eq!(status(&report, "grammar.WildcardPattern"), "covered");
    assert_eq!(status(&report, "grammar.PathPattern"), "missing");
    put(
        &project,
        concat!(
            "type Pointer = fn(_: [u8; { let actual = 1; let _ = actual; actual }]); // Real patterns inside a const array length remain visible despite the outer documentary underscore.\n",
            "fn main() {} // No other patterns can supply these witnesses.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert!(!missing_forms(&report, "grammar.IdentifierPattern").contains(&"production".into()));
    assert_eq!(status(&report, "grammar.WildcardPattern"), "covered");
}

#[test]
fn negative_literal_and_range_patterns_do_not_count_as_arithmetic_expressions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { match 0 { -1 => (), -5..=-2 => (), _ => () } } // Minus signs inside these patterns are grammar prefixes, not NegationExpression nodes.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "grammar.LiteralPattern"),
        ["root.sequence.0.absent"]
    );
    assert!(missing_forms(&report, "grammar.NegationExpression").contains(&"production".into()));
    assert!(missing_forms(&report, "grammar.RangeExpression").contains(&"production".into()));
    assert_eq!(status(&report, "grammar.RangeInclusivePattern"), "covered");
    assert_eq!(
        missing_forms(&report, "grammar.RangePatternBound"),
        ["root.alternative.1"]
    );
    put(
        &project,
        "fn main() { let _ = -1; let _ = -5..=-2; } // Expression negation and range construction do not provide literal or range patterns.\n",
    );
    let report = check(&project, &artifacts);
    assert!(missing_forms(&report, "grammar.LiteralPattern").contains(&"production".into()));
    assert!(missing_forms(&report, "grammar.ModernRangePattern").contains(&"production".into()));
    assert!(!missing_forms(&report, "grammar.NegationExpression").contains(&"production".into()));
    assert_eq!(status(&report, "grammar.RangeInclusiveExpr"), "covered");
}

#[test]
fn structural_grammar_separates_nested_commas_fields_and_base_only_forms() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "struct One { pair: (i32, i32) } // The outer struct has one field despite its inner tuple type.\n",
            "fn main() { let One { pair: (a, b) } = One { pair: (1, 2) }; let _ = 0..2; } // Inner commas and an unrelated range supply neither a struct trailing comma nor a struct rest.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "grammar.StructExprFields"),
        [
            "root.sequence.1.many",
            "root.sequence.1.one",
            "root.sequence.2.alternative.0",
            "root.sequence.2.alternative.1.present",
        ]
    );
    assert_eq!(
        missing_forms(&report, "grammar.StructPatternFields"),
        ["root.sequence.1.many", "root.sequence.1.one"]
    );
    let elements = missing_forms(&report, "grammar.StructPatternElements");
    assert!(!elements.contains(&"root.alternative.0.sequence.1.absent".into()));
    assert!(elements.contains(&"root.alternative.0.sequence.1.present".into()));
    for name in ["StructBase", "StructPatternEtCetera", "RestPattern"] {
        assert_eq!(
            missing_forms(&report, &format!("grammar.{name}")),
            ["production"]
        );
    }
    put(
        &project,
        concat!(
            "struct One { pair: (i32, i32) } // One named field supports a base-only update.\n",
            "fn main() { let original = One { pair: (1, 2) }; let copy = One { ..original }; let One { .. } = copy; } // Both the update and destructuring form omit explicit fields.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.StructBase"), "covered");
    assert_eq!(status(&report, "grammar.StructPatternEtCetera"), "covered");
    assert!(
        !missing_forms(&report, "grammar.StructExpression")
            .contains(&"root.sequence.2.body.alternative.1".into())
    );
    assert!(
        missing_forms(&report, "grammar.StructExprFields")
            .contains(&"root.sequence.2.alternative.0".into())
    );
    assert!(missing_forms(&report, "grammar.StructPatternFields").contains(&"production".into()));
    assert!(
        missing_forms(&report, "grammar.StructPatternElements")
            .contains(&"root.alternative.0.sequence.1.body.alternative.1".into())
    );
    assert_eq!(
        missing_forms(&report, "grammar.RestPattern"),
        ["production"]
    );
    put(
        &project,
        concat!(
            "struct Pair { a: i32, b: i32 } // The update base supplies an unlisted second field.\n",
            "fn main() { let original = Pair { a: 1, b: 2 }; let Pair { a, .. } = Pair { a: 3, ..original }; } // These forms combine an explicit field with the update base or pattern rest.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert!(
        !missing_forms(&report, "grammar.StructExprFields")
            .contains(&"root.sequence.2.alternative.0".into())
    );
    assert!(
        !missing_forms(&report, "grammar.StructPatternElements")
            .contains(&"root.alternative.0.sequence.1.body.alternative.1".into())
    );
}

#[test]
fn shorthand_pattern_modifiers_and_field_attributes_require_their_actual_positions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "struct Named { #[allow(dead_code)] value: i32 } // A declaration attribute does not annotate a destructuring field.\n",
            "#[allow(unused_variables)] fn main() { let Named { value: ref mut renamed } = Named { value: 1 }; *renamed += 1; } // The binding is nested in an explicit field, not a shorthand field.\n",
        ),
    );
    let report = check(&project, &artifacts);
    let field = missing_forms(&report, "grammar.StructPatternField");
    assert!(field.contains(&"root.sequence.0.one".into()));
    assert!(field.contains(&"root.sequence.1.alternative.2.sequence.0.present".into()));
    assert!(field.contains(&"root.sequence.1.alternative.2.sequence.1.present".into()));
    assert!(!field.contains(&"root.sequence.1.alternative.1".into()));
    put(
        &project,
        concat!(
            "struct Named { value: i32 } // Keep the declaration itself unannotated.\n",
            "fn main() { let Named { #[allow(unused_variables)] ref mut value } = Named { value: 1 }; *value += 1; } // Both modifiers and this attribute belong directly to the shorthand field.\n",
        ),
    );
    let report = check(&project, &artifacts);
    let field = missing_forms(&report, "grammar.StructPatternField");
    assert!(!field.contains(&"root.sequence.0.one".into()));
    assert!(field.contains(&"root.sequence.0.zero".into()));
    assert!(!field.contains(&"root.sequence.1.alternative.2.sequence.0.present".into()));
    assert!(!field.contains(&"root.sequence.1.alternative.2.sequence.1.present".into()));
    assert!(field.contains(&"root.sequence.1.alternative.2.sequence.1.absent".into()));
    assert!(
        missing_forms(&report, "grammar.StructExprField").contains(&"root.sequence.0.one".into())
    );
}

#[test]
fn tuple_and_slice_pattern_grammar_does_not_accept_expressions_or_types() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "struct Wrap(i32, i32); // A tuple-struct declaration supplies a constructor.\n",
            "fn main() { let x: (i32, i32) = (1, 2); let y = Wrap(1, 2); let z = [1, 2]; let (grouped) = 3; } // Similar expression and type forms do not destructure their values.\n",
        ),
    );
    let report = check(&project, &artifacts);
    for name in ["TuplePattern", "TupleStructPattern", "SlicePattern"] {
        assert!(missing_forms(&report, &format!("grammar.{name}")).contains(&"production".into()));
    }
    assert_eq!(status(&report, "grammar.GroupedPattern"), "covered");
    put(
        &project,
        "fn main() { let (..) = (1, 2); } // A rest-only tuple pattern has no written comma inside its parentheses.\n",
    );
    let report = check(&project, &artifacts);
    let tuple = missing_forms(&report, "grammar.TuplePatternItems");
    assert!(!tuple.contains(&"root.alternative.1".into()));
    assert!(tuple.contains(&"root.alternative.0".into()));
    assert!(tuple.contains(&"root.alternative.2".into()));
    assert_eq!(status(&report, "grammar.RestPattern"), "covered");
    put(
        &project,
        "fn main() { let (..,) = (1, 2); } // A comma after the sole rest pattern selects the singleton-comma grammar alternative.\n",
    );
    let report = check(&project, &artifacts);
    let tuple = missing_forms(&report, "grammar.TuplePatternItems");
    assert!(!tuple.contains(&"root.alternative.0".into()));
    assert!(tuple.contains(&"root.alternative.1".into()));
    assert!(tuple.contains(&"root.alternative.2".into()));
}

#[test]
fn reference_range_coverage_requires_each_active_source_alternative() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "#[cfg(any())] fn disabled() { let _ = (0.., ..2, .., 0..=2, ..=2); } // These range forms are inactive.\n",
            "macro_rules! ranges { () => { (0.., ..2, .., 0..=2, ..=2) }; } // A macro can generate the otherwise missing forms.\n",
            "fn main() { let _ = 0..2; let _ = ranges!(); let _ = \"0.., ..2, .., 0..=2, ..=2\"; } // Only one range form is written directly in active expression syntax.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.RangeExpr"), "covered");
    assert_eq!(status(&report, "grammar.RangeExpression"), "missing");
    assert_eq!(
        missing_forms(&report, "grammar.RangeExpression"),
        (1..6)
            .map(|i| format!("root.alternative.{i}"))
            .collect::<Vec<_>>()
    );
    put(
        &project,
        "fn main() { let _ = (0..2, 0.., ..2, .., 0..=2, ..=2); } // All six source range forms have active compiler witnesses.\n",
    );
    assert_eq!(
        status(&check(&project, &artifacts), "grammar.RangeExpression"),
        "covered"
    );
}

#[test]
fn reference_lists_distinguish_cardinality_and_their_own_trailing_comma() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "fn identity(value: (i32, i32)) -> (i32, i32) { value } // A single tuple-valued parameter is not several parameters.\n",
            "fn main() { let _ = [(1, 2), (3, 4)]; let _ = identity((1, 2)); } // Inner tuple commas do not become array or call trailing commas.\n",
        ),
    );
    let report = check(&project, &artifacts);
    let array = missing_forms(&report, "grammar.ArrayElements");
    assert!(array.contains(&"root.alternative.0.sequence.1.zero".into()));
    assert!(!array.contains(&"root.alternative.0.sequence.1.one".into()));
    assert!(array.contains(&"root.alternative.0.sequence.1.many".into()));
    assert!(array.contains(&"root.alternative.0.sequence.2.present".into()));
    let call = missing_forms(&report, "grammar.CallParams");
    assert!(!call.contains(&"root.sequence.1.zero".into()));
    assert!(call.contains(&"root.sequence.1.one".into()));
    assert!(call.contains(&"root.sequence.2.present".into()));

    put(
        &project,
        "fn main() { let _ = [1]; let _ = [1, 2]; let _ = [1, 2, 3, /* The last comma belongs to this array. */]; let _ = [0; 2]; } // Every array-elements alternative and local repetition/optional form occurs.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.ArrayElements"), "covered");
    assert_eq!(
        missing_forms(&report, "grammar.ArrayExpression"),
        ["root.sequence.1.absent"]
    );
}

#[test]
fn tuple_grammar_counts_comma_terminated_elements_not_just_tuple_arity() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { let _: (u8, u8) = (1, 2); } // The grammar repeats Type-comma once and then supplies its optional final Type.\n",
    );
    let report = check(&project, &artifacts);
    let forms = missing_forms(&report, "grammar.TupleType");
    assert!(forms.contains(&"root.alternative.1.sequence.1.many".into()));
    assert!(forms.contains(&"root.alternative.1.sequence.2.absent".into()));
    assert!(!forms.contains(&"root.alternative.1.sequence.1.one".into()));
    assert!(!forms.contains(&"root.alternative.1.sequence.2.present".into()));
    assert_eq!(
        missing_forms(&report, "grammar.TupleElements"),
        ["root.sequence.0.many", "root.sequence.1.absent"]
    );
    put(
        &project,
        "fn main() { let _: () = (); let _: (u8,) = (1,); let _: (u8, u8) = (1, 2); let _: (u8, u8,) = (1, 2,); } // These forms cover both tuple alternatives, repetition cardinalities, and optional final elements.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.TupleType"), "covered");
    assert_eq!(status(&report, "grammar.TupleElements"), "covered");
    assert_eq!(status(&report, "grammar.TupleExpression"), "covered");
}

#[test]
fn implicit_receiver_and_closure_types_are_not_written_type_grammar() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let implicit = concat!(
        "struct Receiver; // Give implicit receiver types a containing implementation.\n",
        "impl Receiver { fn shared(&self) {} fn unique(&mut self) {} } // Shorthand receivers do not write a ReferenceType with a referent type.\n",
        "fn main() { let mut r = Receiver; r.shared(); r.unique(); let f = |x| x; let _ = f(1); } // Inferred closure types do not spell the underscore type.\n",
    );
    put(&project, implicit);
    let report = check(&project, &artifacts);
    assert!(missing_forms(&report, "grammar.ReferenceType").contains(&"production".into()));
    assert!(missing_forms(&report, "grammar.InferredType").contains(&"production".into()));
    put(
        &project,
        concat!(
            "fn explicit(value: &mut u32) -> &u32 { value } // These reference types write their referent types explicitly.\n",
            "fn main() { let mut value: _ = 1u32; let _ = explicit(&mut value); } // A written underscore requests inference at a type position.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.InferredType"), "covered");
    assert_eq!(
        missing_forms(&report, "grammar.ReferenceType"),
        ["root.sequence.1.present"]
    );
}

#[test]
fn function_pointer_grammar_requires_types_and_distinguishes_binders_and_abi_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        concat!(
            "unsafe extern \"C\" fn ordinary(#[allow(unused)] x: u32, _: u32) -> u32 { x } // An ordinary declaration is not a function pointer type.\n",
            "fn main() {} // No written function pointer type occurs in this program.\n",
        ),
    );
    let report = check(&project, &artifacts);
    for name in [
        "BareFunctionType",
        "FunctionTypeQualifiers",
        "MaybeNamedParam",
        "BareFunctionReturnType",
    ] {
        assert!(missing_forms(&report, &format!("grammar.{name}")).contains(&"production".into()));
    }
    put(
        &project,
        concat!(
            "type Empty = for<> fn(); // The binder is written even though its parameter vector is empty.\n",
            "type Foreign = unsafe extern r\"C\" fn(u32) -> u32; // This function pointer has a raw ABI string and explicit return type.\n",
            "fn main() {} // The type declarations need no unsafe calls.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "grammar.BareFunctionType"), "covered");
    assert_eq!(status(&report, "grammar.ForLifetimes"), "covered");
    assert_eq!(
        missing_forms(&report, "grammar.Abi"),
        ["root.alternative.0"]
    );
    assert_eq!(
        missing_forms(&report, "grammar.FunctionTypeQualifiers"),
        ["root.sequence.1.body.sequence.1.absent"]
    );
    assert_eq!(
        missing_forms(&report, "grammar.FunctionParametersMaybeNamedVariadic"),
        ["root.alternative.1"]
    );
}

#[test]
fn variadic_parameter_counts_and_attributes_belong_to_their_own_positions() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "type Foreign = unsafe extern \"C\" fn(...); // rustc accepts zero fixed parameters, beyond this pinned Reference production.\nfn main() {} // The compiler accepts the signature without proving the Reference's fixed-parameter form.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(report["accepted"], true);
    assert_eq!(status(&report, "ast.TyKind.FnPtr"), "covered");
    for name in [
        "BareFunctionType",
        "FunctionParametersMaybeNamedVariadic",
        "MaybeNamedFunctionParametersVariadic",
    ] {
        assert!(missing_forms(&report, &format!("grammar.{name}")).contains(&"production".into()));
    }
    put(
        &project,
        concat!(
            "type Foreign = unsafe extern \"C\" fn(#[allow(unused)] #[allow(dead_code)] value: (u32, u32), ...); // Attributes and inner commas belong to the one fixed parameter.\n",
            "fn main() {} // The variadic signature is compiled without making a foreign call.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "grammar.MaybeNamedFunctionParametersVariadic"),
        [
            "root.sequence.0.many",
            "root.sequence.0.one",
            "root.sequence.3.many",
            "root.sequence.3.one",
        ]
    );
    let param = missing_forms(&report, "grammar.MaybeNamedParam");
    assert!(!param.contains(&"root.sequence.0.many".into()));
    assert!(param.contains(&"root.sequence.0.zero".into()));
    assert!(param.contains(&"root.sequence.1.absent".into()));
    assert!(!param.contains(&"root.sequence.1.body.sequence.0.alternative.0".into()));
    assert!(param.contains(&"root.sequence.1.body.sequence.0.alternative.1".into()));
    put(
        &project,
        concat!(
            "type Foreign = unsafe extern \"C\" fn(_: u32, u32, #[allow(unused)] ...); // Only the ellipsis carries an attribute, and only the first fixed parameter has an underscore name.\n",
            "fn main() {} // No variadic call is necessary to exercise the signature grammar.\n",
        ),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        missing_forms(&report, "grammar.MaybeNamedFunctionParametersVariadic"),
        [
            "root.sequence.0.many",
            "root.sequence.0.zero",
            "root.sequence.3.many",
            "root.sequence.3.zero",
        ]
    );
    assert_eq!(
        missing_forms(&report, "grammar.MaybeNamedParam"),
        [
            "root.sequence.0.many",
            "root.sequence.0.one",
            "root.sequence.1.body.sequence.0.alternative.0",
        ]
    );
}

#[test]
#[cfg(unix)]
fn retargeting_an_external_include_symlink_invalidates_the_report() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    fs::write(dir.path().join("first.bin"), b"first").unwrap();
    fs::write(dir.path().join("second.bin"), b"second").unwrap();
    let link = dir.path().join("input.bin");
    symlink("first.bin", &link).unwrap();
    put(
        &project,
        "const DATA: &[u8] = include_bytes!(\"../../input.bin\"); // Read through an external symlink.\nfn main() {} // The included data affects this compilation.\n",
    );
    check(&project, &artifacts);
    fs::remove_file(&link).unwrap();
    symlink("second.bin", &link).unwrap();
    let output = binary()
        .current_dir(root())
        .args(["report", "--no-run", "--project"])
        .arg(&project)
        .arg("--artifacts")
        .arg(&artifacts)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compiler input changed"));
}

#[test]
fn rejection_requires_the_intended_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    success(
        binary()
            .current_dir(root())
            .args(["generate", "--project"])
            .arg(&project)
            .output()
            .unwrap(),
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        status(&report, "semantic.use_after_move_rejected"),
        "covered"
    );
    assert_eq!(
        status(&report, "semantic.conflicting_borrows_rejected"),
        "covered"
    );
    fs::write(project.join("compile-fail/use_after_move.rs"), "fn main() { let invalid: u32 = \"wrong type\"; } // Rejected for an unrelated type mismatch.\n").unwrap();
    assert_eq!(
        status(
            &check(&project, &artifacts),
            "semantic.use_after_move_rejected"
        ),
        "missing"
    );
    fs::write(
        project.join("compile-fail/use_after_move.rs"),
        "fn main() {} // Compiles, so it cannot prove rejection after a move.\n",
    )
    .unwrap();
    assert_eq!(
        status(
            &check(&project, &artifacts),
            "semantic.use_after_move_rejected"
        ),
        "missing"
    );
}

#[test]
fn coercion_requires_typechecker_evidence_not_just_slice_syntax() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "fn main() { let array = [1, 2]; let slice: &[i32] = &array; } // Coerce an array reference to a slice.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(
        status(&report, "semantic.coercion.array_to_slice"),
        "covered"
    );
    let entry = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "semantic.coercion.array_to_slice")
        .unwrap();
    assert_eq!(entry["evidence"][0]["stage"], "typechecked");
    put(
        &project,
        "fn already_slice(value: &[i32]) { let alias: &[i32] = value; } // A slice type alone does not prove unsizing.\nfn main() {} // No array-to-slice coercion occurs in this crate.\n",
    );
    assert_eq!(
        status(
            &check(&project, &artifacts),
            "semantic.coercion.array_to_slice"
        ),
        "missing"
    );
    put(
        &project,
        "macro_rules! coercion { () => {{ let array = [1, 2]; let slice: &[i32] = &array; }}; } // These expressions are expansion-generated.\nfn main() { coercion!(); } // No source-written coercion expression occurs here.\n",
    );
    assert_eq!(
        status(
            &check(&project, &artifacts),
            "semantic.coercion.array_to_slice"
        ),
        "missing"
    );
}

#[test]
fn source_spans_refer_to_original_bytes_with_bom_unicode_and_crlf() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    let source = "\u{feff}// A crab 🦀 introduces this file.\r\nfn main() { loop { break; } } // A loop that exits.\r\n";
    put(&project, source);
    let report = check(&project, &artifacts);
    let entry = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "ast.ExprKind.Loop")
        .unwrap();
    let e = &entry["evidence"][0];
    assert_eq!(
        e["start"].as_u64().unwrap() as usize,
        source.find("loop {").unwrap()
    );
    assert_eq!(
        &source[e["start"].as_u64().unwrap() as usize..e["end"].as_u64().unwrap() as usize],
        "loop { break; }"
    );
    assert_eq!(e["line"], 2);
}

#[test]
fn macro_invocation_needs_a_real_expansion_at_its_source_location() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let artifacts = dir.path().join("artifacts");
    put(
        &project,
        "macro_rules! value { () => { 7 }; } // Define an expression-producing macro.\n#[cfg(any())] fn inactive() { let _ = value!(); } // This call is never expanded.\nfn main() {} // No active macro call appears here.\n",
    );
    assert_eq!(
        status(&check(&project, &artifacts), "ast.ExprKind.MacCall"),
        "missing"
    );
    put(
        &project,
        "macro_rules! value { () => { 7 }; } // Define the inner macro.\nmacro_rules! outer { () => { value!() }; } // Emit an inner macro call during expansion.\nfn main() { let _ = outer!(); } // Only this outer call is written as an active expression.\n",
    );
    let report = check(&project, &artifacts);
    assert_eq!(status(&report, "ast.ExprKind.MacCall"), "covered");
    let outcome = report["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["requirement"]["id"] == "ast.ExprKind.MacCall")
        .unwrap();
    for witness in outcome["evidence"].as_array().unwrap() {
        assert_eq!(witness["stage"], "compiler_expansion");
        assert_eq!(witness["line"], 3);
        assert_eq!(witness["macro_backtrace"], serde_json::json!(["outer!"]));
    }
}
