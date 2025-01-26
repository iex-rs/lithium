// This is partially taken from
// - https://github.com/rust-lang/rust/blob/master/library/panic_unwind/src/emcc.rs

use super::{
    super::{abort, intrinsic::intercept},
    ThrowByPointer,
};

pub(crate) struct ActiveBackend;

/// Emscripten unwinding.
///
/// At the moment, the emscripten target doesn't provide a Itanium-compatible ABI, but it does
/// libcxxabi-style C++ exceptions. This is what we're going to use.
// SAFETY: C++ exceptions satisfy the requirements.
unsafe impl ThrowByPointer for ActiveBackend {
    type ExceptionHeader = Header;

    fn new_header() -> Header {
        Header {
            reference_count: 0,
            exception_type: core::ptr::null(),
            exception_destructor: None,
            caught: false,
            rethrown: false,
            adjusted_ptr: core::ptr::null_mut(),
            padding: core::ptr::null(),
        }
    }

    #[inline]
    unsafe fn throw(ex: *mut Header) -> ! {
        // SAFETY: This is in-bounds for the header.
        let end_of_header = unsafe { ex.add(1) }.cast();

        // SAFETY: We provide a valid exception header.
        unsafe {
            __cxa_throw(end_of_header, &raw const TYPE_INFO, cleanup);
        }
    }

    #[inline(always)]
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Header> {
        let ptr = match intercept(func, |ex| {
            // SAFETY: `core::intrinsics::catch_unwind` provides a pointer to a stack-allocated
            // instance of `CatchData`. It needs to be read inside the `intercept` callback because
            // it'll be dead by the moment `intercept` returns.
            #[expect(
                clippy::cast_ptr_alignment,
                reason = "guaranteed to be aligned by rustc"
            )]
            unsafe {
                (*ex.cast::<CatchData>()).ptr
            }
        }) {
            Ok(result) => return Ok(result),
            Err(ptr) => ptr,
        };

        // SAFETY: `ptr` was obtained from a `core::intrinsics::catch_unwind` call.
        let adjusted_ptr = unsafe { __cxa_begin_catch(ptr) };

        // SAFETY: `adjusted_ptr` points at what the unwinder thinks is a beginning of our exception
        // object. In reality, this is just the ned of header, so `sub(1)` yields the beginning of
        // the header.
        let ex: *mut Header = unsafe { adjusted_ptr.cast::<Header>().sub(1) };

        // SAFETY: `ex` points at a valid header. We're unique, so no data races are possible.
        if unsafe { (*ex).exception_type } != &raw const TYPE_INFO {
            // Rust panic or a foreign exception. Either way, rethrow.
            // SAFETY: This function has no preconditions.
            unsafe {
                __cxa_rethrow();
            }
        }

        // Prevent `__cxa_end_catch` from trying to deallocate the exception object with free(3) and
        // corrupting the heap.
        // SAFETY: We require that Lithium exceptions are not caught by foreign runtimes, so we
        // assume this is still a unique reference to this exception.
        unsafe {
            (*ex).reference_count = 2;
        }

        // SAFETY: This function has no preconditions.
        unsafe {
            __cxa_end_catch();
        }

        Err(ex)
    }
}

// This is __cxa_exception from emscripten sources.
#[repr(C)]
pub(crate) struct Header {
    reference_count: usize,
    exception_type: *const TypeInfo,
    exception_destructor: Option<unsafe fn(*mut ()) -> *mut ()>,
    caught: bool,
    rethrown: bool,
    adjusted_ptr: *mut (),
    padding: *const (),
}

// This is std::type_info.
#[repr(C)]
struct TypeInfo {
    vtable: *const usize,
    name: *const i8,
}

// SAFETY: `!Sync` pointers are stupid.
unsafe impl Sync for TypeInfo {}

#[repr(C)]
struct CatchData {
    ptr: *mut (),
    is_rust_panic: bool,
}

extern "C" {
    #[link_name = "\x01_ZTVN10__cxxabiv117__class_type_infoE"]
    static CLASS_TYPE_INFO_VTABLE: [usize; 3];
}

static TYPE_INFO: TypeInfo = TypeInfo {
    // Normally we would use .as_ptr().add(2) but this doesn't work in a const context.
    vtable: unsafe { &CLASS_TYPE_INFO_VTABLE[2] },
    name: c"lithium_exception".as_ptr(),
};

extern "C-unwind" {
    fn __cxa_begin_catch(thrown_exception: *mut ()) -> *mut ();

    fn __cxa_rethrow() -> !;

    fn __cxa_end_catch();

    fn __cxa_throw(
        thrown_object: *mut (),
        tinfo: *const TypeInfo,
        destructor: unsafe extern "C" fn(*mut ()) -> *mut (),
    ) -> !;
}

/// Destruct an exception when caught by a foreign runtime.
///
/// # Safety
///
/// `ex` must point at a valid exception object.
unsafe extern "C" fn cleanup(_ex: *mut ()) -> *mut () {
    abort("A Lithium exception was caught by a non-Lithium catch mechanism. This is undefined behavior. The process will now terminate.\n");
}
