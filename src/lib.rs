//! Lightweight exceptions.
//!
//! Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust
//! panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if
//! less applicable.
//!
//! When used together with the [`revolve`](https://lib.rs/revolve) crate, unwinding exceptions
//! takes only 3 ns/frame, barely slower than returns. In this configuration, using exceptions
//! instead of [`Result`] both speeds up the success path and only marginally slows down the error
//! path, making this approach ideal when the probability of an error is low.
//!
//!
//! # Usage
//!
//! Throw an exception with [`throw`](throw()), catch it with [`try`](try()) or the more low-level
//! [`intercept`](intercept()). Unlike with Rust panics, non-[`Send`] and non-`'static` types can be
//! used soundly.
//!
//! For interop, all crates that depend on Lithium need to use the same version:
//!
//! ```toml
//! [dependencies]
//! lithium = "1"
//! ```
//!
//!
//! # Platform support
//!
//! At the moment, the custom mechanism is only supported on nightly on the following platforms:
//!
//! - Unix-like targets (Linux and macOS included)
//! - MinGW (GNU-like Windows)
//! - WASM
//!
//! On stable, when compiled with MSVC on Windows, or on more exotic platforms, exception handling
//! is gracefully degraded to Rust panics.
//!
//!
//! # Safety
//!
//! Exceptions lack dynamic typing information. For soundness, the thrown and caught types must
//! match exactly. Note that the functions are generic, and if the type is inferred wrong, UB will
//! happen. Use turbofish to avoid this pitfall.
//!
//! The matching types requirement only apply to exceptions that aren't caught inside the
//! [`try`](try())/[`intercept`](intercept()) callback. For example, this is sound:
//!
//! ```rust
//! use lithium::*;
//!
//! struct A;
//! struct B;
//!
//! unsafe {
//!     let _ = r#try::<_, A>(|| {
//!         let _ = r#try::<_, B>(|| throw(B));
//!         throw(A);
//!     });
//! }
//! ```
//!
//! The responsibility of holding this safety requirement is split between the throwing and the
//! catching functions. All throwing functions must be `unsafe`, listing "only caught by type `E`"
//! as a safety requirement. All catching functions that take a user-supplied callback must be
//! `unsafe` too, listing "callback only throws type `E`" as a safety requirement.
//!
//! Although seemingly redundant, this enables safe abstractions over exceptions when both the
//! throwing and the catching functions are provided by one crate. As long as the exception types
//! used by the crate match, all safe user-supplied callbacks are sound to call, because safe
//! callbacks can only interact with exceptions in an isolated manner.

#![cfg_attr(backend = "itanium", feature(core_intrinsics))]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(
    clippy::pedantic,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_inline_in_public_items,
    clippy::semicolon_inside_block,
    clippy::arithmetic_side_effects
)]

mod backend;
mod exceptions;
mod heterogeneous_stack;
mod intercept;
mod throw;
mod r#try;

pub use intercept::{intercept, InFlightException};
pub use r#try::r#try;
pub use throw::throw;
