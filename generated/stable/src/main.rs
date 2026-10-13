//! A generated, compiler-audited Rust language showcase.
#![allow(dead_code, unused_imports, unused_variables, unused_parens, redundant_semicolons)] // Permit deliberately illustrative forms.
#![warn(unused_must_use)] // Request warnings for unused results of must-use values.
#![deny(unsafe_op_in_unsafe_fn)] // Require unsafe operations to have explicit unsafe blocks.
#![forbid(unused_unsafe)] // Forbid unnecessary unsafe blocks in this crate.
#![recursion_limit = "128"] // Set the compiler recursion limit for this crate.
#![type_length_limit = "1000000"] // Declare a bound on type substitutions; enforcement depends on compiler settings.
#![crate_name = "pangram"] // Give the crate the same name used by its manifest and compiler invocation.
#![crate_type = "bin"] // Declare this crate as an executable binary.
mod lexicon; // Load the lexicon examples from an external module.
mod items; // Load the items examples from an external module.
mod expressions; // Load the expressions examples from an external module.
mod patterns; // Load the patterns examples from an external module.
mod macros; // Load the macros examples from an external module.
mod attributes; // Load the attributes examples from an external module.
mod feature_forms; // Load the feature_forms examples from an external module.
mod trait_forms; // Load the trait_forms examples from an external module.
mod grammar_forms; // Load the grammar_forms examples from an external module.
mod type_forms; // Load the type_forms examples from an external module.
mod structural_forms; // Load the structural_forms examples from an external module.
mod resolved_patterns; // Load the resolved_patterns examples from an external module.
mod additional_features; // Load the additional_features examples from an external module.
fn main() { // The executable entry point runs each demonstration.
    lexicon :: demonstrate () ; // Run the lexicon demonstration and its assertions.
    items :: demonstrate () ; // Run the items demonstration and its assertions.
    expressions :: demonstrate () ; // Run the expressions demonstration and its assertions.
    patterns :: demonstrate () ; // Run the patterns demonstration and its assertions.
    macros :: demonstrate () ; // Run the macros demonstration and its assertions.
    attributes :: demonstrate () ; // Run the attributes demonstration and its assertions.
    feature_forms :: demonstrate () ; // Run the feature_forms demonstration and its assertions.
    trait_forms :: demonstrate () ; // Run the trait_forms demonstration and its assertions.
    grammar_forms :: demonstrate () ; // Run the grammar_forms demonstration and its assertions.
    type_forms :: demonstrate () ; // Run the type_forms demonstration and its assertions.
    structural_forms :: demonstrate () ; // Run the structural_forms demonstration and its assertions.
    resolved_patterns :: demonstrate () ; // Run the resolved_patterns demonstration and its assertions.
    additional_features :: demonstrate () ; // Run the additional_features demonstration and its assertions.
    println!("pangram: ok"); // Print the deterministic runtime verification marker.
} // Close the executable entry point.
