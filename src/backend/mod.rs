//! Unwinding backends.
//!
//! Unwinding is a mechanism of forcefully "returning" through multiple call frames, called
//! *throwing*, up until a special call frame, called *interceptor*. This roughly corresponds to the
//! `resume_unwind`/`catch_unwind` pair on Rust and `throw`/`catch` pair on C++.
//!
//! It's crucial that unwinding doesn't require (source-level) cooperation from the intermediate
//! call frames.
//!
//! Two kinds of backends are supported: those that throw by value and those that throw by pointer.
//! Throwing by value is for backends that can keep arbitrary data retained on stack during
//! unwinding (i.e. unwinding and calling landing pads does not override the throwing stackframe),
//! while throwing by pointer is for backends that need the exception, along with any additional
//! data, to be stored on heap.
//!
//! # Safety
//!
//! Backends must ensure that when an exception is thrown, unwinding proceeds to the closest (most
//! nested) `intercept` frame and that `intercept` returns this exact exception.
//!
//! Note that several exceptions can co-exist at once, even in a single thread. This can happen if
//! a destructor that uses exceptions (without letting them escape past `drop`) is invoked during
//! unwinding from another exception. This can be nested arbitrarily. In this context, the order of
//! catching must be in the reverse order of throwing.
//!
//! During unwinding, all destructors of locals must be run, as if `return` was called. Exceptions
//! may not be ignored or caught twice.

/// Throw-by-pointer backend.
///
/// Implementors of this trait should consider exceptions as type-erased objects. These objects
/// contain a header, provided by the implementor, and the `throw` and `intercept` method work only
/// with this header. The header is part of a greater allocation containing the exception object,
/// but interacting with this object is forbidden.
///
/// The implementation may use the header for any purpose during unwinding. `throw` may assume that
/// the header is either a pristine header returned by `new_header`, or a "used" header returned by
/// `intercept` that was originally created by another exception. The backend must be able to reuse
/// such headers correctly, reinitializing them within `throw` if necessary.
///
/// # Safety
///
/// Implementations must satisfy the rules of the "Safety" section of [this module](self). In
/// addition:
///
/// The implementation may modify the header arbitrarily during unwinding, but modifying any other
/// data from the same allocation is forbidden.
///
/// If the `intercept` method returns `Err`, the returned pointer must be the same as the pointer
/// passed to `throw`, including provenance.
///
/// The user of this trait is allowed to reuse the header when rethrowing exceptions. In particular,
/// the return value of `intercept` may be used as an argument to `throw`.
#[allow(dead_code, reason = "This is only used by some of the backends")]
pub unsafe trait ThrowByPointer {
    /// An exception header.
    ///
    /// Allocated exceptions, as stored in the [`Exception`](super::stacked_exceptions::Exception)
    /// type, will contain this header. This allows exception pointers to be used with ABIs that
    /// require exceptions to contain custom information, like Itanium EH ABI.
    type ExceptionHeader;

    /// Create a new exception header.
    ///
    /// This will be called whenever a new exception needs to be allocated.
    fn new_header() -> Self::ExceptionHeader;

    /// Throw an exception.
    ///
    /// # Safety
    ///
    /// The first requirement is that `ex` is a unique pointer to an exception header.
    ///
    /// Secondly, it is important that intermediate call frames don't preclude unwinding from
    /// happening soundly. For example, [`catch_unwind`](std::panic::catch_unwind) can safely catch
    /// panics and may start catching foreign exceptions soon, both of which can confuse the user of
    /// this trait.
    ///
    /// For this reason, the caller must ensure no intermediate frames can affect unwinding. This
    /// includes not passing throwing callbacks to foreign crates, but also not using `throw` in own
    /// code that might `intercept` an exception without cooperation with the throwing side.
    unsafe fn throw(ex: *mut Self::ExceptionHeader) -> !;

    /// Catch an exception.
    ///
    /// This function returns `Ok` if the function returns normally, or `Err` if it throws (and the
    /// thrown exception is not caught by a nested interceptor).
    #[allow(
        clippy::missing_errors_doc,
        reason = "`Err` value is described immediately"
    )]
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Self::ExceptionHeader>;
}

/// Throw-by-value backend.
///
/// Implementors of this trait should consider exceptions as generic objects. Any additional
/// information used by the implementor has to be stored separately.
///
/// # Safety
///
/// Implementations must satisfy the rules of the "Safety" section of [this module](self). In
/// addition:
///
/// The implementation may modify the header arbitrarily during unwinding, but modifying the
/// exception object is forbidden.
///
/// If the `intercept` method returns `Err`, the returned value must be the same as the value passed
/// to `throw`.
pub unsafe trait ThrowByValue {
    /// A [`RethrowHandle`].
    type RethrowHandle<E>: RethrowHandle;

    /// Throw an exception.
    ///
    /// # Safety
    ///
    /// It is important that intermediate call frames don't preclude unwinding from happening
    /// soundly. For example, [`catch_unwind`](std::panic::catch_unwind) can safely catch panics and
    /// may start catching foreign exceptions soon, both of which can confuse the user of this
    /// trait.
    ///
    /// For this reason, the caller must ensure no intermediate frames can affect unwinding. This
    /// includes not passing throwing callbacks to foreign crates, but also not using `throw` in own
    /// code that might `intercept` an exception without cooperation with the throwing side.
    /// Notably, this also requires that exceptions aren't thrown across a frame that holds a live
    /// [`RethrowHandle`].
    unsafe fn throw<E>(cause: E) -> !;

