use crate::model::*;
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, fs, path::Path};

pub fn extract(root: &Path) -> Result<(String, BTreeMap<String, String>, Vec<Requirement>)> {
    let pin: serde_json::Value = read_json(&root.join("config/reference-revision.json"))?;
    let revision = pin["sha"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing Reference revision"))?
        .to_owned();
    let sources: BTreeMap<String, String> = read_json(&root.join("config/reference-sources.json"))?;
    ensure!(!sources.is_empty(), "empty Reference source inventory");
    ensure!(
        source_hashes(&root.join("vendor/reference"))? == sources,
        "vendored Reference tree differs from its source lock"
    );
    let mut requirements = Vec::new();
    for (path, expected) in &sources {
        let content = fs::read(root.join("vendor/reference").join(path))?;
        ensure!(
            hash(&content) == *expected,
            "Reference source hash mismatch: {path}"
        );
        if !path.ends_with(".md") {
            continue;
        }
        let source = String::from_utf8(content)?;
        requirements.extend(extract_markdown(path, &revision, &source)?);
    }
    ensure!(
        !requirements.is_empty(),
        "Reference extraction found no grammar productions"
    );
    Ok((revision, sources, requirements))
}

fn extract_markdown(path: &str, revision: &str, source: &str) -> Result<Vec<Requirement>> {
    let mut requirements = Vec::new();
    let mut grammar = false;
    let mut lexical = false;
    let mut production: Option<(String, usize, String)> = None;
    let finish = |production: &mut Option<(String, usize, String)>,
                  lexical: bool,
                  out: &mut Vec<Requirement>| {
        if let Some((name, line, body)) = production.take() {
            let (forms, reason) = match crate::grammar::forms(body.trim()) {
                Ok(forms) => (forms, "Local structural forms are derived from Reference EBNF; applicability, compiler mapping, and contextual restrictions still require review".into()),
                Err(error) => (BTreeMap::new(), format!("Reference grammar notation is not yet supported by the structural extractor: {error}; production remains unclassified")),
            };
            out.push(Requirement {
                id: format!("grammar.{name}"),
                description: body.trim().into(),
                dimension: if lexical { "lexical" } else { "syntactic" }.into(),
                origin: "extracted".into(),
                provenance: Provenance {
                    path: format!("reference/{path}"),
                    line,
                    revision: revision.into(),
                    shape_hash: hash(body.trim()),
                },
                applicability: "unclassified".into(),
                reason,
                detector: None,
                required_witnesses: forms,
            });
        }
    };
    let mut saw_production = false;
    for (line_number, line) in source.lines().enumerate() {
        if line.starts_with("```grammar") {
            ensure!(!grammar, "nested grammar fence in {path}");
            grammar = true;
            lexical = line.split(',').any(|p| p == "lexer");
            saw_production = false;
        } else if grammar && line == "```" {
            ensure!(saw_production, "empty grammar block in {path}");
            finish(&mut production, lexical, &mut requirements);
            grammar = false;
        } else if grammar {
            if let Some((name, _)) = line.split_once(" ->") {
                let name = name.strip_prefix("@root ").unwrap_or(name);
                ensure!(
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "unknown grammar production header in {path}: {line}"
                );
                finish(&mut production, lexical, &mut requirements);
                production = Some((name.into(), line_number + 1, format!("{line}\n")));
                saw_production = true;
            } else if let Some((_, _, body)) = &mut production {
                body.push_str(line);
                body.push('\n');
            } else {
                ensure!(
                    line.trim().is_empty(),
                    "unrecognized grammar before first production in {path}: {line}"
                );
            }
        }
    }
    ensure!(!grammar, "unterminated grammar fence in {path}");
    Ok(requirements)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_alternatives_and_rejects_unknown_grammar_headers() {
        let source = "```grammar,expressions\nBorrowExpression ->\n    `&` Expression\n  | `&` `mut` Expression\n```\n";
        let r = extract_markdown("test.md", "revision", source).unwrap();
        assert_eq!(r[0].id, "grammar.BorrowExpression");
        assert!(r[0].description.contains("| `&` `mut`"));
        assert_eq!(r[0].applicability, "unclassified");
        assert!(
            extract_markdown(
                "test.md",
                "revision",
                "```grammar\nNew syntax => Unknown\n```\n"
            )
            .is_err()
        );
    }
}
