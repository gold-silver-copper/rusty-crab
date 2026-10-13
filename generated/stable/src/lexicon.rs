//! Literals, identifiers, and comments are lexical forms. // An inner documentation comment describes this module.
/*! A block documentation comment applies to the containing module. */ // Inner block documentation is another spelling of a doc attribute.
/* Outer block /* nested block */ resumes here. */ // Ordinary block comments can nest.
/// Evaluate representative lexical forms. // An outer line documentation comment describes the following function.
pub fn demonstrate() { // A public function declaration opens its body.
    let anonymous: &'_ str = "fox"; // A placeholder lifetime asks the compiler to infer the reference lifetime.
    let integers = (42_u8, 0xff_u16, 0o77_u32, 0b1010_u64, 1_000usize, 42_i16); // Integer bases, digit separators, and explicit type suffixes alter literal spelling.
    let floats = (1.25f32, 2e3_f64, 4f64); // Floating-point literals support fractions, exponents, and integer-looking suffixed forms.
    let inferred_float = 0.5; // An unsuffixed floating-point literal permits type inference and defaults to f64 here.
    let chars = ('f', '\n', '\u{1f980}', b'\x41'); // Character, escaped Unicode, and byte literals have different types.
    let strings = ("fox\tcrab", r#"a "quoted" fox"#, b"fox", br##"raw bytes"##); // Cooked, raw, byte, and raw byte strings use different lexical forms.
    let c_strings = (c"fox", cr#"raw fox"#); // C string literals include a terminating NUL without spelling it in source.
    let r#type = true; // A raw identifier permits a keyword to be used as a variable name; true is a Boolean literal.
    let café = false; // Unicode identifiers are permitted and false is the other Boolean literal.
    assert!(r#type && !café && integers.0 == 42 && floats.0 > 0.0); // A macro invocation checks representative literal values.
    assert_eq!(chars.3, b'A'); // Hexadecimal byte escapes and ordinary byte spelling denote the same value.
    assert_eq!(strings.2, b"fox"); // Byte strings produce references to byte arrays.
    assert_eq!(c_strings.0.to_bytes(), b"fox"); // A method call exposes the C string contents without its terminating NUL.
    assert_eq!(inferred_float * 2.0, 1.0); // Verify the value represented by the directly written unsuffixed float.
} // Close the lexical demonstration function.
