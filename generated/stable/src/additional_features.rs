struct Pair { left: i32, right: i32 } // Named fields support brace-style destructuring assignment.
struct Positional(i32, i32); // A tuple constructor supports positional destructuring assignment.
pub(self) fn restricted() -> i32 { 7 } // An explicit self restriction makes this function visible only within its module.
fn attributed<#[allow(unused_lifetimes)] 'a, #[allow(dead_code)] T, #[allow(unused_variables)] const N: usize>(#[allow(unused_variables)] values: &'a [T; N]) -> usize { // Outer attributes annotate lifetime, type, and const generic parameters as well as an ordinary function parameter.
    values.len() // The generic function returns the length encoded by its array parameter type.
} // Close the attributed generic function.
fn guarded(input: Option<i32>) -> i32 { // A helper exercises simple and chained let guards in match arms.
    match input { // Each arm first matches a value and then checks its own guard.
        value if let Some(number) = value && number > 5 => number, // A let guard and Boolean condition form a conjunction in this match-arm guard.
        value if let Some(number) = value => number + 1, // A direct if-let guard exposes its successful binding to the arm body.
        _ => 0, // The wildcard handles a missing option when both let guards fail.
    } // Close the match containing the two guard forms.
} // Close the match-guard helper.
pub fn demonstrate() { // Run the additional accepted-syntax examples and assertions.
    let (mut left, mut right) = (0, 0); // Create mutable places before assigning through compound assignees.
    [left, right] = [1, 2]; // An array assignee distributes the two incoming values into existing locals.
    assert_eq!((left, right), (1, 2)); // Verify the array destructuring assignment.
    Pair { left, right } = Pair { left: 3, right: 4 }; // A brace-style assignee assigns named fields into existing places.
    assert_eq!((left, right), (3, 4)); // Verify the struct destructuring assignment.
    Positional(left, right) = Positional(5, 6); // A tuple-constructor assignee assigns positional fields without declaring new bindings.
    assert_eq!((left, right), (5, 6)); // Verify the positional destructuring assignment.
    [left, .., right] = [10, 11, 12]; // A rest assignee discards the middle value while assigning the two endpoints.
    _ = Some(9); // An underscore assignee explicitly discards a value without introducing a pattern binding.
    assert_eq!((left, right), (10, 12)); // Verify that the rest assignee preserved only the selected endpoints.
    let loop_value = 'answer: loop { break 'answer 9; }; // The value-carrying labeled break targets a loop rather than a labeled block.
    'finished: { break 'finished; } // A labeled block can also be exited without an explicit value, producing unit.
    let whole @ Some(inner) = Some(2) else { return; }; // An at-pattern binds the complete option and a real nested binding captures its payload.
    let Some(0 | 1) = Some(1) else { return; }; // The alternatives are nested inside a constructor pattern rather than forming a top-level match arm.
    let [first, .., last] = [10, 20, 30]; // An unbound rest subpattern discards the middle of an array through slice-pattern syntax.
    let array_length = attributed(&[1u8, 2, 3]); // Instantiate the attributed type, lifetime, and const parameters.
    assert_eq!((whole, inner, first, last, loop_value), (Some(2), 2, 10, 30, 9)); // Verify the nested binding, slice rest, and loop result.
    assert_eq!((array_length, restricted()), (3, 7)); // Check the attributed generic function and the self-restricted function.
    let guard_results = [guarded(Some(7)), guarded(Some(2)), guarded(None)]; // Exercise the chained guard, the simple guard, and their fallback.
    assert_eq!(guard_results, [7, 3, 0]); // Verify which guarded match arm supplied each result.
} // Close the additional accepted-feature demonstration.
