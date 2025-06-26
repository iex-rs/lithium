use super::backend::{ActiveBackend, RethrowHandle, ThrowByValue};

/// Throw an exception.
///
/// # Safety
///
/// The caller must ensure that the exception can only be caught by Lithium functions with
/// a matching error type, and that execution doesn't unwind across an in-flight exception. See the
/// safety section of [this crate](crate) for more information.
///
/// # Example
///
/// ```rust
/// use lithium::throw;
///
/// /// Throws Lithium exception of type `&'static str`.
/// unsafe fn throwing() {
///     throw::<&'static str>("Oops!");
/// }
/// ```
#[inline(never)]
pub unsafe fn throw<E>(cause: E) -> ! {
    // SAFETY: Required transitively.
    unsafe {
        ActiveBackend::throw(cause);
    }
}

/// Catch an exception.
///
/// If `func` returns a value, this function wraps it in [`Ok`].
///
/// If `func` throws an exception, this function returns it, wrapped it in [`Err`].
///
/// If you need to rethrow the exception, possibly modifying it in the process, consider using the
/// more efficient [`intercept`] function instead of pairing [`catch`] with [`throw`].
///
/// Rust panics are propagated as-is and not caught.
///
/// Note that while this is a safe function, choosing the right error type is crucial to correctly
/// invoking `unsafe` throwing functions inside the `func` callback.
///
/// # Example
///
/// ```rust
/// use lithium::{catch, throw};
///
/// let res = catch::<(), &'static str>(|| {
///     // SAFETY: caught by the matching `catch` above
///     unsafe {
///         throw::<&'static str>("Oops!");
///     }
/// });
///
/// assert_eq!(res, Err("Oops!"));
/// ```
#[expect(
    clippy::missing_errors_doc,
    reason = "`Err` value is described immediately"
)]
#[inline]
pub fn catch<R, E>(func: impl FnOnce() -> R) -> Result<R, E> {
    // `InFlightException` is immediately dropped before returning from `catch`, so no exceptions
    // may be thrown while it's alive.
    intercept(func).map_err(|(cause, _)| cause)
}

/// Not-quite-caught exception.
///
/// This type is returned by [`intercept`] when an exception is caught. Exception handling is not
/// yet done at that point: it's akin to entering a `catch` clause in C++.
///
/// At this point, you can either drop the handle, which halts the Lithium machinery and brings you
/// back to the sane land of [`Result`], or call [`InFlightException::rethrow`] to piggy-back on the
/// contexts of the caught exception.
pub struct InFlightException<E>(<ActiveBackend as ThrowByValue>::RethrowHandle<E>);

impl<E> InFlightException<E> {
    /// Throw a new exception by reusing the existing context.
    ///
    /// See [`intercept`] docs for examples and safety notes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the exception can only be caught by Lithium functions with
    /// a matching error type, and that execution doesn't unwind across an in-flight exception. See
    /// the safety section of [this crate](crate) for more information.
    #[inline]
    pub unsafe fn rethrow<F>(self, new_cause: F) -> ! {
        // SAFETY: Requirements forwarded.
        unsafe {
            self.0.rethrow(new_cause);
        }
    }
}

