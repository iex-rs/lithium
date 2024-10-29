// This is partially taken from
// - https://github.com/rust-lang/rust/blob/master/library/panic_unwind/src/seh.rs
// with inspiration from ReactOS and Wine sources.

use super::{RethrowHandle, ThrowByValue};
use core::marker::{FnPtr, PhantomData};
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct ActiveBackend;

/// SEH-based unwinding.
///
/// Just like with Itanium, we piggy-back on the [`core::intrinsics::catch_unwind`] intrinsic.
/// Currently, it's configured to catch C++ exceptions with mangled type `rust_panic`, so that's the
/// kind of exception we have to throw.
///
/// This means that we'll also catch Rust panics, so we need to be able to separate them from our
/// exceptions. Luckily, Rust already puts a `canary` field in the exception object to check if it's
/// caught an exception by another Rust std; we'll use it for our own purposes by providing a unique
/// canary value.
// SAFETY: SEH satisfies the requirements.
unsafe impl ThrowByValue for ActiveBackend {
    type RethrowHandle<E> = SehRethrowHandle;

    #[inline]
    unsafe fn throw<E>(cause: E) -> ! {
        // We have to initialize these variables late because we can't ask the linker to do the
        // relative address computation for us. Using atomics for this removes races in Rust code,
        // but atomic writes can still race with non-atomic reads in the vcruntime code. Luckily, we
        // aren't going to LTO with vcruntime.
        CATCHABLE_TYPE
            .type_descriptor
            .write(SmallPtr::new(&raw const TYPE_DESCRIPTOR));
        CATCHABLE_TYPE.copy_function.write(SmallPtr::new_fn(copy));
        CATCHABLE_TYPE_ARRAY.catchable_types[0].write(SmallPtr::new(&raw const CATCHABLE_TYPE));
        THROW_INFO.destructor.write(SmallPtr::new_fn(cleanup));
        THROW_INFO
            .catchable_type_array
            .write(SmallPtr::new(&raw const CATCHABLE_TYPE_ARRAY));

        // SAFETY: We've just initialized the tables.
        unsafe {
            do_throw(cause);
        }
    }

    #[inline]
    unsafe fn intercept<Func: FnOnce() -> R, R, E>(func: Func) -> Result<R, (E, SehRethrowHandle)> {
        union Data<Func, R, E> {
            func: ManuallyDrop<Func>,
            result: ManuallyDrop<R>,
            cause: ManuallyDrop<E>,
        }

        #[inline]
        fn do_call<Func: FnOnce() -> R, R, E>(data: *mut u8) {
            // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer
            // to the `data` variable.
            let data: &mut Data<Func, R, E> = unsafe { &mut *data.cast() };
            // SAFETY: This function is called at the start of the process, so the `func` field is
            // still initialized.
            let func = unsafe { ManuallyDrop::take(&mut data.func) };
            data.result = ManuallyDrop::new(func());
        }

        #[inline]
        fn do_catch<Func: FnOnce() -> R, R, E>(data: *mut u8, ex: *mut u8) {
            // SAFETY: `data` is provided by the `catch_unwind` intrinsic, which copies the pointer
            // to the `data` variable.
            let data: &mut Data<Func, R, E> = unsafe { &mut *data.cast() };
            let ex: *mut Exception<E> = ex.cast();

            // Rethrow foreign exceptions, as well as Rust panics.
            // SAFETY: If `ex` is non-null, it's a `rust_panic` exception, which can either be
            // thrown by us or by the Rust runtime; both have the `header.canary` field as the first
            // field in their structures.
            if ex.is_null() || unsafe { (*ex).header.canary } != (&raw const THROW_INFO).cast() {
                // SAFETY: Rethrowing is always valid.
                unsafe {
                    cxx_throw(core::ptr::null_mut(), core::ptr::null());
                }
            }

            // SAFETY: This is our exception, so `ex` points at a valid instance of `Exception<E>`.
            unsafe {
                (*ex).header.caught = true;
            }
            // SAFETY: As above.
            let cause = unsafe { &mut (*ex).cause };
            // SAFETY: We only read the cause here, so no double copies.
            data.cause = ManuallyDrop::new(unsafe { ManuallyDrop::take(cause) });
        }

        let mut data = Data {
            func: ManuallyDrop::new(func),
        };

        // SAFETY: `do_catch` doesn't do anything that might unwind
        if unsafe {
            core::intrinsics::catch_unwind(
                do_call::<Func, R, E>,
                (&raw mut data).cast(),
                do_catch::<Func, R, E>,
            )
        } == 0i32
        {
            // SAFETY: If zero was returned, no unwinding happened, so `do_call` must have finished
            // till the assignment to `data.result`.
            return Ok(ManuallyDrop::into_inner(unsafe { data.result }));
        }

        // SAFETY: If a non-zero value was returned, unwinding has happened, so `do_catch` was
        // invoked, thus `data.ex` is initialized now.
        let cause = ManuallyDrop::into_inner(unsafe { data.cause });
        Err((cause, SehRethrowHandle))
    }
}

