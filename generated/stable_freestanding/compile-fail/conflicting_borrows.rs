fn main() { // Begin the incompatible-borrow rejection example.
    let mut value = 1; // A mutable local can be borrowed under the ownership rules.
    let shared = &value; // This shared borrow remains live until its later use.
    let exclusive = &mut value; // Expected E0502: a mutable borrow conflicts with the live shared borrow.
    println!("{}", shared); // This use keeps the shared borrow live across the preceding line.
} // Close the intentionally rejected function.
