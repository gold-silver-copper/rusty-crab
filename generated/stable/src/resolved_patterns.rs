const LOWER: i32 = 3; // A named constant can serve as a range-pattern bound.
const UPPER: i32 = 5; // This constant supplies the excluded upper end of the constant-bounded range.
struct Marker; // A unit struct name can denote a path pattern rather than introducing a new binding.
fn classify(value: i32) -> u8 { // A parameter identifier introduces a local binding according to the compiler resolver.
    match value { // Select a result using four complementary ranges and a single negative literal.
        -3 => 1, // A minus before a literal belongs to LiteralPattern rather than to an arithmetic negation expression.
        ..-3 => 2, // An upper-exclusive range pattern matches all integers below negative three.
        -2..=2 => 3, // An inclusive bounded range pattern includes both written endpoints.
        LOWER..UPPER => 4, // An exclusive bounded range pattern resolves its endpoints as constant paths.
        5.. => 5, // A lower-bounded range pattern matches five and every larger integer.
    } // Close the exhaustive integer match.
} // Close the range classification function.
fn nonnegative(value: i32) -> i32 { // Accept another ordinary identifier binding.
    match value { ..=0 => 0, positive => positive } // An upper-inclusive range pattern handles nonpositive inputs and a fresh binding handles the remainder.
} // Close the nonnegative conversion function.
fn marker(value: Marker) -> bool { // Accept a unit struct value without inspecting fields.
    match value { Marker => true } // The resolver identifies Marker as a constructor path, not a newly bound variable.
} // Close the constructor-path pattern example.
pub fn demonstrate() { // Run the literal, range, and resolved-name pattern examples.
    let actual = [classify(-4), classify(-3), classify(0), classify(3), classify(5)]; // These calls exercise the distinct match arms; their negative arguments are actual negation expressions.
    assert_eq!(actual, [2, 1, 3, 4, 5]); // Verify the selected result for each range or literal branch.
    let bounded = [nonnegative(-7), nonnegative(0), nonnegative(9)]; // Exercise the upper-inclusive range and the fallback identifier binding.
    assert_eq!(bounded, [0, 0, 9]); // Verify that only positive inputs survive the conversion.
    let is_marker = marker(Marker); // Construct the unit struct and pass it through the path-pattern match.
    assert!(is_marker); // Verify that the unit constructor pattern matches its value.
} // Close the resolved-pattern demonstration.
