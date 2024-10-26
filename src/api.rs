use super::{
    backend::{ActiveBackend, Backend},
    exceptions::{pop, push, replace_last, Exception},
};
use core::mem::ManuallyDrop;

// Module invariant: thrown exceptions of type `E` are passed to the backend as instance of
// `Exception<E>` with the cause filled, which is immediately read out upon catch.

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this crate](crate) for information on matching types.
///
/// In addition, the caller must ensure that the exception can only be caught by Lithium functions
/// and not by the system runtime. The list of banned functions includes
/// [`std::panic::catch_unwind`] and [`std::thread::spawn`].
///
/// For this reason, the caller must ensure no frames between [`throw`] and [`try`](try()) can catch
/// the exception. This includes not passing throwing callbacks to foreign crates, but also not
/// using [`throw`] in own code that might [`intercept`] an exception without cooperation with the
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
#[inline]
pub unsafe fn throw<E>(cause: E) -> ! {
    let ex = push(cause).cast();
    // SAFETY:
    // - The exception is a unique pointer to an exception object, as allocated by `push`.
    // - "Don't mess with exceptions" is required transitively.
    // This satisfies the module invariant.
    unsafe {
        ActiveBackend::throw(ex);
    }
}

/// Catch an exception.
///
/// If `func` returns a value, this function wraps it in [`Ok`].
///
/// If `func` throws an exception, this function returns it, wrapped it in [`Err`].
///
/// If you need to rethrow the exception, possibly modifying it in the process, consider using the
/// more efficient [`intercept`] function instead of pairing [`try`](try()) with [`throw`].
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
///     r#try::<(), &'static str>(|| throw::<&'static str>("Oops!"))
/// };
///
/// assert_eq!(res, Err("Oops!"));
/// ```
#[allow(clippy::missing_errors_doc)]
#[inline]
pub unsafe fn r#try<R, E>(func: impl FnOnce() -> R) -> Result<R, E> {
    // SAFETY:
    // - `func` only throws `E` by the safety requirement.
    // - `InFlightException` is immediately dropped before returning from `try`, so no exceptions
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
// Type invariant: `ex` is a unique pointer to the exception object on the exception stack.
pub struct InFlightException<E> {
    ex: *mut Exception<E>,
}

impl<E> Drop for InFlightException<E> {
    /// Drop the exception, stopping Lithium unwinding.
    #[inline]
    fn drop(&mut self) {
        // SAFETY:
        // - `ex` is a unique pointer to the exception object by the type invariant.
        // - The safety requirement on `intercept` requires that all exceptions that are thrown
        //   between `intercept` and `drop` are balanced. This exception was at the top of the stack
        //   when `intercept` returned, so it must still be at the top when `drop` is invoked.
        unsafe { pop(self.ex) }
    }
}

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
    /// For this reason, the caller must ensure no frames between `rethrow` and [`try`](try()) can
    /// catch the exception. This includes not passing throwing callbacks to foreign crates, but
    /// also not using `rethrow` in own code that might [`intercept`] an exception without
    /// cooperation with the throwing side.
    #[inline]
    pub unsafe fn rethrow<F>(self, new_cause: F) -> ! {
        let ex = ManuallyDrop::new(self);
        // SAFETY: The same logic that proves `pop` in `drop` is valid applies here. We're not
        // *really* dropping `self`, but the user code does not know that.
        let ex = unsafe { replace_last(ex.ex, new_cause) }.cast();
        // SAFETY:
        // - `ex` is a unique pointer to the exception object because it was just produced by
        //   `replace_last`.
        // - "Don't mess with exceptions" is required transitively.
        // This satisfies the module invariant.
        unsafe {
            ActiveBackend::throw(ex);
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
/// If you always need to catch the exception, use [`try`](try()) instead. This function is mostly
/// useful as an analogue of [`Result::map_err`].
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
/// its [`InFlightException::rethrow`] method).
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
/// println!("{}", unsafe { r#try::<_, Error>(|| g()) }.unwrap_err());
/// ```
#[allow(clippy::missing_errors_doc)]
#[inline]
pub unsafe fn intercept<R, E>(func: impl FnOnce() -> R) -> Result<R, (E, InFlightException<E>)> {
    ActiveBackend::intercept(func).map_err(|ex| {
        let ex: *mut Exception<E> = ex.cast();
        // SAFETY: By the safety requirement, unwinding could only happen from `throw` with type
        // `E`. Backend guarantees the pointer is passed as-is, and `throw` only throws unique
        // pointers to valid instances of `Exception<E>` via the backend.
        let ex_ref = unsafe { &mut *ex };
        // SAFETY: We only read the cause here once.
        let cause = unsafe { ex_ref.cause() };
        (cause, InFlightException { ex })
    })
}
