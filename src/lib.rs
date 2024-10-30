//! Lightweight exceptions.
//!
//! Lithium provides a custom exception mechanism as an alternative to Rust panics. Compared to Rust
//! panics, this mechanism is allocation-free, avoids indirections and RTTI, and hence faster, if
//! less applicable.
//!
//! Under the default configuration, Lithium is more than 2x faster Rust panics on common
//! `Result`-like usecases. See the
//! [benchmark](https://github.com/iex-rs/lithium/blob/master/benches/bench.rs).
//!
//!
//! # Usage
//!
//! Throw an exception with [`throw`], catch it with [`catch`] or the more low-level [`intercept`].
//! Unlike with Rust panics, non-[`Send`] and non-`'static` types can be used soundly.
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
//! - Windows (both MinGW and MSVC)
//! - WASM
//!
//! This mechanism works with `#![no_std]`, as long as the Itanium EH unwinder is linked in. Use
//! `default-features = false` feature to enable no-std support.
//!
//! On stable, when compiled with MSVC on Windows, or on more exotic platforms, exception handling
//! is gracefully degraded to Rust panics. This requires `std`.
//!
//!
//! # Safety
//!
//! Exceptions lack dynamic typing information. For soundness, the thrown and caught types must
//! match exactly. Note that the functions are generic, and if the type is inferred wrong, UB will
//! happen. Use turbofish to avoid this pitfall.
//!
//! The matching types requirement only apply to exceptions that aren't caught inside the
//! [`catch`]/[`intercept`] callback. For example, this is sound:
//!
//! ```rust
//! use lithium::*;
//!
//! struct A;
//! struct B;
//!
//! unsafe {
//!     let _ = catch::<_, A>(|| {
//!         let _ = catch::<_, B>(|| throw(B));
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

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(thread_local))]
#![cfg_attr(
    any(backend = "itanium", backend = "seh"),
    expect(
        internal_features,
        reason = "Can't do anything about core::intrinsics::catch_unwind yet",
    )
)]
#![cfg_attr(backend = "itanium", feature(core_intrinsics))]
#![cfg_attr(backend = "seh", feature(core_intrinsics, fn_ptr_trait))]
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
#![allow(
    unstable_name_collisions,
    reason = "XXX: remove when strict provenance is stabilized"
)]

extern crate alloc;

mod api;
mod backend;

#[cfg(any(backend = "itanium", backend = "panic"))]
mod heterogeneous_stack;
#[cfg(any(backend = "itanium", backend = "panic"))]
mod stacked_exceptions;

pub use api::{catch, intercept, throw, InFlightException};
