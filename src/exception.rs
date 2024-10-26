use super::backend::Header;
use core::mem::ManuallyDrop;

#[repr(C)] // header must be the first field
pub struct Exception<E> {
    header: Header,
    cause: Unaligned<ManuallyDrop<E>>,
}

#[repr(packed)]
struct Unaligned<T>(T);

impl<E> Exception<E> {
    pub fn new(cause: E) -> Self {
        Self {
            header: Header::new(),
            cause: Unaligned(ManuallyDrop::new(cause)),
        }
    }

    pub unsafe fn read_cause(ex: *mut Exception<E>) -> E {
        let cause_ptr = unsafe { &raw mut (*ex).cause.0 };
        ManuallyDrop::into_inner(unsafe { core::ptr::read_unaligned(cause_ptr) })
    }
}
