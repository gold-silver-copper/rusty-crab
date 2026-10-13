macro_rules! scalar_type { () => { u32 }; } // A macro can produce type syntax.
macro_rules! scalar_pattern { () => { 7 }; } // A macro can produce pattern syntax.
macro_rules! module_item { () => { fn generated_value() -> u32 { 7 } }; } // A macro can produce a module-level function item.
module_item!(); // Invoke an item-producing macro at module scope.
macro_rules! associated_item { () => { fn generated() -> u32 { 7 } }; } // A macro can supply an associated function with a default body.
trait Generated { associated_item!(); } // An associated-item macro is expanded inside a trait declaration.
impl Generated for () {} // The unit type uses the generated default trait method.
macro_rules! foreign_item { () => { fn crab_unlinked(); }; } // A macro can produce a foreign function declaration.
unsafe extern "C" { foreign_item!(); } // A macro invocation is also allowed in a foreign item position; no symbol is called.
macro_rules! unit_expression { () => { () }; } // This macro emits a unit expression rather than forwarding its input.
fn no_semicolon() { unit_expression!() } // A macro statement without braces or a semicolon can become the block tail expression.
macro_rules! root_path { () => { $crate::main }; } // The special dollar-crate metavariable names the crate defining a declarative macro.
macro_rules! sum { // A declarative macro matches and expands token trees.
    ($($number:expr),+ $(,)?) => { 0 $(+ $number)+ }; // Repetition, expression fragments, separators, and an optional trailing comma form a macro rule.
} // Close the declarative macro definition.
macro_rules! identity_tokens { // A second macro demonstrates token-tree forwarding.
    ($value:tt) => { $value }; // A token-tree metavariable can be substituted into an expansion.
} // Close the token-forwarding macro.
macro_rules! swallow { // A token-tree macro can accept tokens that are not standalone Rust expressions.
    ($($token:tt)*) => { () }; // Zero-or-more repetition consumes its input and expands to the unit value.
} // Close the swallowing macro.
#[cfg(any())] // An always-false cfg condition disables the following item in this configuration.
fn disabled() { loop {} } // This loop is lexical source presence only and must not establish active loop coverage.
pub fn demonstrate() { // Open the macro demonstration function.
    let typed: scalar_type!() = 7; // Invoke a macro in a type position before type checking.
    match typed { scalar_pattern!() => (), _ => panic!("unexpected value") } // Invoke a macro in a pattern position before checking match exhaustiveness.
    unit_expression! {}; // A macro statement may use braces and a trailing semicolon.
    unit_expression! {} // A brace-delimited macro statement can omit its semicolon.
    assert_eq!(generated_value(), <() as Generated>::generated()); // Check the generated free function and generated trait default.
    let _entrypoint: fn() = root_path!(); // Expand the dollar-crate path and coerce the referenced function item to a pointer without calling it recursively.
    swallow!(abstract become box do final macro override priv typeof unsized virtual yield gen try auto builtin catch contract_ensures contract_requires default reuse yeet); // Reserved and weak keyword spellings are valid macro input tokens; this does not activate their reserved or unstable language meanings.
    let result = sum!(1, 2, 3,); // A function-like macro invocation passes expression fragments with a trailing comma.
    let literal = identity_tokens![7]; // A macro invocation may use square-bracket delimiters.
    swallow! { ~ }; // Brace-delimited macro input may contain a tilde token without treating it as an expression.
    assert_eq!((result, literal), (6, 7)); // Verify the runtime values produced by macro expansion.
} // Close the macro demonstration function.
