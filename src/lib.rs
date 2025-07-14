//! Lightweight exceptions.
//!
//! Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust
//! panics, this mechanism is allocation-free, avoids indirections and RTTI, and is hence faster, if
//! less applicable.
//!
//! On nightly, Lithium is more than 2x faster than Rust panics on common `Result`-like usecases.
//! See the [benchmark](https://github.com/iex-rs/lithium/blob/master/benches/bench.rs).
//!
//! Lithium is a low-level library that exposes unsafe functions. Please use it responsibly, only if
//! it improves performance and hidden beneath a safe abstraction.
//!
//!
//! # Usage
//!
//! Throw an exception with [`throw`], catch it with [`catch`] or the more low-level [`intercept`].
//! Unlike with Rust panics, non-[`Send`] and non-`'static` types can be used soundly.
//!
//! ```rust
//! use lithium::{catch, throw};
//!
//! // SAFETY: thrown exception immediately caught by a matching `catch`
//! let err: Result<(), &'static str> = catch(|| unsafe {
//!     throw::<&'static str>("My exception");
//! });
//!
//! assert_eq!(err.unwrap_err(), "My exception");
//! ```
//!
//! Using the `panic = "abort"` strategy breaks Lithium; avoid doing that.
//!
//! For interop, all crates that depend on Lithium need to use the same version:
//!
//! ```toml
//! [dependencies]
//! lithium = "1"
//! ```
//!
//! If you break either of these two requirements, cargo will scream at you.
//!
//!
//! # Platform support
//!
//! On stable Rust, Lithium uses the built-in panic mechanism, tweaking it to increase performance.
//!
//! On nightly Rust, Lithium uses a custom mechanism on the following targets:
//!
//! |Target             |Implementation |Performance                                  |
//! |-------------------|---------------|---------------------------------------------|
//! |Linux, macOS       |Itanium EH ABI |2.5x faster than panics                      |
//! |Windows (MSVC ABI) |SEH            |1.5x faster than panics                      |
//! |Windows (GNU ABI)  |Itanium EH ABI |2.5x faster than panics, but slower than MSVC|
//! |Emscripten (old EH)|C++ exceptions |2x faster than panics                        |
//! |Emscripten (new EH)|Wasm exceptions|2.5x faster than panics, faster than old EH  |
//! |WASI               |Wasm exceptions|3x faster than panics                        |
//!
//! Lithium strives to support all targets that Rust panics support. If Lithium does not work
//! correctly on such a target, please [open an issue](https://github.com/iex-rs/lithium/issues/).
//!
//! On nightly exclusively, Lithium can work without `std` on certain platforms that expose native
//! thread locals and link in an Itanium-style unwinder, such as `x86_64-unknown-linux-gnu`. Such
//! situations are best handled on a case-by-case basis:
//! [open an issue](https://github.com/iex-rs/lithium/issues/) if you would like to see support for
//! a certain `std`-less target.
//!
//!
//! # Safety
//!
//! Throwing Lithium exceptions is dangerous in the sense that unwinding through certain safe code
//! can cause undefined behavior. Therefore, functions that transitively invoke [`throw`] need to be
//! `unsafe`. The safety restrictions are discharged when the exception is caught, so APIs that
//! utilize exceptions in an isolated manner remain safe. We call functions that [`throw`], but
//! don't [`catch`] all thrown exceptions "throwing".
//!
//! This section specifies the safety requirements that must be satisfied when calling throwing
//! functions. As a short-hand to copying these requirements to the doc comments of each throwing
//! function, write "this function throws a Lithium exception of type `E`" in the safety section.
//!
//! The safety requirements are:
//!
//! 1. The caller must ensure that the exception is caught by the correct type. Lithium exceptions
//!    lack dynamic typing information, and using different types at throw and catch sites may lead
//!    to anything from an implicit `transmute` to instant UB. Consider using turbofish to avoid
//!    erroneous type inference.
//! 2. Control flow must not unwind through [`std::panic::catch_unwind`], [`std::thread::spawn`],
//!    drop glue, or any other non-Lithium function that may interact with unwinding. As these
//!    functions are safe, you should likely refrain from throwing within callbacks passed to other
//!    crates.
//! 3. Control flow must not unwind through a frame that holds an unresolved [`InFlightException`].
//!    This is only relevant when using [`intercept`]; see its docs for more information.
//!
//! These requirements can either be discharged by wrapping the throwing function in a valid
//! [`catch`] or [`intercept`] invocation, or forwarded by marking the caller as `unsafe` and
//! documenting it as throwing.
//!
//!
//! # Example
//!
//! Multiple throwing functions, with obligations discharged at crate boundary:
//!
//! ```rust
//! use lithium::{catch, throw};
//!
//! enum MyCrateError {
//!     Foo,
//!     Bar(u32),
//! }
//!
//! /// Do a foo.
//! ///
//! /// # Safety
//! ///
//! /// Throws Lithium exception `MyCrateError`.
//! unsafe fn foo(x: u32) -> u32 {
//!     if x == 0 {
//!         // SAFETY: `foo` is documented as throwing
//!         unsafe {
//!             throw(MyCrateError::Foo);
//!         }
//!     }
//!     x - 1
//! }
//!
//! /// Do a bar.
//! ///
//! /// # Safety
//! ///
//! /// Throws Lithium exception `MyCrateError`.
//! unsafe fn bar(x: u32) {
//!     if x % 100 == 0 {
//!         // SAFETY: `bar` is documented as throwing
//!         unsafe {
//!             throw(MyCrateError::Bar(x % 100));
//!         }
//!     }
//! }
//!
//! pub fn foo_bar(x: u32) -> Result<(), MyCrateError> {
//!     catch(|| {
//!         // SAFETY: exception is caught via a correctly-typed `catch`
//!         let tmp = unsafe { foo(x) };
//!         // SAFETY: exception is caught via a correctly-typed `catch`
//!         unsafe {
//!             bar(tmp);
//!         }
//!     })
//! }
//! ```
//!
//! Isolated exceptions:
//!
//! ```rust
//! use lithium::{catch, throw};
//!
//! struct A;
//! struct B;
//!
//! let _ = catch::<(), A>(|| {
//!     // SAFETY: immediately caught as `B` (most nested `catch` counts)
//!     let _ = catch::<(), B>(|| unsafe { throw(B) });
//!     // SAFETY: immediately caught as `A`
//!     unsafe {
//!         throw(A);
//!     }
//! });
//! ```

