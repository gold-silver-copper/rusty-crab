#[allow(missing_abi)] // Permit the deprecated implicit-ABI spelling so its still-accepted syntax can be shown.
extern fn implicit_abi(value: i32) -> i32 { value } // An extern function without an ABI string uses the default foreign calling convention; this spelling is deprecated.
extern crate core as rust_core; // An extern-crate item can rename a dependency.
use std::{fmt::Debug, marker::PhantomData as Marker}; // A nested use tree groups imports and can rename an imported type.
use self::Color::*; // A glob import brings enum variants into the local module namespace.
const SIZE: usize = 3; // A constant item has a declared type and compile-time initializer.
static NAME: &str = "fox"; // An immutable static has a fixed allocation and a shared string reference.
static mut COUNTER: u32 = 0; // A mutable static exists but this example does not access it unsafely.
#[derive(Clone, Copy, Debug, PartialEq)] // An outer derive attribute invokes compiler-provided derive macros.
pub struct Point { pub x: i32, pub y: i32 } // A named-field struct declares two public fields.
struct Pair(i32, i32); // A tuple struct has positional fields and a constructor.
struct Unit; // A unit struct has no fields.
#[repr(u8)] // A representation attribute specifies the discriminant representation.
enum Color { Red = 1, Green = 2, Blue = 3 } // A fieldless enum may give explicit discriminant values.
enum Shape { None, Tuple(i32), Named { value: i32 } } // Enum variants may have unit, tuple, and named-field forms.
union Bits { unsigned: u32, signed: i32 } // A union overlays its fields in the same storage.
type Coordinates = (i32, i32); // A type alias supplies another name for an existing type.
struct Buffer<T, const N: usize> { data: [T; N] } // A type parameter and const parameter control the element and array length.
struct Borrowed<'a, T: ?Sized> { value: &'a T } // A lifetime parameter and relaxed Sized bound allow borrowing an unsized value.
trait Describe: Debug { // A trait can require a supertrait.
    type Output; // An associated type is chosen by each implementation.
    fn describe(&self) -> Self::Output; // A required trait method takes a shared receiver and uses an associated type.
} // Close the trait declaration.
trait Label { const LABEL: &'static str = "point"; } // An associated constant can have a default; this separate trait is not used as a trait object.
impl Describe for Point { // A trait implementation supplies behavior for a concrete type.
    type Output = i32; // Choose the associated output type for this implementation.
    fn describe(&self) -> i32 { self.x + self.y } // A method body returns its tail expression implicitly.
} // Close the trait implementation.
impl Point { // An inherent implementation adds methods without implementing a trait.
    fn consume(self) -> i32 { self.x } // A by-value receiver takes ownership of the instance.
    fn boxed(self: Box<Self>) -> i32 { self.x } // An explicit self type receives an owned box of the implementing type.
    const ORIGIN: Self = Self { x: 0, y: 0 }; // Self names the implementing type inside an associated constant.
    fn shift(&mut self, dx: i32) { self.x += dx; } // A mutable receiver permits changing a field through the borrow.
} // Close the inherent implementation.
unsafe trait MarkerTrait {} // An unsafe trait declares a contract its implementers must uphold.
unsafe impl MarkerTrait for Point {} // This empty marker contract imposes no additional safety condition.
unsafe extern "C" { // An unsafe extern block declares foreign symbols using the C ABI.
    safe fn getpid() -> i32; // This Unix process-ID query has no caller preconditions and can be declared safe.
    static errno_placeholder: i32; // A foreign static declaration need not be linked if it is never referenced.
    fn printf(format: *const rust_core::ffi::c_char, ... ) -> i32; // A foreign variadic declaration uses an ellipsis after fixed parameters.
} // Close the foreign declaration block; these declarations are not called here.
const fn doubled(value: i32) -> i32 { value * 2 } // A const function can execute during constant evaluation or at runtime.
unsafe fn read_value(pointer: *const i32) -> i32 { // An unsafe function requires its caller to provide a valid aligned pointer.
    unsafe { *pointer } // The caller guarantees a live i32, so dereferencing the pointer is sound.
} // Close the unsafe function.
extern "C" fn c_identity(value: i32) -> i32 { value } // An ordinary Rust-defined function may use the C calling convention.
fn identity<T>(value: T) -> T where T: Copy { value } // A where-clause constrains a generic function's type parameter.
fn borrowing<'a>(value: &'a str) -> &'a str { value } // A named lifetime relates the input and output borrows.
fn raw_lifetime<'r#type>(value: &'r#type str) -> &'r#type str { value } // Raw lifetime names permit a keyword after the lifetime marker.
fn higher_ranked<F: for<'a> Fn(&'a str) -> &'a str>(f: F) { assert_eq!(f("fox"), "fox"); } // A higher-ranked trait bound requires a callable for every input lifetime.
pub(crate) fn restricted() {} // Crate-restricted visibility exposes an item only within this crate.
mod nested { // An inline module introduces another namespace.
    pub(super) fn parent_visible() {} // Parent-restricted visibility exposes the function to its parent module.
    pub(in crate::items) fn path_visible() {} // A visibility restriction may name an ancestor module path.
} // Close the nested module.
fn shorten<'a, 'b>(value: &'a str) -> &'b str where 'a: 'b { value } // A lifetime where-predicate allows shortening a longer-lived borrow.
fn capture<'a, T: 'a>(value: &'a T) -> impl Copy + use<'a, T> { value } // A precise capture bound names the lifetime and type captured by an opaque return type.
fn bounded<I: Iterator<Item: Copy>>(iter: I) -> usize { iter.count() } // An associated type bound constrains the iterator item without naming a concrete type.
#[repr(align(8))] // A nested attribute meta list contains an integer literal argument.
struct Aligned(u8); // The alignment attribute applies to this tuple struct.
#[doc = "A documented identity function."] // A name-value attribute supplies documentation with a string literal.
#[inline] // An empty-argument attribute requests inline consideration by the compiler.
fn documented(value: i32) -> i32 { value } // The function receives both of the preceding attributes.
pub fn demonstrate() { // Run examples using the declared items.
    let mut point = Point { x: 2, y: 3 }; // A struct expression initializes named fields.
    point.shift(1); // Method-call syntax automatically borrows the mutable receiver.
    let other = Point { y: 7, ..point }; // Struct update syntax takes unspecified fields from another value.
    let result = <Point as Describe>::describe(&other); // A qualified path selects a specific trait implementation.
    let object: &dyn Describe<Output = i32> = &other; // A trait object fixes an associated type and permits dynamic dispatch.
    let buffer = Buffer::<u8, { SIZE }> { data: [5; SIZE] }; // A braced const argument distinguishes the value argument from a type path in the parsed syntax.
    let view: Borrowed<'static, str> = Borrowed { value: NAME }; // Explicit lifetime and type arguments instantiate a struct borrowing a static string.
    let bits = Bits { unsigned: 7 }; // A union expression initializes exactly one field.
    let signed = unsafe { bits.signed }; // Both fields are equally sized integers and every i32 bit pattern is valid.
    let pointer = &signed as *const i32; // An as-cast converts a shared reference to a raw pointer.
    let checked = unsafe { read_value(pointer) }; // The pointer still refers to the live, aligned local signed value.
    higher_ranked(borrowing); // A generic function accepts an item satisfying a higher-ranked bound.
    assert_eq!(result, object.describe()); // Static and dynamic trait dispatch produce the same result in this example.
    assert_eq!(identity::<i32>(doubled(checked)), 14); // Explicit generic arguments and const-capable function calls compose.
    assert_eq!((buffer.data[0], view.value, c_identity(1)), (5, "fox", 1)); // Indexing, field access, and a function with a foreign ABI remain usable from Rust.
} // Close the item demonstration.
