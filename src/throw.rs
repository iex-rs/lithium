use super::{
    backend::{ActiveBackend, Backend},
    exceptions::push,
};

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this module](super) for information on matching types.
///
/// In addition, the caller must ensure that the exception can only be caught by Lithium functions
/// and not by the system runtime. The list of banned functions includes
/// [`std::panic::catch_unwind`] and [`std::thread::spawn`].
///
/// For this reason, the caller must ensure no frames between [`throw`] and
/// [`catch`](super::catch()) can catch the exception. This includes not passing throwing callbacks
/// to foreign crates, but also not using [`throw`] in own code that might
/// [`intercept`](super::intercept()) an exception without cooperation with the throwing side.
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
    unsafe {
        ActiveBackend::throw(ex);
    }
}
