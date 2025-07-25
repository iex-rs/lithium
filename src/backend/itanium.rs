use super::{
    super::{abort, intrinsic::intercept},
    ThrowByPointer,
};
use core::mem::MaybeUninit;

pub const LITHIUM_EXCEPTION_CLASS: u64 = u64::from_ne_bytes(*b"RUSTIEX\0");

pub(crate) struct ActiveBackend;

// SAFETY: We use Itanium EH ABI, which supports nested exceptions correctly. We can assume we don't
// encounter foreign frames, because that's a safety requirement of `throw`. The ABI guarantees that
// the most of the header fields stay valid after catching because the exception may theoretically
// be rethrown. No guarantees are explicitly made about `private1`, but it's also preserved in
// practice; see the comment below.
unsafe impl ThrowByPointer for ActiveBackend {
    type ExceptionHeader = Header;

    fn new_header() -> Header {
        Header {
            class: LITHIUM_EXCEPTION_CLASS,
            cleanup: Some(cleanup),
            // ARM EH ABI [1] requires that the first private field is initialized to 0 before the
            // unwind routines see it. This is not necessary for other architectures (except C6x),
            // but being consistent doesn't hurt. In practice, libgcc uses this field to store force
            // unwinding information, so leaving this uninitialized leads to SIGILLs and SIGSEGVs
            // because it uses the field as a callback address. Strictly speaking, we should
            // reinitialize this field back to zero when we do `_Unwind_RaiseException` later, but
            // this is unnecessary for libgcc, and libunwind uses the cross-platform mechanism for
            // ARM too.
            // [1]: https://github.com/ARM-software/abi-aa/blob/76d56124610302e645b66ac4e491be0c1a90ee11/ehabi32/ehabi32.rst#language-independent-unwinding-types-and-functions
            private1: if cfg!(all(target_arch = "arm", not(target_vendor = "apple"))) {
                MaybeUninit::new(core::ptr::null())
            } else {
                MaybeUninit::uninit()
            },
            private_rest: MaybeUninit::uninit(),
        }
    }

    #[inline]
    unsafe fn throw(ex: *mut Header) -> ! {
        // SAFETY: We provide a valid exception header.
        unsafe {
            raise(ex.cast());
        }
    }

    #[inline(always)]
    fn intercept<Func: FnOnce() -> R, R>(func: Func) -> Result<R, *mut Header> {
        let ex = match intercept(func, |ex| ex) {
            Ok(value) => return Ok(value),
            Err(ex) => ex,
        };

        // SAFETY: `ex` is a pointer to an exception object as provided by the unwinder, so it must
        // be valid for reads. It's not explicitly documented that the class is not modified in
        // runtime, but that sounds like common sense. Note that we only dereference the class
        // rather than the whole `Header`, as we don't know whether `ex` is aligned to `Header`, but
        // it must be at least aligned for `u64` access.
        #[expect(clippy::cast_ptr_alignment, reason = "See the safety comment above")]
        let class = unsafe { *ex.cast::<u64>() };

        if class != LITHIUM_EXCEPTION_CLASS {
            // SAFETY: The EH ABI allows rethrowing foreign exceptions under the following
            // conditions:
            // - The exception is not modified or otherwise interacted with. We don't do this,
            //   expect for determining whether it's foreign in the first place.
            // - Runtime EH-related functions are not invoked between catching the exception and
            //   rethrowing it. We don't do that.
            // - The foreign exception is not active at the same time as another exception. We don't
            //   trigger exceptions between catch and rethrow, so we only have to rule out the
            //   foreign exception being nested prior to our catch. This is somewhat complicated:
            //   - If the foreign exception is actually a Rust panic, we know from stdlib's code
            //     that the personality function works just fine with rethrowing regardless of
            //     nesting. This is not a hard proof, but this is highly unlikely to change.
            //   - If the foreign exception was produced neither by Rust, nor by Lithium, the case
            //     is similar to how the behavior of `std::panic::catch_unwind` being unwound by
            //     a foreign exception is undefined; i.e., it's on the user who allows foreign
            //     exceptions to travel through Lithium frames.
            //   If project-ffi-unwind changes the rustc behavior, we might have to update this
            //   code.
            unsafe {
                raise(ex);
            }
        }

        Err(ex.cast())
    }
}

