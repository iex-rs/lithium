use super::{backend::AlignAs, exception::Exception, heterogeneous_stack::unbounded::Stack};
use core::mem::MaybeUninit;

thread_local! {
    static EXCEPTIONS: Stack<AlignAs> = const { Stack::new() };
}

pub fn push<E>(cause: E) -> *mut Exception<E> {
    EXCEPTIONS.with(|stack| std::ptr::from_mut(stack.push().write(Exception::new(cause))))
}

pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    EXCEPTIONS.with(|stack| unsafe {
        stack.pop(ex.cast::<MaybeUninit<Exception<E>>>());
    });
}

pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
    EXCEPTIONS.with(|stack| {
        let ex = unsafe { stack.replace_last(ex.cast::<MaybeUninit<Exception<E>>>()) };
        std::ptr::from_mut(ex.write(Exception::new(cause)))
    })
}

pub fn is_recoverable<E>(ptr: *const Exception<E>) -> bool {
    EXCEPTIONS.with(|stack| stack.is_recoverable(ptr))
}

pub unsafe fn last_local<E>() -> *mut Exception<E> {
    EXCEPTIONS.with(|stack| unsafe { stack.recover_last_mut::<Exception<E>>() }.as_mut_ptr())
}
