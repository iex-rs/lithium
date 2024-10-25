use super::{backend, stack_allocator};

/// Throw an exception.
///
/// If uncaught, exceptions eventually terminate the process or the thread.
///
/// # Safety
///
/// See the safety section of [this module](super) for information on matching types. In addition,
/// the caller must ensure the exception is not caught with [`std::panic::catch_unwind`].
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
    let (is_local, ex) = stack_allocator::push(cause);
    backend::throw(is_local, ex);
}