    /// Catch an exception.
    ///
    /// This function returns `Ok` if the function returns normally, or `Err` if it throws (and the
    /// thrown exception is not caught by a nested interceptor).
    #[allow(
        clippy::missing_errors_doc,
        reason = "`Err` value is described immediately"
    )]
    fn intercept<Func: FnOnce() -> R, R, E>(func: Func) -> Result<R, (E, Self::RethrowHandle<E>)>;
}

/// A rethrow handle.
///
/// This handle is returned by [`ThrowByValue::intercept`] implementations that support efficient
/// rethrowing. Sometimes, certain allocations or structures can be retained between throw calls,
/// and this handle can be used to optimize this.
///
/// The handle owns the structures/allocations, and when it's dropped, it should free those
/// resources, if necessary.
pub trait RethrowHandle {
    /// Throw a new exception by reusing the existing context.
    ///
    /// See [`ThrowByValue::intercept`] docs for examples and safety notes.
    ///
    /// # Safety
    ///
    /// All safety requirements of [`ThrowByValue::throw`] apply.
    unsafe fn rethrow<F>(self, new_cause: F) -> !;
}

#[cfg(backend = "itanium")]
#[path = "itanium.rs"]
mod imp;

#[cfg(backend = "seh")]
#[path = "seh.rs"]
mod imp;

#[cfg(backend = "panic")]
#[path = "panic.rs"]
mod imp;

#[cfg(backend = "emscripten")]
#[path = "emscripten.rs"]
mod imp;

#[cfg(backend = "wasm")]
#[path = "wasm.rs"]
mod imp;

pub(crate) use imp::ActiveBackend;

#[cfg(test)]
mod test {
    use super::{ActiveBackend, RethrowHandle, ThrowByValue};
    use alloc::string::String;

    #[test]
    fn intercept_ok() {
        let result = ActiveBackend::intercept::<_, _, ()>(|| String::from("Hello, world!"));
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[test]
    fn intercept_err() {
        let result = ActiveBackend::intercept::<_, (), String>(|| unsafe {
            ActiveBackend::throw(String::from("Hello, world!"));
        });
        let (caught_ex, _) = result.unwrap_err();
        assert_eq!(caught_ex, "Hello, world!");
    }

    #[test]
    fn intercept_panic() {
        let result = std::panic::catch_unwind(|| {
            ActiveBackend::intercept::<_, _, ()>(|| {
                std::panic::resume_unwind(alloc::boxed::Box::new("Hello, world!"))
            })
        });
        assert_eq!(
            *result.unwrap_err().downcast_ref::<&'static str>().unwrap(),
            "Hello, world!",
        );
    }

    #[test]
    fn nested_intercept() {
        let result = ActiveBackend::intercept::<_, _, ()>(|| {
            ActiveBackend::intercept::<_, _, String>(|| unsafe {
                ActiveBackend::throw(String::from("Hello, world!"));
            })
        });
        let (caught_ex, _) = result.unwrap().unwrap_err();
        assert_eq!(caught_ex, "Hello, world!");
    }

    #[test]
    fn rethrow() {
        let result = ActiveBackend::intercept::<_, (), String>(|| {
            let result = ActiveBackend::intercept::<_, _, String>(|| unsafe {
                ActiveBackend::throw(String::from("Hello, world!"));
            });
            let (ex2, handle) = result.unwrap_err();
            assert_eq!(ex2, "Hello, world!");
            unsafe {
                handle.rethrow(ex2);
            }
        });
        let (caught_ex, _) = result.unwrap_err();
        assert_eq!(caught_ex, "Hello, world!");
    }

    #[test]
    fn destructors_are_run() {
        struct Dropper<'a>(&'a mut bool);
        impl Drop for Dropper<'_> {
            fn drop(&mut self) {
                *self.0 = true;
            }
        }

        let mut destructor_was_run = false;
        let result = ActiveBackend::intercept::<_, (), String>(|| {
            let _dropper = Dropper(&mut destructor_was_run);
            unsafe {
                ActiveBackend::throw(String::from("Hello, world!"));
            }
        });
        let (caught_ex, _) = result.unwrap_err();
        assert_eq!(caught_ex, "Hello, world!");

        assert!(destructor_was_run);
    }

    #[test]
    fn nested_with_drop() {
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) {
                let result = ActiveBackend::intercept::<_, (), String>(|| unsafe {
                    ActiveBackend::throw(String::from("Awful idea"));
                });
                let (caught_ex2, _) = result.unwrap_err();
                assert_eq!(caught_ex2, "Awful idea");
            }
        }

        let result = ActiveBackend::intercept::<_, (), String>(|| {
            let _dropper = Dropper;
            unsafe {
                ActiveBackend::throw(String::from("Hello, world!"));
            }
        });
        let (caught_ex1, _) = result.unwrap_err();
        assert_eq!(caught_ex1, "Hello, world!");
    }
}
