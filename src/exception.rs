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

    pub unsafe fn place(ptr: *mut Self, cause: E) {
        ptr.write(Self::new(cause));
    }
    pub unsafe fn replace_cause<F>(ex: *mut Exception<E>, cause: F) -> *mut Exception<F> {
        let ex: *mut Exception<F> = ex.cast();
        core::ptr::write_unaligned(&raw mut (*ex).cause, ManuallyDrop::new(cause));
        ex
    }

    pub unsafe fn read_cause(ex: *mut Exception<E>) -> E {
        ManuallyDrop::into_inner(core::ptr::read_unaligned(&raw mut (*ex).cause))
    }

    pub fn heap_alloc(cause: E) -> *mut Self {
        Box::into_raw(Box::new(Exception::new(cause)))
    }
    pub unsafe fn heap_dealloc(ex: *mut Self) {
        drop(Box::from_raw(ex));
    }
}
