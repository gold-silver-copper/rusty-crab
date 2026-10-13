fn main() { // Begin the ownership rejection example.
    let value = String::from("fox"); // An owned string is not Copy.
    let moved = value; // Moving ownership makes value unavailable for later use.
    let invalid = value.len(); // Expected E0382: this attempts to borrow the moved value.
} // Close the intentionally rejected function.
