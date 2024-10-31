use super::backend::{ActiveBackend, RethrowHandle, ThrowByValue};

// Module invariant: thrown exceptions of type `E` are passed to the backend as instance of
// `Exception<E>` with the cause filled, which is immediately read out upon catch.

/// Throw an exception.
///
/// # Safety
///
/// See the safety section of [this crate](crate) for information on matching types.
///
/// In addition, the caller must ensure that the exception can only be caught by Lithium functions
/// and not by the system runtime. The list of banned functions includes
/// [`std::panic::catch_unwind`] and [`std::thread::spawn`], as well as throwing from `main`.
///
/// For this reason, the caller must ensure no frames between [`throw`] and [`catch`] can catch the
/// exception. This includes not passing throwing callbacks to foreign crates, but also not using
/// [`throw`] in own code that might [`intercept`] an exception without cooperation with the
/// throwing side.
///
/// # Example
///
/// ```should_panic
/// use lithium::*;
///
/// unsafe {
///     throw::<&'static str>("Oops!");
/// }
/// ```
#[inline(always)]
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
/// # Safety
///
/// `func` must only throw exceptions of type `E`. See the safety section of [this crate](crate) for
/// more information.
///
/// # Example
///
/// ```rust
/// use lithium::*;
///
/// // SAFETY: the exception type matches
/// let res = unsafe {
///     catch::<(), &'static str>(|| throw::<&'static str>("Oops!"))
/// };
///
/// assert_eq!(res, Err("Oops!"));
/// ```
#[expect(
    clippy::missing_errors_doc,
    reason = "`Err` value is described immediately"
)]
#[inline]
pub unsafe fn catch<R, E>(func: impl FnOnce() -> R) -> Result<R, E> {
    // SAFETY:
    // - `func` only throws `E` by the safety requirement.
    // - `InFlightException` is immediately dropped before returning from `catch`, so no exceptions
    //   may be thrown while it's alive.
    unsafe { intercept(func) }.map_err(|(cause, _)| cause)
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
    /// See the safety section of [this crate](crate) for information on matching types.
    ///
    /// In addition, the caller must ensure that the exception can only be caught by Lithium
    /// functions and not by the system runtime. The list of banned functions includes
    /// [`std::panic::catch_unwind`] and [`std::thread::spawn`].
    ///
    /// For this reason, the caller must ensure no frames between `rethrow` and [`catch`] can catch
    /// the exception. This includes not passing throwing callbacks to foreign crates, but also not
    /// using `rethrow` in own code that might [`intercept`] an exception without cooperation with
    /// the throwing side.
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
/// in [`Err`]. This handle can be used to rethrow the exception, possibly modifying its value or
/// type in the process.
///
/// If you always need to catch the exception, use [`catch`] instead. This function is mostly useful
/// as an analogue of [`Result::map_err`].
///
/// Rust panics are propagated as-is and not caught.
///
/// # Safety
///
/// `func` must only throw exceptions of type `E`. See the safety section of [this crate](crate) for
/// more information.
///
/// **In addition**, certain requirements are imposed on how the returned [`InFlightException`] is
/// used. In particular, no exceptions may be thrown between the moment this function returns
/// an [`InFlightException`] and the moment it is dropped (either by calling [`drop`] or by calling
/// its [`InFlightException::rethrow`] method). Panics, however, are allowed.
///
/// Caught exceptions are not subject to this requirement, i.e. the following pattern is safe:
///
/// ```rust
/// use lithium::*;
///
/// unsafe {
///     let result = intercept::<(), i32>(|| throw::<i32>(1));
///     drop(intercept::<(), i32>(|| throw::<i32>(2)));
///     drop(result);
/// }
/// ```
///
/// # Example
///
/// ```rust
/// use anyhow::{anyhow, Error, Context};
/// use lithium::*;
///
/// /// Throws [`Error`].
/// unsafe fn f() {
///     throw::<Error>(anyhow!("f failed"));
/// }
///
/// /// Throws [`Error`].
/// unsafe fn g() {
///     // SAFETY:
///     // - f only ever throws Error
///     // - no exception is thrown between `intercept` returning and call to `rethrow`
///     match intercept::<_, Error>(|| f()) {
///         Ok(x) => x,
///         Err((e, handle)) => handle.rethrow(e.context("in g")),
///     }
/// }
///
/// // SAFETY: g only ever throws Error
/// println!("{}", unsafe { catch::<_, Error>(|| g()) }.unwrap_err());
/// ```
#[expect(
    clippy::missing_errors_doc,
    reason = "`Err` value is described immediately"
)]
#[inline]
pub unsafe fn intercept<R, E>(func: impl FnOnce() -> R) -> Result<R, (E, InFlightException<E>)> {
    // SAFETY: Requirements forwarded.
    unsafe { ActiveBackend::intercept(func) }
        .map_err(|(cause, handle)| (cause, InFlightException(handle)))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn catch_ok() {
        let result: Result<String, ()> = unsafe { catch(|| String::from("Hello, world!")) };
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[test]
    fn catch_err() {
        let result: Result<(), String> = unsafe { catch(|| throw(String::from("Hello, world!"))) };
        assert_eq!(result.unwrap_err(), "Hello, world!");
    }

    #[cfg(feature = "std")]
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
            let _: Result<(), ()> = unsafe { catch(|| panic!("Hello, world!")) };
        }))
        .unwrap_err();
        assert!(destructor_was_run);

        // Ensure that panic count is reset to 0
        assert!(!std::thread::panicking());
    }

    #[test]
    fn rethrow() {
        let result: Result<(), String> = unsafe {
            catch(|| {
                let (err, in_flight): (String, _) =
                    intercept(|| throw(String::from("Hello, world!"))).unwrap_err();
                in_flight.rethrow(err + " You look nice btw.");
            })
        };
        assert_eq!(result.unwrap_err(), "Hello, world! You look nice btw.");
    }

    #[test]
    fn panic_while_in_flight() {
        struct Dropper;
        impl Drop for Dropper {
            fn drop(&mut self) {
                let _ = std::panic::catch_unwind(|| {
                    let (_err, _in_flight): (String, _) = unsafe {
                        intercept(|| throw(String::from("Literally so insanely suspicious")))
                    }
                    .unwrap_err();
                    panic!("Would be a shame if something happened to the exception.");
                });
            }
        }

        let result: Result<(), String> = unsafe {
            catch(|| {
                let _dropper = Dropper;
                throw(String::from("Hello, world!"));
            })
        };
        assert_eq!(result.unwrap_err(), "Hello, world!");
    }
}