#![no_std]
#![cfg_attr(thread_local = "attribute", feature(thread_local))]
#![cfg_attr(
    any(
        backend = "itanium",
        backend = "seh",
        backend = "emscripten",
        backend = "wasm"
    ),
    expect(
        internal_features,
        reason = "Can't do anything about core::intrinsics::catch_unwind yet",
    )
)]
#![cfg_attr(
    any(
        backend = "itanium",
        backend = "seh",
        backend = "emscripten",
        backend = "wasm"
    ),
    feature(core_intrinsics, rustc_attrs)
)]
#![cfg_attr(backend = "seh", feature(fn_ptr_trait, std_internals))]
#![cfg_attr(
    any(backend = "wasm", all(backend = "itanium", target_arch = "wasm32")),
    feature(wasm_exception_handling_intrinsics)
)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(
    clippy::cargo,
    clippy::pedantic,
    clippy::alloc_instead_of_core,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_underscore,
    clippy::assertions_on_result_states,
    clippy::clone_on_ref_ptr,
    clippy::decimal_literal_representation,
    clippy::default_numeric_fallback,
    clippy::deref_by_slicing,
    clippy::else_if_without_else,
    clippy::empty_drop,
    clippy::empty_enum_variants_with_brackets,
    clippy::empty_structs_with_brackets,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::fn_to_numeric_cast_any,
    clippy::format_push_string,
    clippy::infinite_loop,
    clippy::inline_asm_x86_att_syntax,
    clippy::mem_forget, // use ManuallyDrop instead
    clippy::missing_assert_message,
    clippy::missing_const_for_fn,
    clippy::missing_inline_in_public_items,
    clippy::mixed_read_write_in_expression,
    clippy::multiple_unsafe_ops_per_block,
    clippy::mutex_atomic,
    clippy::needless_raw_strings,
    clippy::pub_without_shorthand,
    clippy::rc_buffer,
    clippy::rc_mutex,
    clippy::redundant_type_annotations,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_name_method,
    clippy::self_named_module_files,
    clippy::semicolon_inside_block,
    clippy::separated_literal_suffix,
    clippy::shadow_unrelated,
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    clippy::string_lit_chars_any,
    clippy::string_to_string,
    clippy::tests_outside_test_module,
    clippy::try_err,
    clippy::undocumented_unsafe_blocks,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_safety_doc,
    clippy::unnecessary_self_imports,
    clippy::unneeded_field_pattern,
    clippy::unused_result_ok,
    clippy::wildcard_enum_match_arm,
)]
#![allow(
    clippy::inline_always,
    reason = "I'm not an idiot, this is a result of benchmarking/profiling"
)]

#[cfg(panic = "abort")]
compile_error!("Using Lithium with panic = \"abort\" is unsupported");

#[cfg(any(abort = "std", backend = "panic", thread_local = "std", test))]
extern crate std;

extern crate alloc;

mod api;
mod backend;

#[cfg(any(
    backend = "itanium",
    backend = "emscripten",
    backend = "wasm",
    backend = "panic"
))]
mod heterogeneous_stack;
#[cfg(any(
    backend = "itanium",
    backend = "emscripten",
    backend = "wasm",
    backend = "panic"
))]
mod stacked_exceptions;

#[cfg(any(
    backend = "itanium",
    backend = "seh",
    backend = "emscripten",
    backend = "wasm"
))]
mod intrinsic;

pub use api::{InFlightException, catch, intercept, throw};

/// Abort the process with a message.
///
/// If `std` is available, this also outputs a message to stderr before aborting.
#[cold]
#[inline(never)]
fn abort(message: &str) -> ! {
    #[cfg(abort = "std")]
    {
        use std::io::Write;
        let _ = std::io::stderr().write_all(message.as_bytes());
        std::process::abort();
    }

    // This is a nightly-only method, but build.rs sets `abort = "std"` for stable backends.
    #[cfg(abort = "core")]
    {
        let _ = message;
        core::intrinsics::abort();
    }
}
