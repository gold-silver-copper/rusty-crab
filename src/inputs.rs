//! Observe the bytes delivered to rustc, including non-source include files.
use crate::model::hash;
use rustc_span::source_map::{FileLoader, RealFileLoader};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileInputs {
    pub files: BTreeMap<String, String>,
    pub probes: BTreeMap<String, bool>,
    pub inconsistent: Vec<String>,
}

impl FileInputs {
    pub fn merge(&mut self, other: Self) {
        for (key, digest) in other.files {
            if self
                .files
                .insert(key.clone(), digest.clone())
                .is_some_and(|old| old != digest)
            {
                self.inconsistent
                    .push(format!("Compiler input changed between invocations: {key}"));
            }
        }
        for (key, present) in other.probes {
            if self
                .probes
                .insert(key.clone(), present)
                .is_some_and(|old| old != present)
            {
                self.inconsistent.push(format!(
                    "Compiler file probe changed between invocations: {key}"
                ));
            }
        }
        self.inconsistent.extend(other.inconsistent);
        self.inconsistent.sort();
        self.inconsistent.dedup();
    }
}

pub type SharedInputs = Arc<Mutex<FileInputs>>;

pub struct TrackingLoader {
    pub root: PathBuf,
    pub inputs: SharedInputs,
}

impl TrackingLoader {
    fn key(&self, path: &Path) -> io::Result<String> {
        // Preserve symlinks and .. components: reopening this same spelling must
        // observe a retargeted external symlink, rather than its former target.
        let absolute = std::path::absolute(path)?;
        let base: Vec<_> = self.root.components().collect();
        let parts: Vec<_> = absolute.components().collect();
        let common = base.iter().zip(&parts).take_while(|(a, b)| a == b).count();
        let mut relative = PathBuf::new();
        for _ in common..base.len() {
            relative.push("..");
        }
        for part in &parts[common..] {
            relative.push(part.as_os_str());
        }
        relative
            .to_str()
            .map(|s| s.replace('\\', "/"))
            .ok_or_else(|| io::Error::other("compiler input path is not UTF-8"))
    }

    fn record(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let key = self.key(path)?;
        let digest = hash(bytes);
        let mut inputs = self.inputs.lock().unwrap();
        if inputs
            .files
            .insert(key.clone(), digest.clone())
            .is_some_and(|old| old != digest)
        {
            inputs
                .inconsistent
                .push(format!("Compiler input changed between reads: {key}"));
        }
        Ok(())
    }
}

impl FileLoader for TrackingLoader {
    fn file_exists(&self, path: &Path) -> bool {
        let present = RealFileLoader.file_exists(path);
        let mut inputs = self.inputs.lock().unwrap();
        match self.key(path) {
            Ok(key) => {
                if inputs
                    .probes
                    .insert(key.clone(), present)
                    .is_some_and(|old| old != present)
                {
                    inputs
                        .inconsistent
                        .push(format!("Compiler file probe changed: {key}"));
                }
            }
            Err(_) => inputs
                .inconsistent
                .push("Cannot identify compiler file probe".into()),
        }
        present
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        let source = RealFileLoader.read_file(path)?;
        self.record(path, source.as_bytes())?;
        Ok(source)
    }

    fn read_binary_file(&self, path: &Path) -> io::Result<Arc<[u8]>> {
        let bytes = RealFileLoader.read_binary_file(path)?;
        self.record(path, &bytes)?;
        Ok(bytes)
    }

    fn current_directory(&self) -> io::Result<PathBuf> {
        RealFileLoader.current_directory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_bytes_are_hashed_before_normalization_and_conflicting_reads_fail() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source.rs");
        let loader = TrackingLoader {
            root: root.path().to_path_buf(),
            inputs: Default::default(),
        };
        let original = "\u{feff}fn main() {}\r\n";
        std::fs::write(&path, original).unwrap();
        assert_eq!(loader.read_file(&path).unwrap(), original);
        assert_eq!(
            loader.inputs.lock().unwrap().files["source.rs"],
            hash(original)
        );
        std::fs::write(&path, b"fn main() { loop {} }").unwrap();
        assert_eq!(
            &*loader.read_binary_file(&path).unwrap(),
            b"fn main() { loop {} }"
        );
        assert!(
            crate::audit::validate_inputs(root.path(), &loader.inputs.lock().unwrap()).is_err()
        );
    }

    #[test]
    fn previously_absent_file_probe_cannot_silently_change() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("optional.rs");
        let loader = TrackingLoader {
            root: root.path().to_path_buf(),
            inputs: Default::default(),
        };
        assert!(!loader.file_exists(&path));
        std::fs::write(&path, b"fn extra() {}").unwrap();
        assert!(
            crate::audit::validate_inputs(root.path(), &loader.inputs.lock().unwrap()).is_err()
        );
    }
}
