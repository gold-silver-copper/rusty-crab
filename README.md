# Rusty Crab

Rusty Crab generates an annotated Rust showcase and audits it against an inventory extracted from a pinned Rust compiler and its matching Reference. The auditor inspects actual compiler data independently of the generator's example labels.

**The implementation is in progress. It does not yet demonstrate every applicable requirement.** Normal auditing reports the gaps; strict auditing fails until they are resolved. The full objective is in [rust-pangram-prompt.txt](rust-pangram-prompt.txt).

## Run the project

Run these commands from this repository:

```sh
cargo run --locked -- inventory extract
cargo run --locked -- generate --profile stable
cargo run --locked -- audit --profile stable
cargo run --locked -- report
cargo test --locked
```

The audit compiles and runs the generated core, checks its exact `pangram: ok` output, inspects compiler observations, and validates the intended diagnostics of the negative fixtures. `--no-run` performs inspection without executing the core; it cannot establish runtime coverage.

```sh
cargo run --locked -- audit --profile stable --strict
```

The strict command currently returns a nonzero exit status, because the complete inventory still has missing, unsupported, and unclassified requirements. An ordinary audit returning zero means its validation workflow ran successfully; consult its report for coverage completeness.

Outputs are inspectable:

- [generated/stable/src/main.rs](generated/stable/src/main.rs) is the generated entry point. Its modules cover lexical forms, items and traits, expressions, patterns, macros, and attributes.
- [generated/stable/compile-fail](generated/stable/compile-fail) contains intentionally rejected ownership examples.
- [artifacts/inventory.json](artifacts/inventory.json) contains candidates, classifications, descriptions, source provenance, and detector identities.
- [artifacts/audit.json](artifacts/audit.json) contains coverage outcomes, compiler evidence, input hashes, and validation results.
- [artifacts/report.txt](artifacts/report.txt) maps requirements to source witnesses and explains gaps.

The generated core can also run independently:

```sh
cargo run --manifest-path generated/stable/Cargo.toml
```

Do not run a formatter over generated source as part of generation. The renderer deliberately preserves one explanatory comment per physical line; formatting may separate comments from the constructs they explain. Change [catalog/stable.json](catalog/stable.json) or [catalog/negative.json](catalog/negative.json), then regenerate.

`generate`, `audit`, and `report` accept `--project DIR`. Audit artifacts can go to a different location with `--artifacts DIR`; `--inventory FILE` selects a saved inventory. The default project is `generated/stable` and the default artifacts directory is `artifacts`.

## Freestanding companion

The `stable_freestanding` profile generates a separate `no_std`/`no_main` binary crate with an explicit panic handler and no implicit library prelude. It compiles to a host-target object file using `-Cpanic=abort`. It exports a small C-ABI function for a future host, but supplies neither a host nor an operating-system entry point. The auditor never links or executes this profile.

```sh
cargo run --locked -- generate --profile stable_freestanding
cargo run --locked -- audit --profile stable_freestanding
cargo run --locked -- report --profile stable_freestanding
```

The default project is [generated/stable_freestanding](generated/stable_freestanding), and its reports go to [artifacts/stable_freestanding](artifacts/stable_freestanding). Both implemented profiles use the same stable-language inventory. Each report covers only its own configuration; evidence from the companion is not silently combined with the runnable core. Unused requirements remain missing, not excluded. The companion's `ran` field is always false; successful validation requires actual object generation in addition to metadata compilation. `--no-run` does not skip that object check.

To compile its object independently, without the auditor's lint cap:

```sh
mkdir -p generated/stable_freestanding/target
rustc --edition=2024 --crate-type=bin --crate-name=pangram -Cpanic=abort --emit=obj generated/stable_freestanding/src/main.rs -o generated/stable_freestanding/target/pangram.o
```

The panic handler is intentionally divergent and is never invoked by the audit. No runtime, linking, allocator, or target-startup claim follows from producing the object. Negative ownership fixtures are separate crates, may use `std`, and are checked under the selected profile's actual flags. `no_link` still needs a separate macro-provider example; importing `std` into the panic-handler crate introduces a conflicting panic definition even with that attribute.

