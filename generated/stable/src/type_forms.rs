type Plain = fn(); // A function pointer type can omit parameters, qualifiers, and its unit return type.
type Unsafe = unsafe fn(); // Calling an unsafe function pointer requires its caller to uphold the function contract.
#[allow(missing_abi)] // This example deliberately omits an ABI name to demonstrate the implicit C ABI spelling.
type ImplicitAbi = extern fn(); // An extern function pointer without a string uses the default C ABI.
type ExplicitAbi = extern "C" fn(); // A cooked string explicitly selects the C calling convention.
type RawAbi = extern r"C" fn(); // A raw string can spell the same explicit ABI name.
type Borrow = for<'a> fn(&'a u32) -> &'a u32; // A higher-ranked binder makes this pointer accept a reference with any caller-chosen lifetime.
type EmptyBinder = for<> fn(); // A written higher-ranked binder can contain no generic parameters.
type Named = fn(value: u32, _: u32); // Function pointer parameters can have documentary names or an underscore name.
type Attributed = fn(#[allow(unused_variables)] u32, #[allow(unused_variables)] #[allow(dead_code)] u32, u32,); // Outer attributes attach to individual parameters; the final comma is optional.
type VariadicOne = unsafe extern "C" fn(i32, ...); // A variadic C function pointer type has one fixed parameter and an ellipsis; no call is made.
type VariadicTwo = unsafe extern "C" fn(i32, i32, #[allow(unused_variables)] ...); // Two fixed parameters precede an ellipsis carrying one outer attribute.
type VariadicThree = unsafe extern "C" fn(i32, i32, i32, #[allow(unused_variables)] #[allow(dead_code)] ...); // Three fixed parameters and two ellipsis attributes demonstrate repeated grammar elements.
fn identity(value: &u32) -> &u32 { value } // Lifetime elision ties the returned reference to the only input reference.
pub fn demonstrate() { // Run the portable type examples while leaving foreign and variadic pointer types uncalled.
    let borrow: Borrow = identity; // A function item coerces to a pointer whose signature has a higher-ranked lifetime.
    let mut value = 7u32; // A mutable local owns the value used by the reference and pointer examples.
    let shared: &u32 = borrow(&value); // An explicitly written shared reference type describes the returned borrow.
    assert_eq!(*shared, 7); // The selected function pointer preserves the referenced value.
    let unique: &mut u32 = &mut value; // An explicitly written mutable reference type provides exclusive access for this borrow.
    *unique += 1; // A safe mutable-reference dereference changes the live local value.
    let raw_mut: *mut u32 = &raw mut value; // A mutable raw pointer type and raw borrow form a pointer to the live local without dereferencing it.
    let raw_const: *const u32 = &raw const value; // A const raw pointer type permits observing the address; this example does not dereference it.
    assert_eq!(raw_mut.cast_const(), raw_const); // Both raw pointers identify the same local allocation; comparing them requires no unsafe access.
    assert_eq!(value, 8); // The earlier exclusive borrow incremented the original value.
} // Close the type showcase function.