#[derive(Debug)]
pub(crate) struct SehRethrowHandle;

impl RethrowHandle for SehRethrowHandle {
    unsafe fn rethrow<F>(self, new_cause: F) -> ! {
        // SAFETY: This is a rethrow, so the first throw must have initialized the tables.
        unsafe {
            do_throw(new_cause);
        }
    }
}

/// Throw an exception as a C++ exception.
///
/// # Safety
///
/// The caller must ensure all global tables are initialized.
unsafe fn do_throw<E>(cause: E) -> ! {
    let mut exception = Exception {
        header: ExceptionHeader {
            canary: (&raw const THROW_INFO).cast(), // any static will work
            caught: false,
        },
        cause: ManuallyDrop::new(cause),
    };

    // SAFETY: THROW_INFO exists for the whole duration of the program.
    unsafe {
        cxx_throw((&raw mut exception).cast(), &raw const THROW_INFO);
    }
}

#[repr(C)]
struct ExceptionHeader {
    canary: *const (), // From Rust ABI
    caught: bool,
}

#[repr(C)]
struct Exception<E> {
    header: ExceptionHeader,
    cause: ManuallyDrop<E>,
}

#[cfg(target_arch = "x86")]
macro_rules! thiscall {
    ($($tt:tt)*) => {
        unsafe extern "thiscall" $($tt)*
    };
}
#[cfg(not(target_arch = "x86"))]
macro_rules! thiscall {
    ($($tt:tt)*) => {
        unsafe extern "C" $($tt)*
    };
}

#[repr(C)]
struct ExceptionRecordParameters {
    magic: usize,
    exception_object: *mut ExceptionHeader,
    throw_info: *const ThrowInfo,
    #[cfg(target_pointer_width = "64")]
    image_base: *const u8,
}

#[repr(C)]
struct ThrowInfo {
    attributes: u32,
    destructor: SmallPtr<thiscall!(fn(*mut ExceptionHeader))>,
    forward_compat: SmallPtr<fn()>,
    catchable_type_array: SmallPtr<*const CatchableTypeArray>,
}

#[repr(C)]
struct CatchableTypeArray {
    n_types: i32,
    catchable_types: [SmallPtr<*const CatchableType>; 1],
}

#[repr(C)]
struct CatchableType {
    properties: u32,
    type_descriptor: SmallPtr<*const TypeDescriptor>,
    this_displacement: PointerToMemberData,
    size_or_offset: i32,
    copy_function: SmallPtr<
        thiscall!(fn(*mut ExceptionHeader, *const ExceptionHeader) -> *mut ExceptionHeader),
    >,
}

#[repr(C)]
struct TypeDescriptor {
    vtable: *const *const (),
    reserved: usize,
    name: [u8; 11], // null-terminated
}
// SAFETY: `!Sync` for pointers is stupid.
unsafe impl Sync for TypeDescriptor {}

#[repr(C)]
struct PointerToMemberData {
    member_displacement: i32,
    virtual_base_pointer_displacement: i32,
    vdisp: i32, // ???
}

// See ehdata.h for definitions
const EH_EXCEPTION_NUMBER: u32 = u32::from_be_bytes(*b"\xe0msc");
const EH_NONCONTINUABLE: u32 = 1;
const EH_MAGIC_NUMBER1: usize = 0x1993_0520; // Effectively a version

static TYPE_DESCRIPTOR: TypeDescriptor = TypeDescriptor {
    vtable: &raw const TYPE_INFO_VTABLE,
    reserved: 0,
    name: *b"rust_panic\0",
};

static CATCHABLE_TYPE: CatchableType = CatchableType {
    properties: 0,
    type_descriptor: SmallPtr::null(), // filled by throw
    this_displacement: PointerToMemberData {
        member_displacement: 0,
        virtual_base_pointer_displacement: -1,
        vdisp: 0,
    },
    // We don't really have a good answer to this, and we don't let the C++ runtime catch our
    // exception, so it's not a big problem.
    size_or_offset: 1,
    copy_function: SmallPtr::null(), // filled by throw
};