## Compiler and source pins

The project uses Rust **1.99.0**, revision `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`. The Reference revision is `82da570bafd9efed421bdbae0f8603ede8dd308b`, obtained from that compiler revision's `src/doc/reference` submodule entry.

[rust-toolchain.toml](rust-toolchain.toml) requests the matching `rustc-dev`, `rust-src`, and `rustfmt` components. Rustup installs them when needed. The current build configuration uses an ELF linker rpath and has been validated on `aarch64-unknown-linux-gnu`; other host platforms need build validation and possibly linker adjustments.

Compiler-private APIs are experimental even in the stable compiler's development libraries. [.cargo/config.toml](.cargo/config.toml) sets `RUSTC_BOOTSTRAP=rusty_crab` solely to compile the auditor crate against those APIs. The compiler worker and all audited compiler invocations remove that variable. The generated programs are checked using ordinary stable feature gating. A build-time revision check prevents accidentally building this auditor against a different compiler.

Selected compiler source is vendored under `vendor/rust`; the matching Reference source is under `vendor/reference`. Their original license files are retained. The source locks in `config` hash the complete vendored trees. Added, removed, or changed files require explicit source-lock review. No network access is needed for extraction or auditing once the toolchain and Cargo dependencies are installed.

## What the evidence means

The inventory has several deliberately distinct sources:

| Source | Extracted candidates |
| --- | --- |
| Compiler AST definitions | Public enum variants, including operators and modifiers |
| Compiler lexer | Token, literal, base, and documentation-comment categories |
| Compiler symbol table | Keyword spellings, including reserved and weak keywords |
| Feature tables | Accepted, unstable, internal, and removed feature gates |
| Built-in attribute table | Attribute names; argument forms require further interpretation |
| Feature compatibility tables | Gate dependencies and incompatible combinations |
| Matching Reference | Named grammar productions, preserving their full definitions |
| Curated semantic specifications | Selected type-checker adjustments and expected diagnostics |

These are candidates for interpretation, not a claim that every compiler enum variant is a distinct language feature. [config/rules.json](config/rules.json) records explicit classifications. Unknown candidates remain unclassified. Error-recovery states and synthetic nodes are distinguished from writable syntax. Unstable and removed gates remain visible even when inapplicable to the stable profile.

Evidence is collected at different stages:

