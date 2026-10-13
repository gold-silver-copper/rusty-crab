struct FamilyImpl; // A unit struct serves as the concrete implementation of a type family.
trait Family { // A trait can declare families of associated types rather than only one fixed type.
    type Borrowed<'a> where Self: 'a; // A generic associated type accepts a lifetime and constrains the lifetime of Self.
    type Wrapped<T>; // This associated type has its own type parameter, independent of the trait.
    type Repeated<const N: usize>; // This associated type has its own const parameter.
    const VALUE: u32; // An associated constant without a default must be supplied by implementations.
} // Close the trait declaring the associated type families.
impl Family for FamilyImpl { // Implement the associated families and required constant for a concrete type.
    type Borrowed<'a> = &'a u32 where Self: 'a; // The lifetime-parameterized associated type denotes a borrowed integer.
    type Wrapped<T> = Option<T>; // The type-parameterized associated type wraps its argument in Option.
    type Repeated<const N: usize> = [u8; N]; // The const-parameterized associated type produces an array of the requested length.
    const VALUE: u32 = 6; // A trait implementation supplies the required associated constant.
} // Close the trait implementation.
impl FamilyImpl { // An inherent implementation defines methods independently of a trait.
    async fn value(&self) -> u32 { <Self as Family>::VALUE } // An inherent async method returns a future that will yield the associated constant when polled.
} // Close the inherent implementation.
const extern "C" fn constant_identity(value: u32) -> u32 { value } // A safe const extern function combines const evaluation with an explicit calling convention.
const unsafe extern "C" fn unsafe_constant_identity(value: u32) -> u32 { value } // An unsafe const extern declaration requires an unsafe call; this body has no additional safety preconditions.
const SAFE_VALUE: u32 = constant_identity(8); // Calling the safe const extern function in a constant initializer forces const evaluation.
const UNSAFE_VALUE: u32 = unsafe { unsafe_constant_identity(9) }; // Calling this unsafe const function is sound because its body only returns its integer argument.
pub fn demonstrate() { // Open the associated-item and async syntax demonstrations.
    let value = 7; // Provide a live integer for the borrowed associated type.
    let borrowed: <FamilyImpl as Family>::Borrowed<'_> = &value; // A qualified associated type projection supplies its lifetime argument.
    let wrapped: <FamilyImpl as Family>::Wrapped<u32> = Some(7); // A qualified type projection supplies a type argument.
    let repeated: <FamilyImpl as Family>::Repeated<3> = [1; 3]; // A qualified type projection supplies a const argument to choose the array length.
    let instance = FamilyImpl; // Create a receiver for the inherent async method.
    let _pending_method = instance.value(); // Calling the async method creates a future; this example type-checks it without polling it.
    let asynchronous = async |n: u32| { n + 1 }; // An async closure has async on the closure itself, rather than returning a separate async block.
    let _pending_closure = asynchronous(1); // Calling the async closure creates another future that is deliberately not polled here.
    assert_eq!((*borrowed, wrapped, repeated, FamilyImpl::VALUE), (7, Some(7), [1; 3], 6)); // Verify the values represented by the associated types and constant.
    assert_eq!((SAFE_VALUE, UNSAFE_VALUE), (8, 9)); // Verify the results produced by safe and unsafe const extern function evaluation.
} // Close the associated-item and async syntax demonstrations.
