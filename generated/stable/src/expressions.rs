fn fallible() -> Result<i32, &'static str> { // A Result-returning function enables error propagation with question mark.
    let value = Ok::<i32, &str>(3)?; // The try operator unwraps success or returns early on error.
    return Ok(value); // An explicit return expression leaves the function.
} // Close the fallible function.
async fn asynchronous() -> i32 { // An async function returns a future that yields its declared output.
    let owned = String::from("fox"); // An owned string can be captured by an async move block.
    async move { owned.len() as i32 }.await // An async move block captures ownership, and await obtains its result.
} // Close the async function; it is type-checked but not polled by this example.
fn diverges() -> ! { loop {} } // The never return type describes a function that does not return; this function is not called.
fn sequence() -> impl Iterator<Item = i32> { 0..3 } // Return-position impl Trait hides the concrete iterator type.
pub fn demonstrate() { // Open the expression demonstration function.
    fn local_item() {} // An item declaration is also permitted as a statement inside a block.
    let grouped_type: (i32) = 1; // Parentheses can group a type without forming a one-element tuple.
    let mut x: i32; // A declaration can defer initialization while fixing the variable's type.
    x = 4; // An assignment initializes the previously uninitialized variable.
    let arithmetic = (x + 2, x - 2, x * 2, x / 2, x % 3); // Arithmetic binary operators have separate syntax and precedence rules.
    let bits = (x & 3, x | 3, x ^ 3, x << 1, x >> 1); // Bitwise operators combine bits or shift them by an integer count.
    let comparison = (x == 4, x != 5, x < 5, x <= 4, x > 3, x >= 4); // Equality and ordering comparisons produce Boolean values.
    let boolean = true && false || true; // Logical and and or short-circuit evaluation of their right operands.
    x += 2; x -= 1; x *= 2; x /= 2; x %= 4; // Arithmetic compound assignments update a place in place.
    x &= 3; x |= 4; x ^= 1; x <<= 1; x >>= 1; // Bitwise compound assignments have their own operator forms.
    let negative = -x; // Unary negation reverses the sign of a signed integer.
    let inverted = !false; // Unary not negates a Boolean and also supports bitwise integer inversion.
    let shared = &x; // A shared borrow creates a reference to a live local variable.
    let copied = *shared; // Dereference reads through a reference to a Copy value.
    let unique = &mut x; // A mutable borrow permits exclusive mutation through the reference.
    *unique = copied + 1; // A dereference expression can be the destination of assignment.
    let raw_const = &raw const x; // A raw const borrow forms a raw pointer without a reference result.
    let raw_mut = &raw mut x; // A raw mutable borrow forms a mutable raw pointer.
    let tuple = (1, true, "fox"); // A tuple combines values of different types.
    let array = [1, 2, 3]; // An array expression explicitly lists its elements.
    let repeated = [0_u8; 4]; // A repeat expression fills a fixed-size array from one value.
    let coerced: &[i32] = &array; // The compiler inserts an unsizing coercion from an array reference to a slice reference.
    let slice: &[i32] = &array[..]; // A full range indexes the whole array and coerces it to a slice.
    let ranges = (0..2, 0..=2, 0.., ..2, ..=2, ..); // Range expressions allow inclusive, exclusive, and absent endpoints.
    let condition = if x > 0 { 1 } else if x == 0 { 0 } else { -1 }; // If and else branches form an expression with a common result type.
    let scoped = { let value = 2; value + 1 }; // A block expression introduces scope and returns its tail expression.
    let constant = const { 2 + 2 }; // An inline const block is evaluated during compilation.
    let grouped = (x + 1) * 2; // Parentheses group a subexpression and change precedence.
    let mut total = 0; // A mutable binding can be updated by successive loops.
    'outer: for n in 0..4 { // A labeled for loop iterates through a range.
        if n == 1 { continue 'outer; } // A labeled continue starts the next iteration of the named loop.
        total += n; // The accumulator is updated with a compound assignment.
    } // Close the for-loop body.
    while total < 6 { total += 1; } // A while loop repeats while its Boolean condition remains true.
    let result = loop { break total; }; // A break expression can supply the value of a loop expression.
    let labeled = 'value: { break 'value 7; }; // A labeled block can be exited with a result value.
    let mut option = Some(1); // An enum constructor produces an Option value.
    while let Some(value) = option { total += value; option = None; } // A while-let loop continues while a refutable pattern matches.
    if let Some(value) = Some(2) && value > 0 { total += value; } // A let-chain combines pattern binding with a Boolean condition.
    assert_eq!(total, 9); // Check the updates made by the while-let loop and the let-chain.
    let closure = |n: i32| -> i32 { n + 1 }; // A closure can explicitly annotate parameter and return types.
    let captured = String::from("fox"); // Create a value whose ownership will be captured by a closure.
    let moved = move || captured.len(); // A move closure captures its environment by value.
    let function: fn(i32) -> i32 = |n| n; // A noncapturing closure coerces to a function pointer.
    let inferred: _ = function(closure(1)); // An inferred type placeholder delegates the type choice to the compiler.
    let (mut a, mut b) = (0, 0); // A tuple pattern introduces multiple mutable local bindings.
    assert_eq!((a, b), (0, 0)); // Read the initial bindings before destructuring assignment replaces their values.
    (a, b, _) = (1, 2, 3); // Destructuring assignment updates existing places and ignores one value.
    ; // An empty statement is permitted as a standalone semicolon.
    assert_eq!(fallible().unwrap(), 3); // A successful fallible call can be checked at runtime.
    assert_eq!((result, labeled, moved(), inferred, a, b), (6, 7, 3, 2, 1, 2)); // Verify the values produced by control flow, closures, and assignment.
    assert_eq!(sequence().sum::<i32>(), 3); // A method turbofish chooses the sum result type of an opaque iterator.
} // Close the expression demonstration function.