- **Lexical presence:** `rustc_lexer` sees an actual source token. Strings and comments cannot fake other token categories. A reserved keyword inside a macro's token input demonstrates its spelling, not a stabilized language meaning.
- **Active syntax:** the expanded AST is inspected before lowering, after conditional compilation. A source-syntax witness must also match the original unexpanded AST at the same location. This prevents macro-generated declarations from gaining credit through forwarded keyword spans. Syntax inside opaque macro input remains conservatively uncredited; the catalog demonstrates those forms directly. Outlined module declarations are explicitly reconciled with their loaded compiler form. External modules within the project are inspected; imported source outside the project does not provide coverage.
- **Resolved pattern syntax:** identifier and path patterns use the compiler's name-resolution table, recorded as `resolved_ast` evidence. A lowercase constant name does not count as a fresh binding; the same source spelling can become a binding when its declaration changes. The independent source parse supplies possible grammatical interpretations, which must match the active compiler interpretation. Documentary function-pointer parameter names, `_:` labels, `self` parameters, and shorthand struct fields do not count as standalone identifier/wildcard patterns. Actual patterns nested inside a parameter type's const expression remain eligible.
- **Macro invocation syntax:** an independent parse supplies the original invocation's grammatical position. It counts only when matched to an actual compiler expansion at the same source location. Evidence records macro names and expansion provenance. Empty or token-forwarding expansions that leave no usable hygiene trace can remain undetected; a parsed call by itself never establishes active use.
- **Accepted syntax features:** interpreted contracts for 38 accepted gates require concrete AST forms. Requirements with multiple forms need every listed form: for example, both raw-borrow mutabilities, let-chains in both `if` and `while`, associated types with lifetime/type/const parameters, and distinct free/inherent async functions. Further contracts distinguish destructuring assignments by assignee shape, loop break values from labeled-block exits, nested or-patterns from top-level alternatives, slice rest forms, parameter versus field attributes, restricted visibility forms, and simple versus chained match-arm let guards. An inner binding under `@` must be confirmed by name resolution; a constant or constructor name cannot substitute. Partial evidence remains visible alongside the missing forms. These syntactic witnesses do not claim runtime polling, arbitrary const evaluation, or exhaustive combinations.
- **Reference grammar:** 82 reviewed productions currently have 301 local form requirements extracted from their EBNF structure. These cover alternatives, optional groups both present and absent, and repetitions with zero/one/multiple occurrences where permitted. Compiler visitors check the corresponding expression/type contexts; lexer checks distinguish trailing commas from commas inside nested expressions. A tuple's number of comma-terminated elements is checked separately from its arity. Type mappings include explicit references and pointers, arrays, slices, and function pointers with ABI spellings, higher-ranked binders, named parameters, attributes, and variadic parameter lists. Shorthand receivers and compiler-inferred closure types do not count as written reference or underscore types. Struct construction and destructuring mappings distinguish shorthand fields, tuple-index fields, attribute counts, update bases, and rest patterns. Tuple and slice pattern lists are observed only in pattern contexts; their delimiters and trailing commas are checked independently of similar expression and type forms. The compiler also accepts a variadic function-pointer type with no fixed parameters, but the pinned Reference production requires a fixed parameter; that compiler extension does not earn evidence for the Reference production or its parents. Literal and modern range patterns are checked in their pattern contexts; a pattern's optional minus does not count as an arithmetic negation expression, and constant-path range bounds remain distinct from literal bounds. Referenced productions have their own requirements, and arbitrary combinations of local choices are outside this coverage metric.
- **Type-checked behavior:** compiler adjustment tables establish array-to-slice, concrete-reference-to-trait-object, and closure-to-function-pointer coercions. Merely writing the destination type does not establish a coercion.
- **Expected rejection:** the negative examples must produce exactly the intended coded error at a primary span in the fixture. An unrelated error cannot satisfy the requirement.
- **Runtime validation:** the core must exit successfully and produce the exact expected marker. Its assertions check selected results; running the core does not establish per-feature runtime coverage of every declared function.

AST evidence is accepted only after the compiler accepts the core. Each witness includes a project-relative file, original byte offsets, line and column, detector, stage, origin, and configuration identity. Original byte offsets account for UTF-8 BOMs and CRLF normalization. Compiler-read local source files outside the project are tracked for freshness without receiving coverage credit.

Canonical artifacts use sorted collections, relative paths, content hashes, and no timestamps. Reports record the compiler revision, host target, edition, built-in `cfg` settings, enabled feature gates, and relevant flags. The report command rejects changed project sources, changed compiler inputs, stale inventories, and a changed auditor implementation.

Audit schema 3 records the bytes actually delivered through rustc's file loader, including `include_bytes!` and `include_str!` inputs outside the project, plus file-existence probes used during module lookup. Repeated reads must agree; retargeting an external symlink invalidates evidence when its contents change. `env!` and `option_env!` dependencies come from the compiler's environment dependency records. Reports store value hashes or explicit absence, never the values themselves. The effective environment accounts for the worker's cleared `RUSTC_BOOTSTRAP`. Inputs from the core, negative fixtures, and executable or object build are checked together. Workers also report their compiler configuration, which must match the requested profile. Actual `cfg` values come from the compiler with those same flags, including the panic strategy. A report saved under another profile is rejected. Earlier audit schemas must be regenerated.

## Extending coverage

Add or refine a requirement before adding its example. Implement an independent detector, then add annotated catalog rows and a negative control that would expose a false positive. Generator intent metadata is navigation material only; the auditor never uses it as proof.

Every generated source line has a real comment token. Annotation validation checks lexical placement and a minimum nonempty explanation; the educational accuracy of the prose still needs review. `syn` checks rendered syntax, while `rustc` decides whether the program is accepted.

