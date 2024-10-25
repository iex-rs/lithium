use core::mem::{ManuallyDrop, MaybeUninit};

pub const LITHIUM_EXCEPTION_CLASS: u64 = u64::from_ne_bytes(*b"RUSTLITH");

#[repr(C)]
pub struct UnwindException {
    pub class: u64,
    cleanup: Option<unsafe extern "C" fn(i32, *mut UnwindException)>,
    private: MaybeUninit<[*const (); 2]>,
}

#[repr(C)] // unwind must be the first field
pub struct Exception<E> {
    unwind: UnwindException,
    cause: ManuallyDrop<E>,
}

impl<E> Exception<E> {
    pub fn new(cause: E) -> Self {
        Self {
            unwind: UnwindException {
                class: LITHIUM_EXCEPTION_CLASS,
                cleanup: Some(cleanup),
                private: MaybeUninit::uninit(),
            },
            cause: ManuallyDrop::new(cause),
        }
    }

    pub unsafe fn place(ptr: *mut Self, cause: E) {
        ptr.write(Self::new(cause))
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

unsafe extern "C" fn cleanup(_code: i32, _ex: *mut UnwindException) {
    eprintln!(
        "A Lithium exception was caught by a non-Lithium catch mechanism. The process will now terminate.",
    );
    std::process::abort();
}
