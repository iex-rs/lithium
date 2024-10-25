use super::exceptions::{Exception, UnwindException};
use core::alloc::Layout;
use core::cell::{Cell, UnsafeCell};
use core::mem::{size_of, MaybeUninit};

thread_local! {
    static EXCEPTIONS: StackAllocator = const { StackAllocator::new() };
}

#[repr(C)]
struct StackAllocator {
    size: Cell<usize>,
    _align: [UnwindException; 0],
    data: UnsafeCell<[MaybeUninit<u8>; Self::LOCAL_LEN]>, // aligned to UnwindException
}

impl StackAllocator {
    const LOCAL_LEN: usize = 4096;

    const fn new() -> Self {
        Self {
            size: Cell::new(0),
            _align: [],
            data: UnsafeCell::new([MaybeUninit::uninit(); Self::LOCAL_LEN]),
        }
    }

    fn is_local<E>(&self, ex: *mut Exception<E>) -> bool {
        size_of::<Exception<E>>() <= Self::LOCAL_LEN
            && ex.addr().wrapping_sub(self.data.get().addr()) < Self::LOCAL_LEN
    }
    fn can_be_local<E>(&self) -> bool {
        size_of::<Exception<E>>() <= Self::LOCAL_LEN
            && self.size.get() + size_of::<Exception<E>>() <= Self::LOCAL_LEN
    }

    unsafe fn push_local<E>(&self, cause: E) -> *mut Exception<E> {
        let ex = self.data.get().byte_add(self.size.get()).cast();
        Exception::place(ex, cause);
        self.size.set(self.size.get() + size_of::<Exception<E>>());
        ex
    }
    unsafe fn pop_local<E>(&self) {
        self.size.set(self.size.get() - size_of::<Exception<E>>());
    }

    fn push<E>(&self, cause: E) -> *mut Exception<E> {
        if self.can_be_local::<E>() {
            unsafe { self.push_local(cause) }
        } else {
            Exception::heap_alloc(cause)
        }
    }

    unsafe fn pop<E>(&self, ex: *mut Exception<E>) {
        if self.is_local(ex) {
            self.pop_local::<E>();
        } else {
            Exception::heap_dealloc(ex);
        }
    }

    unsafe fn replace_last<E, F>(&self, ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
        if self.is_local(ex) {
            self.pop_local::<E>();
            if size_of::<F>() <= size_of::<E>() || self.can_be_local::<F>() {
                // Fits in local data. Avoid push_local so that ex is not recomputed from size
                self.size.set(self.size.get() + size_of::<Exception<F>>());
                return Exception::replace_cause(ex, cause);
            }
        } else {
            // Box<T>'s are compatible as long as Ts have identical layouts. Which is a good thing,
            // because that's a lot easier to check than type equality.
            if Layout::new::<Exception<E>>() == Layout::new::<Exception<F>>() {
                return Exception::replace_cause(ex, cause);
            }
            Exception::heap_dealloc(ex);
            if size_of::<F>() < size_of::<E>() && self.can_be_local::<F>() {
                // Fits in local data
                return self.push_local(cause);
            }
        }
        Exception::heap_alloc(cause)
    }
}

pub fn push<E>(cause: E) -> *mut Exception<E> {
    EXCEPTIONS.with(|store| store.push(cause))
}

pub unsafe fn pop<E>(ex: *mut Exception<E>) {
    EXCEPTIONS.with(|store| store.pop(ex))
}

pub unsafe fn replace_last<E, F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
    EXCEPTIONS.with(|store| store.replace_last(ex, cause))
}