// The alignment on this structure is... complicated. GCC uses `__attribute__((aligned))` here and
// expects everyone else to do the same, but we don't have that in Rust. The rules for computing the
// default (maximum) alignment are unclear. If we guess too low, the unwinder might access unaligned
// data, so we use 16 bytes on all platforms to keep safe. This includes 32-bit machines, becuase on
// i386 `__attribute__((aligned))` aligns to 16 bytes too. Therefore, the alignment of this
// structure might be larger than the actual alignment when we access foreign exceptions, so we
// can't use this type for that.
#[repr(C, align(16))]
pub struct Header {
    class: u64,
    cleanup: Option<unsafe extern "C" fn(i32, *mut Header)>,
    // See `new_header` for why this needs to be a separate field.
    private1: MaybeUninit<*const ()>,
    private_rest: MaybeUninit<[*const (); UNWINDER_PRIVATE_WORD_COUNT - 1]>,
}

// Data from https://github.com/rust-lang/rust/blob/master/library/unwind/src/libunwind.rs
const UNWINDER_PRIVATE_WORD_COUNT: usize = {
    // The Itanium EH ABI says the structure contains 2 private uint64_t words. Some architectures
    // decided this means "2 private native words". So on some 32-bit architectures this is two
    // 64-bit words, which together with padding amount to 5 native words, and on other
    // architectures it's two native words. Others are just stupid.
    if cfg!(target_arch = "x86") {
        5
    } else if cfg!(any(
        all(target_arch = "x86_64"),
        all(target_arch = "aarch64", target_pointer_width = "64"),
    )) {
        if cfg!(windows) { 6 } else { 2 }
    } else if cfg!(target_arch = "arm") {
        if cfg!(target_vendor = "apple") { 5 } else { 20 }
    } else if cfg!(all(target_arch = "aarch64", target_pointer_width = "32")) {
        5
    } else if cfg!(target_os = "emscripten") {
        20
    } else if cfg!(all(target_arch = "hexagon", target_os = "linux")) {
        35
    } else if cfg!(any(
        target_arch = "m68k",
        target_arch = "mips",
        target_arch = "mips32r6",
        target_arch = "csky",
        target_arch = "mips64",
        target_arch = "mips64r6",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "s390x",
        target_arch = "sparc",
        target_arch = "sparc64",
        target_arch = "riscv64",
        target_arch = "riscv32",
        target_arch = "loongarch64",
        target_arch = "wasm32"
    )) {
        2
    } else {
        panic!("Unsupported architecture");
    }
};

/// Destruct an exception when caught by a foreign runtime.
///
/// # Safety
///
/// `ex` must point at a valid exception object.
unsafe extern "C" fn cleanup(_code: i32, _ex: *mut Header) {
    abort(
        "A Lithium exception was caught by a non-Lithium catch mechanism. This is undefined behavior. The process will now terminate.\n",
    );
}

#[cfg(not(target_arch = "wasm32"))]
unsafe extern "C-unwind" {
    fn _Unwind_RaiseException(ex: *mut u8) -> !;
}

/// Raise an Itanium EH ABI-compatible exception.
///
/// # Safety
///
/// `ex` must point at a valid instance of `_Unwind_Exception`.
#[inline]
unsafe fn raise(ex: *mut u8) -> ! {
    #[cfg(not(target_arch = "wasm32"))]
    // SAFETY: Passthrough.
    unsafe {
        _Unwind_RaiseException(ex);
    }

    // Although Wasm has its own backend, it has worse debug experience than Itanium can offer, so
    // we teach this backend how to handle Wasm as well.
    #[cfg(target_arch = "wasm32")]
    // SAFETY: Passthrough.
    unsafe {
        core::arch::wasm32::throw::<0>(ex);
    }
}
