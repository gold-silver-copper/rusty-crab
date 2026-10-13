fn sum3(a: i32, b: i32, c: i32) -> i32 { a + b + c } // A three-argument function supplies a call with multiple repeated argument separators.
pub fn demonstrate() { // Open examples selected by the Reference grammar gap report.
    let empty: [u8; 0] = []; // An empty array expression omits its optional ArrayElements group.
    let single = [1]; // One listed array element uses zero comma-expression repetitions and no trailing comma.
    let pair = [1, 2,]; // Two array elements use one comma-expression repetition and an optional trailing comma.
    let unit: () = (); // The unit tuple type and expression both omit their element lists.
    let singleton: (i32,) = (3,); // A one-element tuple needs its comma; the trailing optional type or expression is absent.
    let tuple: (i32, i32,) = (4, 5,); // Two comma-terminated tuple elements demonstrate multiple repetitions and no final uncommaed element.
    let sum = sum3(1, 2, 3,); // A call with three arguments demonstrates multiple comma-argument repetitions and a trailing comma.
    loop { break; } // An unlabeled break without a value exits this infinite-loop expression immediately.
    let mut visits = 0; // Count iterations that reach the body after its continue condition.
    for n in 0..2 { if n == 0 { continue; } visits += 1; } // A continue expression without a label skips to the next iteration of the innermost loop.
    assert_eq!((empty.len(), single, pair, unit, singleton, tuple, sum, visits), (0, [1], [1, 2], (), (3,), (4, 5), 6, 1)); // Verify the collection, call, and control-flow results represented by the grammar examples.
} // Close the Reference grammar demonstrations.
