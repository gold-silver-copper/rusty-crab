struct Empty {} // An empty braced struct has a different spelling from a unit struct.
enum EmptyVariant { Braced {} } // An enum variant can use empty braces rather than unit syntax.
struct Pair(i32, i32); // A tuple struct provides positional fields for a rest pattern.
struct Named { value: i32 } // A named field can be initialized using a same-named local binding.
pub fn demonstrate() { // Open the additional accepted-feature syntax demonstrations.
    let projected = (2, 3).0; // A numeric field projection reads the first tuple element directly in source syntax.
    let value = 5; // Bind a name matching the following struct field.
    let named = Named { value }; // Field initialization shorthand uses value for both the field name and its expression.
    let Pair(first, ..) = Pair(3, 4); // A tuple-struct rest pattern ignores its remaining positional fields.
    let lower = match -1_i32 { ..=0 => true, _ => false }; // A half-open inclusive range pattern accepts any value at or below its upper bound.
    let mut total = 0; // Accumulate deterministic results from conditional pattern matching.
    if let Some(number) = Some(2) { total += number; } // A plain if-let condition binds a value only when its pattern matches.
    if let Some(1) | Some(2) = Some(2) { total += 1; } // A top-level or-pattern allows either alternative in an if-let condition.
    let mut option = Some(1); // Initialize an option for a single successful while-let iteration.
    while let Some(1) | Some(2) = option { total += 1; option = None; } // A top-level or-pattern controls the while-let loop and changing the option stops it.
    let mut chain = Some(2); // Initialize a value for the while-let-chain condition.
    while let Some(number) = chain && number > 0 { total += number; chain = None; } // A while let-chain combines a pattern binding with a Boolean test of the bound value.
    let _empty = (Empty {}, EmptyVariant::Braced {}); // Construct both empty braced forms so their declarations have visible uses.
    assert_eq!((named.value, first, lower, total), (5, 3, true, 6)); // Verify shorthand initialization, tuple-struct destructuring, range matching, and conditional updates.
    assert_eq!(projected, 2); // Verify the value read by the direct tuple-field projection.
} // Close the additional feature syntax demonstrations.
