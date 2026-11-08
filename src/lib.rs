#![doc = include_str!("../README.md")]

use datalog_structural_model::Program;
use rdf_reasoning::{Exclusion, Preset, PresetParseError};
use serde::Serialize;

/// An exact shared RDF profile, compiled into portable checked Datalog rules.
///
/// This is a program description, not an evaluated closure or engine capability claim.
#[derive(Debug, Serialize)]
pub struct CompiledProfile {
    /// The selected semantic contract.
    pub preset: Preset,
    /// The shared rule catalogue's checked program.
    pub program: Program,
    /// Obligations deliberately excluded by this profile.
    pub exclusions: &'static [Exclusion],
}

impl CompiledProfile {
    /// Compile the finite rule catalogue for this preset without loading graph data.
    #[must_use]
    pub fn new(preset: Preset) -> Self {
        Self {
            preset,
            program: preset.program(),
            exclusions: preset.exclusions(),
        }
    }
}

/// Compile a named shared RDF profile, rejecting unknown or unsupported selectors.
///
/// # Errors
/// Returns the shared catalogue's typed parse error for an unsupported selector.
pub fn compile_profile(selector: &str) -> Result<CompiledProfile, PresetParseError> {
    selector.parse().map(CompiledProfile::new)
}

/// Enumerate exactly the finite set of available rule-profile selectors.
pub fn profiles() -> impl ExactSizeIterator<Item = &'static str> {
    Preset::ALL.iter().map(|preset| preset.name())
}
