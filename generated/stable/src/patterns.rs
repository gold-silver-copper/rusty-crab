struct Named { x: i32, y: i32 } // A named-field struct supplies data for structural patterns.
struct Wrapped(i32); // A tuple struct supplies a constructor pattern.
const THREE: i32 = 3; // A constant name can be matched as a pattern.
fn extract(value: Option<i32>) -> i32 { // A function accepts an enum that may lack a value.
    let Some(number) = value else { return 0; }; // A refutable let-else pattern requires a diverging else branch.
    number // The successful binding remains available after the let-else statement.
} // Close the let-else example.
pub fn demonstrate() { // Open the pattern demonstration function.
    let Named { x: renamed, y } = Named { x: 1, y: 2 }; // Struct patterns can rename a field or bind it with shorthand syntax.
    let Named { x, .. } = Named { x: 3, y: 4 }; // A struct rest pattern ignores remaining fields.
    let Wrapped(inner) = Wrapped(5); // A tuple-struct pattern destructures a positional field.
    let (first, .., last) = (1, 2, 3, 4); // A tuple pattern can ignore a middle sequence with a rest pattern.
    let &[head, ref tail @ ..] = &[1, 2, 3][..] else { return; }; // A reference and slice pattern binds the remaining slice through an at-pattern.
    let ref borrowed = inner; // A ref binding borrows the matched value instead of moving it.
    let mut value = 7; // Create a mutable local for a mutable reference pattern.
    let &mut copied = &mut value; // An explicit mutable reference pattern matches through a mutable reference.
    let (grouped) = copied; // A parenthesized pattern groups a single pattern without forming a tuple.
    let outcome = match value { // A match expression selects the first matching arm.
        n @ 0..=2 => n, // An at-pattern binds the complete value while an inclusive range restricts it.
        self::THREE | 4 => 30, // A constant pattern and a literal pattern are joined with an or-pattern.
        5..7 => 50, // An exclusive range pattern excludes its upper bound.
        n if n > 6 => n, // A match guard tests an additional condition after binding a pattern.
        _ => 0, // A wildcard pattern makes the match exhaustive for the remaining values.
    }; // Close the match expression and finish its let statement.
    match Some(inner) { Some(n) => assert_eq!(n, 5), None => unreachable!() } // Tuple-variant and unit-variant patterns distinguish enum cases.
    assert_eq!((renamed, y, x, first, last, head, tail.len()), (1, 2, 3, 1, 4, 1, 2)); // Verify the values produced by structural patterns.
    assert_eq!((*borrowed, grouped, outcome, extract(None), extract(Some(9))), (5, 7, 7, 0, 9)); // Verify borrowing, guards, and both branches of the let-else example.
} // Close the pattern demonstration function.