/// Begin exception catching.
///
/// If `func` returns a value, this function wraps it in [`Ok`].
///
/// If `func` throws an exception, the error cause along with a handle to the exception is returned
/// in [`Err`]. This handle can be used to rethrow the exception via [`InFlightException::rethrow`],
/// possibly modifying its value or type in the process. This is more efficient than [`catch`]ing
/// and throwing a new exception because it can reuse existing unwinding contexts.
///
/// If you always need to catch the exception, use [`catch`] instead. `intercept` is mostly useful
/// as an optimal analogue of [`Result::map_err`].
///
/// Rust panics are propagated as-is and not caught.
///
/// Note that while this is a safe function, choosing the right error type and not misusing the
/// returned handle is crucial to correctly invoking `unsafe` throwing functions inside the `func`
/// callback.
///
/// # In-flight exceptions
///
/// If `func` throws, `intercept` catches the exception without finalizing it. Finalization is
/// delayed until the handle is either dropped (e.g. via `drop` or by going out of scope) or
/// rethrown (via [`InFlightException::rethrow`]).
///
/// The range of allowed operations is reduced during the "critical zone" while an exception is in
/// flight. The rules basically amount to "in-flight exceptions must be correctly nested":
///
/// 1. Every exception thrown while another exception is in flight must be caught within the same
///    range. In other words, control flow must not unwind across a stack frame holding an in-flight
///    exception. Rust panics are exempt from this requirement.
///
/// 2. Every exception intercepted while another exception is in flight must be finalized before the
///    first exception. In other words, if `intercept` is called twice, the returned handles must be
///    dropped in the inverse order of creation. (Coupled with the first rule, this implies that
///    rethrowing the inner exception via a handle is always UB.)
///
/// Note that leaking the exception handle with [`core::mem::forget`] is not a "get out of jail
/// free" card, it just extends the critical zone until the end of the program.
///
/// Here are two counterexamples with undefined behavior:
///
/// ```no_run
/// use lithium::{intercept, throw};
///
/// // This creates an in-flight exception
/// let result = intercept::<(), i32>(|| {
///     // SAFETY: immediately caught by correctly typed `intercept`
///     unsafe {
///         throw::<i32>(1);
///     }
/// });
///
/// // This throws while an exception is in flight. This is UB!
/// // If this statement was wrapped in `catch`, it would've been fine.
/// // SAFETY: none
/// unsafe {
///     throw::<i32>(2);
/// }
///
/// // Finalize the exception--all too late
/// drop(result);
///
/// // Diagram:
/// //     |-----------|     in-flight exception 1
/// //           |---------| exception 2 attempts to unwind across the function holding `result`
/// ```
///
/// ```no_run
/// use lithium::{intercept, throw};
///
/// // This creates an in-flight exception 1
/// let result1 = intercept::<(), i32>(|| {
///     // SAFETY: wait for it
///     unsafe {
///         throw::<i32>(1);
///     }
/// });
///
/// // This creates an in-flight exception 2
/// let result2 = intercept::<(), i32>(|| {
///     // SAFETY: wait for it
///     unsafe {
///         throw::<i32>(2);
///     }
/// });
///
/// // The exceptions are stacked in order 1, 2, and so must be discarded in the opposite order to
/// // be nested correctly. But they aren't here:
/// drop(result1); // This causes UB
/// drop(result2);
/// // Had the `drop` calls been swapped, the code would have been valid.
///
/// // Diagram:
/// //     |-----------|     in-flight exception 1
/// //           |---------| in-flight exception 2 finalized after 1
/// ```
///
/// # Example
///
/// ```rust
/// use anyhow::{anyhow, Error, Context};
/// use lithium::{catch, intercept, throw};
///
/// /// Throws [`Error`].
/// unsafe fn f() {
///     throw::<Error>(anyhow!("f failed"));
/// }
///
/// /// Throws [`Error`].
/// unsafe fn g() {
///     // SAFETY:
///     // - error type matches
///     // - we don't touch Lithium between `intercept` and `rethrow`
///     match intercept::<(), Error>(|| unsafe { f() }) {
///         Ok(()) => {},
///         // SAFETY: `g` is documented as throwing [`Error`]
///         Err((e, handle)) => unsafe { handle.rethrow(e.context("in g")) },
///     }
/// }
///
/// // SAFETY: caught by a valid `catch`
/// println!("{}", catch::<_, Error>(|| unsafe { g() }).unwrap_err());
/// ```
#[expect(
    clippy::missing_errors_doc,
    reason = "`Err` value is described immediately"
)]
#[inline(always)]
pub fn intercept<R, E>(func: impl FnOnce() -> R) -> Result<R, (E, InFlightException<E>)> {
    ActiveBackend::intercept(func).map_err(|(cause, handle)| (cause, InFlightException(handle)))
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::string::String;

    #[test]
    fn catch_ok() {
        let result: Result<String, ()> = catch(|| String::from("Hello, world!"));
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[test]
    fn catch_err() {
        let result: Result<(), String> = catch(|| unsafe { throw(String::from("Hello, world!")) });
        assert_eq!(result.unwrap_err(), "Hello, world!");
    }

    #[test]
    fn catch_panic() {
        struct Dropper<'a>(&'a mut bool);
        impl Drop for Dropper<'_> {
            fn drop(&mut self) {
                *self.0 = true;
            }
        }

        let mut destructor_was_run = false;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _dropper = Dropper(&mut destructor_was_run);
            let _: Result<(), ()> = catch(|| panic!("Hello, world!"));
        }))
        .unwrap_err();
        assert!(destructor_was_run);

        // Ensure that panic count is reset to 0
        assert!(!std::thread::panicking());
    }

    #[test]
    fn rethrow() {
        let result: Result<(), String> = catch(|| {
            let (err, in_flight): (String, _) =
                intercept(|| unsafe { throw(String::from("Hello, world!")) }).unwrap_err();
            unsafe {
                in_flight.rethrow(err + " You look nice btw.");
            }
        });
        assert_eq!(result.unwrap_err(), "Hello, world! You look nice btw.");
    }

    #[test]
    fn panic_while_in_flight() {
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) {
                let _ = std::panic::catch_unwind(|| {
                    let (_err, _in_flight): (String, _) = intercept(|| unsafe {
                        throw(String::from("Literally so insanely suspicious"))
                    })
                    .unwrap_err();
                    panic!("Would be a shame if something happened to the exception.");
                });
            }
        }

        let result: Result<(), String> = catch(|| {
            let _dropper = Dropper;
            unsafe {
                throw(String::from("Hello, world!"));
            }
        });
        assert_eq!(result.unwrap_err(), "Hello, world!");
    }
}
