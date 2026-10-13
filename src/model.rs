use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub const COMMIT: &str = "b940084d7eb6a299eb4bfeb8e34901bc051e7ac4";
pub const AUDIT_SCHEMA: u32 = 3;
pub const OBSERVATION_SCHEMA: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub path: String,
    pub line: usize,
    pub revision: String,
    pub shape_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub description: String,
    pub dimension: String,
    pub origin: String,
    pub provenance: Provenance,
    pub applicability: String,
    pub reason: String,
    pub detector: Option<String>,
    /// All named forms must have independent evidence before this requirement is covered.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required_witnesses: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub schema: u32,
    pub compiler_commit: String,
    pub reference_revision: String,
    pub reference_hashes: BTreeMap<String, String>,
    pub source_hashes: BTreeMap<String, String>,
    pub rules_hash: String,
    pub limitations: Vec<String>,
    pub requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Evidence {
    pub requirement: String,
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub stage: String,
    pub detector: String,
    pub origin: String,
    pub configuration: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macro_backtrace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Configuration {
    pub compiler_commit: String,
    pub release: String,
    pub edition: String,
    pub target: String,
    pub profile: String,
    pub crate_type: String,
    pub cfg: Vec<String>,
    pub features: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompilerObservation {
    pub schema: u32,
    pub compiler_commit: String,
    pub configuration: Configuration,
    pub accepted: bool,
    pub inputs: crate::inputs::FileInputs,
    pub environment: BTreeMap<String, Option<String>>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub requirement: Requirement,
    pub status: String,
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_witnesses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audit {
    pub schema: u32,
    pub auditor_hash: String,
    pub configuration: Configuration,
    pub inventory_hash: String,
    pub source_hashes: BTreeMap<String, String>,
    pub compiler_inputs: crate::inputs::FileInputs,
    pub compiler_environment: BTreeMap<String, Option<String>>,
    pub accepted: bool,
    pub ran: bool,
    pub annotations_valid: bool,
    pub issues: Vec<String>,
    pub validations: Vec<Validation>,
    pub outcomes: Vec<Outcome>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validation {
    pub id: String,
    pub stage: String,
    pub passed: bool,
    pub detail: String,
}

pub fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

pub fn canonical_hash(value: &impl Serialize) -> Result<String> {
    Ok(hash(serde_json::to_vec(value)?))
}

pub fn source_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(root: &Path, dir: &Path, map: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            ensure!(
                !entry.file_type()?.is_symlink(),
                "vendored source must not contain symlinks: {}",
                path.display()
            );
            if entry.file_type()?.is_dir() {
                walk(root, &path, map)?;
            } else {
                map.insert(
                    path.strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    hash(fs::read(path)?),
                );
            }
        }
        Ok(())
    }
    let mut map = BTreeMap::new();
    walk(root, root, &mut map)?;
    Ok(map)
}

pub fn validate_inventory(inventory: &Inventory) -> Result<()> {
    ensure!(inventory.schema == 1, "unsupported inventory schema");
    ensure!(
        inventory.compiler_commit == COMMIT,
        "stale compiler identity in inventory"
    );
    ensure!(
        !inventory.requirements.is_empty(),
        "empty inventory cannot establish coverage"
    );
    let mut last = None;
    for r in &inventory.requirements {
        ensure!(
            last.is_none_or(|s: &str| s < r.id.as_str()),
            "inventory IDs must be unique and sorted"
        );
        ensure!(
            ["applicable", "inapplicable", "unclassified"].contains(&r.applicability.as_str()),
            "unknown applicability for {}",
            r.id
        );
        ensure!(
            !r.reason.is_empty(),
            "missing classification explanation: {}",
            r.id
        );
        for (form, description) in &r.required_witnesses {
            ensure!(
                r.detector.is_some() || r.applicability == "unclassified",
                "named witnesses require a detector: {}",
                r.id
            );
            ensure!(
                !form.is_empty() && !description.trim().is_empty(),
                "empty witness specification: {}",
                r.id
            );
        }
        last = Some(r.id.as_str());
    }
    Ok(())
}
