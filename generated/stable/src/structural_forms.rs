struct Empty {} // An empty named-field struct has braces but no fields.
struct Triple { a: i32, b: i32, c: i32 } // Three fields let the showcase vary field-list cardinality and update syntax.
struct Positional(i32, i32, i32); // A tuple struct can be constructed or destructured using positional field names.
struct Zero(); // A zero-field tuple struct still uses a pair of constructor parentheses.
pub fn demonstrate() { // Run the struct and pattern grammar examples and check their results.
    let Empty {} = Empty {}; // Empty struct construction and an empty struct pattern each omit their optional field list.
    let a = 1; // A local binding provides a value for field shorthand construction.
    let original = Triple { a, b: 2, c: 3, }; // Shorthand and explicit fields coexist; the final comma belongs to the struct expression.
    let updated = Triple { a: 10, ..original }; // An update base supplies the fields not listed explicitly.
    let mut copied = Triple { ..updated }; // A base-only struct expression has no explicit field entries.
    let Triple { .. } = copied; // A rest-only struct pattern discards all fields without naming any.
    let Triple { a: first, b: second, c: third, } = copied; // A three-field pattern has its own optional final comma.
    let Triple { ref a, ref mut b, c: mut last } = copied; // Shorthand ref bindings borrow fields; the explicit final field creates a mutable value binding.
    *b += 1; // Mutate the second field through its exclusive reference.
    last += 1; // Mutate the copied value bound by the explicit field pattern.
    assert_eq!((*a, *b, last), (10, 3, 4)); // Verify shared and exclusive field bindings and the separate mutable copy.
    let Triple { mut a, .. } = copied; // A shorthand mut binding combines an explicitly selected field with a rest pattern.
    a += 1; // The shorthand binding owns its integer copy and can change independently.
    let attributed = Triple { #[allow(unused_variables)] a: 4, #[allow(unused_variables)] #[allow(dead_code)] b: 5, c: 6 }; // One or multiple outer attributes attach to individual construction fields.
    let Triple { #[allow(unused_variables)] a: attr_a, #[allow(unused_variables)] #[allow(unused_mut)] b: attr_b, c: attr_c } = attributed; // Destructuring fields also accept one or multiple outer attributes.
    let Positional { 0: left, 1: middle, 2: right } = Positional { 0: 7, 1: 8, 2: 9 }; // Integer tuple indices name positional fields in brace-style construction and destructuring.
    let Zero() = Zero(); // A tuple-struct pattern with no elements omits its optional pattern list.
    let Positional(p, q, r,) = Positional(7, 8, 9); // Three tuple-struct subpatterns exercise repeated elements and an outer trailing comma.
    let () = (); // An empty tuple pattern omits its optional elements.
    let (single,) = (11,); // A singleton tuple pattern requires its comma to distinguish it from grouping.
    let (..) = (1, 2, 3); // A rest-only tuple pattern matches the entire tuple without individual bindings.
    let (pair_a, pair_b,) = (12, 13); // A tuple pattern with multiple entries can also end in a comma.
    let empty: [i32; 0] = []; // Fix the element type of an empty array before matching it.
    let [] = empty; // An empty slice pattern omits its optional element list.
    let [one] = [14]; // A one-element slice pattern has no repeated comma-and-pattern entries.
    let [start, center, end,] = [15, 16, 17]; // Three slice subpatterns and a trailing comma cover the longer list form.
    assert_eq!((first, second, third, a), (10, 2, 3, 11)); // The field update and copied bindings preserve the expected values.
    assert_eq!((attr_a, attr_b, attr_c), (4, 5, 6)); // Field attributes leave the constructed and destructured values intact.
    assert_eq!((left, middle, right), (p, q, r)); // Brace-style positional patterns and tuple-struct patterns extract equivalent values.
    assert_eq!((single, pair_a, pair_b, one, start, center, end), (11, 12, 13, 14, 15, 16, 17)); // Verify the bindings produced by tuple and slice pattern forms.
} // Close the structural grammar demonstration.
