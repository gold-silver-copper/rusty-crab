//! Attributes attach metadata and compilation requests to items. // This module documents attribute syntax.
#[must_use = "Inspect the returned marker"] // A name-value must-use attribute supplies a diagnostic message.
pub struct Marker; // The must-use attribute applies to values of this unit struct.
#[non_exhaustive] // External crates must allow for future additions to this enum.
pub enum Extensible { First } // A public enum can restrict exhaustive matching outside its crate.
#[deprecated(since = "0.1.0", note = "Use modern instead")] // A deprecated attribute has named string arguments.
pub fn legacy() {} // This function is retained to illustrate deprecation metadata.
#[inline(always)] // An inline attribute can provide an explicit policy argument.
pub fn modern() -> Marker { Marker } // An annotated function returns the must-use type.
#[cold] // A cold attribute marks a function as an unlikely execution path.
fn unlikely() {} // The cold function need not run to demonstrate the attribute syntax.
#[expect(dead_code, reason = "Show a lint expectation")] // An expectation records an anticipated lint and an explanatory reason.
fn deliberately_unused() {} // This unused private item illustrates the expected dead-code lint.
#[track_caller] // This attribute lets the callee observe the caller location.
fn caller_file() -> &'static str { std::panic::Location::caller().file() } // The standard library exposes the tracked source location.
#[unsafe(no_mangle)] // An unsafe attribute exports the exact function name; this chosen symbol is unique in the project.
pub extern "C" fn crab_pangram_export_one() {} // The exported function has an explicit C calling convention.
#[unsafe(export_name = "crab_pangram_export_two")] // An unsafe export-name attribute assigns another unique linker symbol.
pub extern "C" fn exported_alias() {} // The Rust name and exported linker name need not be identical.
#[used] // The used attribute requests retention of a static in the object file.
#[unsafe(link_section = ".crab_pangram_bytes")] // A dedicated ordinary data section avoids reserved runtime section names.
static SECTION_BYTE: u8 = 7; // This initialized byte is stored in the selected section.
#[cfg(unix)] // The following foreign declarations apply only to Unix targets.
#[link(name = "c")] // This foreign module names the C library on Unix systems.
unsafe extern "C" { // Open a foreign declaration block; no foreign function is called in this example.
    #[link_name = "getpid"] // A link-name attribute associates a Rust declaration with a differently named foreign symbol.
    safe fn process_id_alias() -> i32; // This process-ID query has no caller preconditions.
} // Close the foreign declaration block.
#[macro_export] // A macro-export attribute exposes a declarative macro at the crate root.
macro_rules! crab_exported_macro { () => { 7 }; } // The exported macro expands to a literal expression.
#[macro_use] // A macro-use attribute imports macros from the following inline module.
mod definitions { // Open a module containing a local declarative macro.
    macro_rules! local_seven { () => { 7 }; } // This macro will be usable after the macro-use module declaration.
} // Close the macro definition module.
pub fn demonstrate() { // Open the attribute demonstration function.
    let marker = modern(); // Bind the must-use return value so it is not discarded.
    let tracked = caller_file(); // Call the function that receives caller-location information.
    assert!(tracked.ends_with("attributes.rs")); // The tracked caller is this source file.
    assert_eq!(local_seven!(), crate::crab_exported_macro!()); // Both local macro import and exported macro paths resolve successfully.
} // Close the attribute demonstration function.
