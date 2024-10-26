use super::{backend::AlignAs, exception::Exception, heterogeneous_stack::unbounded::Stack};

thread_local! {
    static EXCEPTIONS: Stack<AlignAs> = const { Stack::new() };
}

pub fn push<E>(cause: E) -> *mut Exception<E> {
    EXCEPTIONS.with(|stack| {
        let ex: *mut Exception<E> = stack.push();
        unsafe {
            ex.write(Exception::new(cause));
        }
        ex
    })
}

pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    EXCEPTIONS.with(|stack| unsafe {
        stack.pop(ex);
    });
}

pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
    EXCEPTIONS.with(|stack| {
        let ex: *mut Exception<F> = unsafe { stack.replace_last(ex) };
        unsafe {
            ex.write(Exception::new(cause));
        }
        ex
    })
}

pub fn is_recoverable<E>(ptr: *const Exception<E>) -> bool {
    EXCEPTIONS.with(|stack| stack.is_recoverable(ptr))
}

pub unsafe fn last_local<E>() -> *mut Exception<E> {
    EXCEPTIONS.with(|stack| unsafe { stack.recover_last_mut::<Exception<E>>() })
}
