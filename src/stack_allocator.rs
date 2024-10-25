use super::{backend::AlignAs, exception::Exception, heterogeneous_stack::Stack};
use core::alloc::Layout;
use core::mem::size_of;

thread_local! {
    static EXCEPTIONS: StackAllocator = const { StackAllocator::new() };
}

#[repr(C)]
struct StackAllocator {
    stack: Stack<AlignAs, 4096>,
}

impl StackAllocator {
    const fn new() -> Self {
        Self {
            stack: Stack::new(),
        }
    }

    fn push<E>(&self, cause: E) -> (bool, *mut Exception<E>) {
        match self.stack.try_push() {
            Some(item) => (true, item.write(Exception::new(cause))),
            None => (false, Exception::heap_alloc(cause)),
        }
    }

    unsafe fn pop<E>(&self, ex: *mut Exception<E>) {
        if self
            .stack
            .contains_allocated::<Exception<E>>(unsafe { &*ex })
        {
            unsafe {
                self.stack.pop_unchecked::<Exception<E>>();
            }
        } else {
            unsafe {
                Exception::heap_dealloc(ex);
            }
        }
    }

    unsafe fn replace_last<E, F>(
        &self,
        ex: *mut Exception<E>,
        cause: F,
    ) -> (bool, *mut Exception<F>) {
        if self
            .stack
            .contains_allocated::<Exception<E>>(unsafe { &*ex })
        {
            unsafe {
                self.stack.pop_unchecked::<Exception<E>>();
            }
            if size_of::<F>() <= size_of::<E>() {
                // Necessarily fits in local data
                let ex: &mut Exception<E> =
                    unsafe { self.stack.try_push().unwrap_unchecked().assume_init_mut() };
                return (true, unsafe { Exception::replace_cause(ex, cause) });
            }
        } else {
            // Box<T>'s are compatible as long as Ts have identical layouts. Which is a good thing,
            // because that's a lot easier to check than type equality.
            if Layout::new::<Exception<E>>() == Layout::new::<Exception<F>>() {
                return (false, unsafe { Exception::replace_cause(ex, cause) });
            }
            unsafe {
                Exception::heap_dealloc(ex);
            }
            // Can't fit in local data
            if size_of::<F>() >= size_of::<E>() {
                return (false, Exception::heap_alloc(cause));
            }
        }
        self.push(cause)
    }

    #[allow(dead_code)]
    unsafe fn last_local<E>(&self) -> *mut Exception<E> {
        unsafe { self.stack.last_mut::<Exception<E>>().assume_init_mut() }
    }
}

pub fn push<E>(cause: E) -> (bool, *mut Exception<E>) {
    EXCEPTIONS.with(|store| store.push(cause))
}

pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    EXCEPTIONS.with(|store| unsafe { store.pop(ex) });
}

pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> (bool, *mut Exception<F>) {
    EXCEPTIONS.with(|store| unsafe { store.replace_last(ex, cause) })
}

#[allow(dead_code)]
pub unsafe fn last_local<E>() -> *mut Exception<E> {
    EXCEPTIONS.with(|store| unsafe { store.last_local() })
}
