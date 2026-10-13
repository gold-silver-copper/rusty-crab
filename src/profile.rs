//! Reviewed compiler configurations. Catalog intent never selects worker flags.
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Stable,
    Freestanding,
}

impl Profile {
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "stable" => Ok(Self::Stable),
            "stable_freestanding" => Ok(Self::Freestanding),
            _ => bail!("unimplemented profile {name}; use stable or stable_freestanding"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Freestanding => "stable_freestanding",
        }
    }

    pub fn runs(self) -> bool {
        self == Self::Stable
    }

    pub fn flags(self) -> Vec<String> {
        let mut flags: Vec<String> = [
            "--edition=2024",
            "--crate-type=bin",
            "--crate-name=pangram",
            "--cap-lints=allow",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if self == Self::Freestanding {
            flags.push("-Cpanic=abort".into());
        }
        flags
    }
}