Reference mappings use `rustc_reference_v1` with a `grammar_shape_hash` in `config/rules.json`. The extractor derives required form IDs for every production written in the supported EBNF core, including those without reviewed compiler mappings. These unmapped candidates stay unclassified. The configuration cannot substitute a shorter form list for a Reference mapping. Updating vendored Reference hashes without reviewing a changed production leaves its mapping unclassified and disables its detector. Unhandled grammar notation also remains unclassified, with its extraction limitation recorded. Lexical character sets, bounded repetitions, cuts, prose restrictions, and other unmapped constructs still need implementation. `inventory diff` reports changed form IDs as well as changed productions.

The test suite covers deletion of the sole witness, forged metadata, comments and strings, disabled `cfg` branches, macro expansions and source invocations, external modules and source changes, unrelated negative diagnostics, type-based coercion evidence, normalized source positions, repeated generation and auditing, and strict rejection of stale or unknown inventory data. It also checks included binary/text files, environment values and absence, dependencies exclusive to negative fixtures, changed symlinks, and conflicting reads. Feature tests distinguish similar constructs and grammatical contexts: a generic trait does not make its associated type generic, and a closure returning an async block is not an async closure. They also follow labeled break destinations across nested loops/blocks, reject tuple declarations as destructuring assignments, distinguish nested guard blocks from guard let-chains, and require attribute and visibility forms in their actual contexts. Patterns inside const expressions nested in a pattern path start their own pattern roots.

Grammar tests require every range alternative, distinguish nested commas from a list's own trailing comma, count tuple repetitions correctly, distinguish function-pointer types from function declarations, separate fixed-parameter attributes from ellipsis attributes, recognize empty written lifetime binders, resolve identical pattern names differently when their declarations change, reject documentary names as patterns, separate negative literal patterns from arithmetic expressions, distinguish base-only struct updates from fields followed by an update base, require shorthand pattern modifiers in their proper positions, distinguish rest-only tuples with and without a comma, and simulate a new Reference alternative to verify that an old mapping cannot silently cover it.

Profile tests independently compile the freestanding object, require repeatable reports with no execution claim, verify `cfg(panic)` behavior in positive and negative inputs, reject reports from another configuration, and deliberately trigger an assembler error to prove metadata acceptance cannot substitute for successful object generation.

The remaining work is substantial:

1. Interpret the remaining compiler candidates and extend Reference mapping beyond the 82 reviewed productions, including contextual restrictions and richer grammar notation. Seeing one AST variant does not complete an entire grammar production.
2. Extend macro evidence to expansions without retained hygiene, parsed macro argument fragments, custom procedural macros, and attribute expansion; expand attribute and modifier coverage.
3. Define individual witnesses for the remaining applicable accepted feature gates. Their historical names are not themselves evidence of use. Syntax contracts live in `config/rules.json`; compiler predicates live in `src/features.rs`.
4. Extend the freestanding companion beyond object generation with explicit host/link/runtime contracts where appropriate. Add configurations for custom procedural macros, additional editions, nightly experiments, and target-specific constructs. Their boundaries are recorded in [config/profiles.json](config/profiles.json).
5. Extend dependency tracking when procedural macros, native link inputs, or runtime resources introduce dependencies beyond compiler file loading and recorded environment access; extend independent tests alongside the detectors.

These gaps remain in the report and block an exhaustive completeness claim. They must not be reclassified as inapplicable simply to obtain a successful strict audit.

## Compiler upgrades

Keep the old inventory before changing pins. Fetch the new compiler sources and its matching Reference submodule revision, review source locks, update the compiler-private integration and interpretation rules, and regenerate an inventory. The exhaustive generated discriminant matches and source-tree locks make mismatches explicit.

```sh
cargo run --locked -- inventory diff old-inventory.json new-inventory.json
```

The diff lists added, removed, and changed requirements, plus source, Reference, and rule changes requiring review. A successful upgrade must include detector validation and renewed evidence; an unchanged requirement count is not sufficient.
