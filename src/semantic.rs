use crate::model::*;
use anyhow::Result;
use std::{fs, path::Path};

pub const COERCIONS: &[(&str, &str)] = &[
    (
        "semantic.coercion.array_to_slice",
        "A reference to an array is implicitly unsized to a slice reference",
    ),
    (
        "semantic.coercion.concrete_to_dyn",
        "A reference to a concrete sized type is implicitly unsized to a trait-object reference",
    ),
    (
        "semantic.coercion.closure_to_fn",
        "A noncapturing closure implicitly coerces to a function pointer",
    ),
];

pub fn requirements(repository: &Path) -> Result<Vec<Requirement>> {
    let path = "compiler/rustc_middle/src/ty/adjustment.rs";
    let content = fs::read(repository.join("vendor/rust").join(path))?;
    Ok(COERCIONS.iter().map(|(id, description)| Requirement {
        id: (*id).into(), description: (*description).into(), dimension: "semantic".into(), origin: "manually_specified".into(),
        provenance: Provenance { path: path.into(), line: 11, revision: COMMIT.into(), shape_hash: hash(&content) },
        applicability: "applicable".into(), reason: "Selected positive semantic witness requiring the actual type-checker adjustment and source/target types".into(), detector: Some("rustc_typeck_v1".into()),
        required_witnesses: Default::default(),
    }).collect())
}
