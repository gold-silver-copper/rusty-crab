use crate::model::*;
use anyhow::{Context, Result, bail, ensure};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use syn::{parse::Parser, spanned::Spanned};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub applicability: String,
    pub reason: String,
    pub detector: Option<String>,
    #[serde(default)]
    pub dimension: Option<String>,
    #[serde(default)]
    pub required_witnesses: BTreeMap<String, String>,
    #[serde(default)]
    pub grammar_shape_hash: Option<String>,
}

fn docs(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| {
            let syn::Meta::NameValue(m) = &a.meta else {
                return None;
            };
            if !m.path.is_ident("doc") {
                return None;
            }
            let syn::Expr::Lit(l) = &m.value else {
                return None;
            };
            let syn::Lit::Str(s) = &l.lit else {
                return None;
            };
            Some(s.value().trim().to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn candidate(
    id: String,
    description: String,
    dimension: &str,
    path: &str,
    node: &impl ToTokens,
    span: proc_macro2::Span,
) -> Requirement {
    Requirement {
        id,
        description,
        dimension: dimension.into(),
        origin: "extracted".into(),
        provenance: Provenance {
            path: path.into(),
            line: span.start().line,
            revision: COMMIT.into(),
            shape_hash: hash(node.to_token_stream().to_string()),
        },
        applicability: "unclassified".into(),
        reason: "Requires explicit interpretation of the compiler candidate".into(),
        detector: None,
        required_witnesses: Default::default(),
    }
}

pub fn extract(root: &Path) -> Result<Inventory> {
    let sources_root = root.join("vendor/rust");
    let locked: BTreeMap<String, String> = read_json(&root.join("config/sources.json"))?;
    ensure!(!locked.is_empty(), "empty source lock");
    ensure!(
        source_hashes(&sources_root)? == locked,
        "vendored compiler source tree differs from its lock; added, removed, or changed files need review"
    );
    for (path, expected) in &locked {
        let bytes = fs::read(sources_root.join(path))?;
        ensure!(
            hash(&bytes) == *expected,
            "compiler source hash mismatch: {path}; review the revision and inventory before updating the source lock"
        );
    }
    let rules_path = root.join("config/rules.json");
    let rules: BTreeMap<String, Rule> = read_json(&rules_path)?;
    let mut requirements = Vec::new();
    for (prefix, path) in [
        ("ast", "compiler/rustc_ast/src/ast.rs"),
        ("lex", "compiler/rustc_lexer/src/lib.rs"),
    ] {
        let file = syn::parse_file(&fs::read_to_string(sources_root.join(path))?)?;
        let before = requirements.len();
        for item in file.items {
            if let syn::Item::Enum(item) = item {
                if !matches!(item.vis, syn::Visibility::Public(_)) {
                    continue;
                }
                for v in item.variants {
                    let id = format!("{prefix}.{}.{}", item.ident, v.ident);
                    let description = docs(&v.attrs);
                    requirements.push(candidate(
                        id.clone(),
                        if description.is_empty() {
                            id
                        } else {
                            description
                        },
                        if prefix == "lex" {
                            "lexical"
                        } else {
                            "syntactic"
                        },
                        path,
                        &v,
                        v.ident.span(),
                    ));
                }
            }
        }
        ensure!(
            requirements.len() > before,
            "no enum candidates extracted from {path}"
        );
    }
    for table in ["accepted", "unstable", "removed"] {
        let path = format!("compiler/rustc_feature/src/{table}.rs");
        let file = syn::parse_file(&fs::read_to_string(sources_root.join(&path))?)?;
        let mut invocations = 0;
        for item in file.items {
            if let syn::Item::Macro(m) = &item {
                if m.ident.is_some() || !m.mac.path.is_ident("declare_features") {
                    continue;
                }
                invocations += 1;
                let tuples =
                    syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                        .parse2(m.mac.tokens.clone())?;
                ensure!(!tuples.is_empty(), "empty feature table {table}");
                for entry in tuples {
                    let syn::Expr::Tuple(tuple) = &entry else {
                        bail!("unknown feature table entry in {path}")
                    };
                    ensure!(
                        (4..=6).contains(&tuple.elems.len()),
                        "changed feature table tuple in {path}"
                    );
                    let status = tuple.elems[0].to_token_stream().to_string();
                    let syn::Expr::Path(name) = &tuple.elems[1] else {
                        bail!("unknown feature name")
                    };
                    let name = name
                        .path
                        .get_ident()
                        .context("feature must have a simple name")?
                        .to_string();
                    let mut r = candidate(
                        format!("feature.{name}"),
                        docs(&tuple.attrs),
                        "semantic",
                        &path,
                        &entry,
                        tuple.span(),
                    );
                    match status.as_str() {
                        "accepted" => {
                            r.applicability = "applicable".into();
                            r.reason = "Accepted feature gate; applicability refinements and semantic detector require review".into();
                        }
                        "unstable" | "incomplete" | "internal" | "removed" => {
                            r.applicability = "inapplicable".into();
                            r.reason = format!(
                                "Compiler table classifies this as {status}; outside the stable profile. Other profiles remain pending."
                            );
                        }
                        _ => bail!("unknown feature status {status}"),
                    }
                    requirements.push(r);
                }
            }
            if let syn::Item::Const(c) = &item {
                if ["INCOMPATIBLE_FEATURES", "DEPENDENT_FEATURES"]
                    .contains(&c.ident.to_string().as_str())
                {
                    let mut r = candidate(
                        format!("interaction.{}", c.ident),
                        "Feature-gate compatibility constraints".into(),
                        "interaction",
                        &path,
                        c,
                        c.ident.span(),
                    );
                    r.applicability = "inapplicable".into();
                    r.reason = "Constraints on unstable feature gates; retained for companion nightly profile".into();
                    requirements.push(r);
                }
            }
        }
        ensure!(
            invocations == 1,
            "expected one declare_features invocation in {path}, found {invocations}"
        );
    }
    let path = "compiler/rustc_feature/src/builtin_attrs.rs";
    let file = syn::parse_file(&fs::read_to_string(sources_root.join(path))?)?;
    let mut tables = 0;
    for item in file.items {
        let syn::Item::Static(s) = item else { continue };
        if s.ident != "BUILTIN_ATTRIBUTES" {
            continue;
        }
        tables += 1;
        let syn::Expr::Reference(r) = *s.expr else {
            bail!("changed attribute table reference")
        };
        let syn::Expr::Array(a) = *r.expr else {
            bail!("changed attribute table array")
        };
        ensure!(!a.elems.is_empty(), "empty builtin attribute table");
        for expr in a.elems {
            let syn::Expr::Path(p) = &expr else {
                bail!("unknown builtin attribute entry")
            };
            ensure!(
                p.path.segments.len() == 2 && p.path.segments[0].ident == "sym",
                "unknown builtin attribute shape"
            );
            let name = &p.path.segments[1].ident;
            requirements.push(candidate(
                format!("attribute.{name}"),
                format!(
                    "Built-in attribute {name}; argument forms require separate interpretation"
                ),
                "syntactic",
                path,
                &expr,
                expr.span(),
            ));
        }
    }
    ensure!(tables == 1, "missing or duplicated builtin attribute table");
    let path = "compiler/rustc_span/src/symbol.rs";
    for keyword in crate::compiler_source::keywords(&fs::read_to_string(sources_root.join(path))?)?
    {
        requirements.push(candidate(format!("keyword.{}", keyword.name), format!("Lexical spelling {:?}; context-dependent keyword meaning requires syntactic evidence", keyword.spelling.value()), "lexical", path, &keyword.spelling, keyword.name.span()));
    }
    let (reference_revision, reference_hashes, grammar) = crate::reference::extract(root)?;
    requirements.extend(grammar);
    for r in &mut requirements {
        if let Some(rule) = rules.get(&r.id) {
            r.applicability.clone_from(&rule.applicability);
            r.reason.clone_from(&rule.reason);
            r.detector.clone_from(&rule.detector);
            if let Some(dimension) = &rule.dimension {
                ensure!(
                    ["lexical", "syntactic", "semantic", "interaction"]
                        .contains(&dimension.as_str()),
                    "invalid coverage dimension: {dimension}"
                );
                r.dimension.clone_from(dimension);
            }
            r.required_witnesses.clone_from(&rule.required_witnesses);
            if rule.detector.as_deref() == Some("rustc_reference_v1") {
                ensure!(
                    r.id.starts_with("grammar.") && rule.grammar_shape_hash.is_some(),
                    "Reference detector needs a grammar production and reviewed shape hash: {}",
                    r.id
                );
                ensure!(
                    rule.required_witnesses.is_empty(),
                    "Reference structural forms must be extracted, not supplied by rules: {}",
                    r.id
                );
                match crate::grammar::forms(&r.description) {
                    Ok(forms) => r.required_witnesses = forms,
                    Err(error) => {
                        r.applicability = "unclassified".into();
                        r.detector = None;
                        r.reason =
                            format!("Reference notation needs implementation and review: {error}");
                    }
                }
                if rule.grammar_shape_hash.as_deref() != Some(&r.provenance.shape_hash) {
                    r.applicability = "unclassified".into();
                    r.detector = None;
                    r.reason = format!(
                        "Reference production changed since its compiler mapping was reviewed; newly extracted forms remain unclassified. {}",
                        r.reason
                    );
                }
            }
        }
    }
    requirements.extend(crate::fixtures::requirements(root)?);
    requirements.extend(crate::semantic::requirements(root)?);
    requirements.sort_by(|a, b| a.id.cmp(&b.id));
    for key in rules.keys() {
        ensure!(
            requirements.iter().any(|r| r.id == *key),
            "interpretation rule has no compiler candidate: {key}"
        );
    }
    let inventory = Inventory {
        schema: 1, compiler_commit: COMMIT.into(), reference_revision, reference_hashes, source_hashes: locked, rules_hash: hash(fs::read(rules_path)?),
        limitations: vec![
            "Reference productions are enumerated, but parser mapping and coverage of alternatives and optional grammar groups remain incomplete.".into(),
            "Accepted feature interpretation remains incomplete; implemented syntax witnesses do not establish the remaining semantic contracts.".into(),
            "Companion nightly, cross-target, historical-edition, and procedural-macro profiles remain incomplete.".into(),
        ], requirements,
    };
    validate_inventory(&inventory)?;
    Ok(inventory)
}

pub fn diff(old: &Inventory, new: &Inventory) -> serde_json::Value {
    let a: BTreeMap<_, _> = old.requirements.iter().map(|r| (&r.id, r)).collect();
    let b: BTreeMap<_, _> = new.requirements.iter().map(|r| (&r.id, r)).collect();
    let added: Vec<_> = b.keys().filter(|k| !a.contains_key(*k)).copied().collect();
    let removed: Vec<_> = a.keys().filter(|k| !b.contains_key(*k)).copied().collect();
    let changed: Vec<_> = b
        .iter()
        .filter(|(k, v)| a.get(*k).is_some_and(|old| old != *v))
        .map(|(k, _)| *k)
        .collect();
    let mut form_changes = BTreeMap::new();
    for (id, requirement) in &b {
        if let Some(previous) = a.get(id) {
            let before = &previous.required_witnesses;
            let after = &requirement.required_witnesses;
            if before != after {
                let added: Vec<_> = after.keys().filter(|k| !before.contains_key(*k)).collect();
                let removed: Vec<_> = before.keys().filter(|k| !after.contains_key(*k)).collect();
                let changed: Vec<_> = after
                    .keys()
                    .filter(|k| before.get(*k).is_some_and(|v| v != &after[*k]))
                    .collect();
                form_changes.insert(
                    *id,
                    serde_json::json!({"added": added, "removed": removed, "changed": changed}),
                );
            }
        }
    }
    serde_json::json!({"old_compiler": old.compiler_commit, "new_compiler": new.compiler_commit, "added": added, "removed": removed, "changed_requiring_review": changed, "form_changes": form_changes, "source_changed": old.source_hashes != new.source_hashes, "reference_changed": old.reference_revision != new.reference_revision || old.reference_hashes != new.reference_hashes, "rules_changed": old.rules_hash != new.rules_hash})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn new_reference_alternatives_invalidate_the_reviewed_mapping() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let first = extract(repository).unwrap();
        let temp = tempfile::tempdir().unwrap();
        copy_tree(&repository.join("vendor"), &temp.path().join("vendor"));
        copy_tree(&repository.join("config"), &temp.path().join("config"));
        let path = temp
            .path()
            .join("vendor/reference/src/expressions/return-expr.md");
        let original = fs::read_to_string(&path).unwrap();
        let changed = original.replace(
            "ReturnExpression -> `return` Expression?",
            "ReturnExpression -> `return` Expression? | `resume` Expression",
        );
        assert_ne!(original, changed);
        fs::write(&path, changed).unwrap();
        assert!(
            extract(temp.path()).is_err(),
            "unreviewed source-lock changes must fail"
        );
        write_json(
            &temp.path().join("config/reference-sources.json"),
            &source_hashes(&temp.path().join("vendor/reference")).unwrap(),
        )
        .unwrap();
        // Updating the source lock alone does not review the detector's mapping.
        let next = extract(temp.path()).unwrap();
        let requirement = next
            .requirements
            .iter()
            .find(|r| r.id == "grammar.ReturnExpression")
            .unwrap();
        assert_eq!(requirement.applicability, "unclassified");
        assert!(requirement.detector.is_none());
        assert!(
            requirement
                .required_witnesses
                .contains_key("root.alternative.1")
        );
        let changes = diff(&first, &next);
        assert!(
            changes["form_changes"]["grammar.ReturnExpression"]["added"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "root.alternative.1")
        );
        fs::write(
            &path,
            original.replace("Expression?\n```", "Expression? _unless FutureRule_\n```"),
        )
        .unwrap();
        write_json(
            &temp.path().join("config/reference-sources.json"),
            &source_hashes(&temp.path().join("vendor/reference")).unwrap(),
        )
        .unwrap();
        let next = extract(temp.path()).unwrap();
        let requirement = next
            .requirements
            .iter()
            .find(|r| r.id == "grammar.ReturnExpression")
            .unwrap();
        assert_eq!(requirement.applicability, "unclassified");
        assert!(requirement.reason.contains("unhandled Reference notation"));
    }

    #[test]
    fn extraction_is_deterministic_and_source_drift_cannot_silently_pass() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let first = extract(repository).unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&extract(repository).unwrap()).unwrap()
        );
        let temp = tempfile::tempdir().unwrap();
        copy_tree(&repository.join("vendor"), &temp.path().join("vendor"));
        copy_tree(&repository.join("config"), &temp.path().join("config"));
        let path = temp
            .path()
            .join("vendor/rust/compiler/rustc_ast/src/ast.rs");
        let mut source = fs::read_to_string(&path).unwrap();
        source.push_str("\npub enum NextCompilerSyntax { NovelForm }\n");
        fs::write(path, source).unwrap();
        assert!(
            extract(temp.path())
                .unwrap_err()
                .to_string()
                .contains("differs from its lock")
        );
        // Simulate an explicitly reviewed source-lock update, but no interpretation rule yet.
        write_json(
            &temp.path().join("config/sources.json"),
            &source_hashes(&temp.path().join("vendor/rust")).unwrap(),
        )
        .unwrap();
        let upgraded = extract(temp.path()).unwrap();
        let candidate = upgraded
            .requirements
            .iter()
            .find(|r| r.id == "ast.NextCompilerSyntax.NovelForm")
            .unwrap();
        assert_eq!(candidate.applicability, "unclassified");
        assert!(candidate.detector.is_none());
        let diff = diff(&first, &upgraded);
        assert!(
            diff["added"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id == "ast.NextCompilerSyntax.NovelForm")
        );
    }
}