static CATCHABLE_TYPE_ARRAY: CatchableTypeArray = CatchableTypeArray {
    n_types: 1,
    catchable_types: [
        SmallPtr::null(), // filled by throw
    ],
};

static THROW_INFO: ThrowInfo = ThrowInfo {
    attributes: 0,
    destructor: SmallPtr::null(), // filled by throw
    forward_compat: SmallPtr::null(),
    catchable_type_array: SmallPtr::null(), // filled by throw
};

fn abort() -> ! {
    #[cfg(feature = "std")]
    {
        eprintln!(
            "A Lithium exception was caught by a non-Lithium catch mechanism. This is undefined behavior. The process will now terminate.",
        );
        std::process::abort();
    }
    #[cfg(not(feature = "std"))]
    core::intrinsics::abort();
}

macro_rules! define_fns {
    ($abi:tt) => {
        unsafe extern $abi fn cleanup(ex: *mut ExceptionHeader) {
            // SAFETY: `ex` is a `this` pointer when called by the C++ runtime.
            if !unsafe { (*ex).caught } {
                // Caught by the cxx runtime
                abort();
            }
        }

        unsafe extern $abi fn copy(_to: *mut ExceptionHeader, _from: *const ExceptionHeader) -> *mut ExceptionHeader {
            abort();
        }
    };
}

#[cfg(target_arch = "x86")]
define_fns!("thiscall");
#[cfg(not(target_arch = "x86"))]
define_fns!("C");

extern "C" {
    #[cfg(target_pointer_width = "64")]
    static __ImageBase: u8;

    #[link_name = "\x01??_7type_info@@6B@"]
    static TYPE_INFO_VTABLE: *const ();
}

#[repr(transparent)]
struct SmallPtr<P> {
    value: AtomicU32,
    phantom: PhantomData<P>,
}

// SAFETY: `!Sync` for pointers is stupid.
unsafe impl<P> Sync for SmallPtr<P> {}

impl<P> SmallPtr<P> {
    fn from_erased(p: *const ()) -> Self {
        #[cfg(target_pointer_width = "32")]
        let value = p.expose_provenance();
        #[cfg(target_pointer_width = "64")]
        let value = if p.is_null() {
            0
        } else {
            p.expose_provenance()
                .wrapping_sub((&raw const __ImageBase).addr())
                .try_into()
                .map_err(|_| format!("address: {:p}, image base: {:p}", p, &raw const __ImageBase))
                .expect("Too large image")
        };
        Self {
            value: AtomicU32::new(value),
            phantom: PhantomData,
        }
    }

    const fn null() -> Self {
        Self {
            value: AtomicU32::new(0),
            phantom: PhantomData,
        }
    }

    fn write(&self, rhs: SmallPtr<P>) {
        self.value.store(rhs.value.into_inner(), Ordering::Relaxed);
    }
}

impl<P: FnPtr> SmallPtr<P> {
    fn new_fn(p: P) -> Self {
        Self::from_erased(p.addr())
    }
}

impl<T: ?Sized> SmallPtr<*const T> {
    fn new(p: *const T) -> Self {
        Self::from_erased(p.cast())
    }
}

extern "system-unwind" {
    fn RaiseException(
        code: u32,
        flags: u32,
        n_parameters: u32,
        paremeters: *mut ExceptionRecordParameters,
    ) -> !;
}

/// Throw a C++ exception.
///
/// # Safety
///
/// `throw_info` must point to a correctly initialized `ThrowInfo` value, valid for the whole
/// duration of the unwinding procedure.
unsafe fn cxx_throw(exception_object: *mut ExceptionHeader, throw_info: *const ThrowInfo) -> ! {
    // This is a reimplementation of `_CxxThrowException`, with quite a few information hardcoded
    // and functions calls inlined.
    let mut parameters = ExceptionRecordParameters {
        magic: EH_MAGIC_NUMBER1,
        exception_object,
        throw_info,
        #[cfg(target_pointer_width = "64")]
        image_base: &raw const __ImageBase,
    };

    // SAFETY: Just an extern call.
    unsafe {
        RaiseException(
            EH_EXCEPTION_NUMBER,
            EH_NONCONTINUABLE,
            #[expect(clippy::arithmetic_side_effects)]
            (core::mem::size_of::<ExceptionRecordParameters>() / core::mem::size_of::<usize>())
                .try_into()
                .unwrap(),
            &raw mut parameters,
        );
    }
}
