#![no_std] // Use core instead of automatically linking std; this crate does not allocate or use operating-system services.
#![no_main] // Omit the ordinary Rust main entry point; this profile produces an object for a separately supplied host.
#![no_implicit_prelude] // Require explicit library names and imports instead of bringing library preludes into scope.
extern crate core; // Explicitly name the core library so its types are available without an extern prelude.
#[panic_handler] // Supply the panic handler required by this no_std binary crate.
fn panic(_information: &core::panic::PanicInfo<'_>) -> ! { // The panic handler receives diagnostic information and must never return.
    loop { core::hint::spin_loop(); } // A panic would spin indefinitely; this compile-only showcase never invokes the handler.
} // Close the divergent panic handler.
#[unsafe(no_mangle)] // Safety: this object defines the project-owned rusty_crab_entry symbol once; an eventual host must avoid defining it again.
pub extern "C" fn rusty_crab_entry(value: u32) -> u32 { // Export a C-ABI function for a future host; it is not an operating-system entry point and is not executed by this profile.
    if value <= 9 { // Guard the small input domain using a primitive comparison without library-prelude imports.
        value + 1 // The guarded input is at most nine, so this increment cannot overflow.
    } else { // Handle inputs outside the guarded decimal-digit range.
        0 // Return a fixed sentinel for values outside the small example domain.
    } // Close the bounded conditional expression.
} // Close the exported object function.
