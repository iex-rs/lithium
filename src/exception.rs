use super::backend::Header;
use core::mem::ManuallyDrop;

#[repr(C)] // header must be the first field
pub struct Exception<E> {
    header: Header,
    cause: ManuallyDrop<E>,
}

impl<E> Exception<E> {
    pub fn new(cause: E) -> Self {
        Self {
            header: Header::new(),
            cause: ManuallyDrop::new(cause),
        }
    }

    pub unsafe fn replace_cause<F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
        let ex: *mut Exception<F> = ex.cast();
        let cause_ptr = unsafe { &raw mut (*ex).cause };
        unsafe {
            core::ptr::write_unaligned(cause_ptr, ManuallyDrop::new(cause));
        }
        ex
    }

    pub unsafe fn read_cause(ex: *mut Exception<E>) -> E {
        let cause_ptr = unsafe { &raw mut (*ex).cause };
        ManuallyDrop::into_inner(unsafe { core::ptr::read_unaligned(cause_ptr) })
    }

    pub fn heap_alloc(cause: E) -> *mut Self {
        Box::into_raw(Box::new(Exception::new(cause)))
    }
    pub unsafe fn heap_dealloc(ex: *mut Self) {
        drop(unsafe { Box::from_raw(ex) });
    }
}
