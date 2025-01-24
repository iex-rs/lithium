// This is partially taken from
// - https://github.com/rust-lang/rust/blob/master/library/panic_unwind/src/seh.rs
// with exception constants and the throwing interface retrieved from ReactOS and Wine sources.

use super::{
    super::{abort, intrinsic::intercept},
    RethrowHandle, ThrowByValue,
};
use alloc::boxed::Box;
use core::any::Any;
use core::marker::{FnPtr, PhantomData};
use core::mem::ManuallyDrop;
use core::panic::PanicPayload;
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
///
/// SEH has its share of problems, but one cool detail is that stack is not unwinded until the catch
/// handler returns. This means that we can save the exception object on stack and then copy it to
/// the destination from the catch handler, thus reducing allocations.
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
        enum CaughtUnwind<E> {
            LithiumException(E),
            RustPanic(Box<dyn Any + Send + 'static>),
        }

        let catch = |ex: *mut u8| {
            // This callback is not allowed to unwind, so we can't rethrow exceptions.
            if ex.is_null() {
                // This is a foreign exception.
                abort(
                    "Lithium caught a foreign exception. This is unsupported. The process will now terminate.\n",
                );
            }

            let ex_lithium: *mut Exception<E> = ex.cast();

            // SAFETY: If `ex` is non-null, it's a `rust_panic` exception, which can either be
            // thrown by us or by the Rust runtime; both have the `header.canary` field as the first
            // field in their structures.
            if unsafe { (*ex_lithium).header.canary } != (&raw const THROW_INFO).cast() {
                // This is a Rust exception. We can't rethrow it immediately from this nounwind
                // callback, so let's catch it first.
                // SAFETY: `ex` is the callback value of `core::intrinsics::catch_unwind`.
                let payload = unsafe { __rust_panic_cleanup(ex) };
                // SAFETY: `__rust_panic_cleanup` returns a Box.
                let payload = unsafe { Box::from_raw(payload) };
                return CaughtUnwind::RustPanic(payload);
            }

            // We catch the exception by reference, so the C++ runtime will drop it. Tell our
            // destructor to calm down.
            // SAFETY: This is our exception, so `ex_lithium` points at a valid instance of
            // `Exception<E>`.
            unsafe {
                (*ex_lithium).header.caught = true;
            }
            // SAFETY: As above.
            let cause = unsafe { &mut (*ex_lithium).cause };
            // SAFETY: We only read the cause here, so no double copies.
            CaughtUnwind::LithiumException(unsafe { ManuallyDrop::take(cause) })
        };

        match intercept(func, catch) {
            Ok(value) => Ok(value),
            Err(CaughtUnwind::LithiumException(cause)) => Err((cause, SehRethrowHandle)),
            Err(CaughtUnwind::RustPanic(payload)) => throw_std_panic(payload),
        }
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
    ($(#[$outer:meta])* fn $($tt:tt)*) => {
        $(#[$outer])* unsafe extern "thiscall" fn $($tt)*
    };
}
#[cfg(not(target_arch = "x86"))]
macro_rules! thiscall {
    ($(#[$outer:meta])* fn $($tt:tt)*) => {
        $(#[$outer])* unsafe extern "C" fn $($tt)*
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

fn abort_on_caught_by_cxx() -> ! {
    abort("A Lithium exception was caught by a non-Lithium catch mechanism. This is undefined behavior. The process will now terminate.\n");
}

thiscall! {
    /// Destruct an exception object.
    ///
    /// # Safety
    ///
    /// `ex` must point at a valid exception object.
    fn cleanup(ex: *mut ExceptionHeader) {
        // SAFETY: `ex` is a `this` pointer when called by the C++ runtime.
        if !unsafe { (*ex).caught } {
            // Caught by the cxx runtime
            abort_on_caught_by_cxx();
        }
    }
}

thiscall! {
    /// Copy an exception object.
    ///
    /// # Safety
    ///
    /// `from` must point at a valid exception object, while `to` must point at a suitable
    /// allocation for the new object.
    fn copy(_to: *mut ExceptionHeader, _from: *const ExceptionHeader) -> *mut ExceptionHeader {
        abort_on_caught_by_cxx();
    }
}

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
    /// Construct a small pointer.
    ///
    /// # Panics
    ///
    /// Panics if `p` is too far from the image base.
    #[inline]
    fn from_erased(p: *const ()) -> Self {
        #[cfg(target_pointer_width = "32")]
        let value = p.expose_provenance() as u32;
        #[cfg(target_pointer_width = "64")]
        let value = p
            .expose_provenance()
            .wrapping_sub((&raw const __ImageBase).addr())
            .try_into()
            .expect("Too large image");
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

// This is provided by the `panic_unwind` built-in crate, so it's always available if
// panic = "unwind" holds
extern "Rust" {
    fn __rust_start_panic(payload: &mut dyn PanicPayload) -> u32;
}

extern "C" {
    #[expect(improper_ctypes, reason = "Copied from std")]
    fn __rust_panic_cleanup(payload: *mut u8) -> *mut (dyn Any + Send + 'static);
}

fn throw_std_panic(payload: Box<dyn Any + Send + 'static>) -> ! {
    // We can't use resume_unwind here, as it increments the panic count, and we didn't decrement it
    // upon catching the panic. Call `__rust_start_panic` directly instead.
    struct RewrapBox(Box<dyn Any + Send + 'static>);

    // SAFETY: Copied straight from std.
    unsafe impl PanicPayload for RewrapBox {
        fn take_box(&mut self) -> *mut (dyn Any + Send) {
            Box::into_raw(core::mem::replace(&mut self.0, Box::new(())))
        }
        fn get(&mut self) -> &(dyn Any + Send) {
            &*self.0
        }
    }

    impl core::fmt::Display for RewrapBox {
        fn fmt(&self, _f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            // `__rust_start_panic` is not supposed to use the `Display` implementation in unwinding
            // mode.
            unreachable!()
        }
    }

    // SAFETY: Copied straight from std.
    unsafe {
        __rust_start_panic(&mut RewrapBox(payload));
    }
    core::intrinsics::abort();
}

/// Throw a C++ exception.
///
/// # Safety
///
/// `throw_info` must point to a correctly initialized `ThrowInfo` value, valid for the whole
/// duration of the unwinding procedure.
#[inline(always)]
unsafe fn cxx_throw(exception_object: *mut ExceptionHeader, throw_info: *const ThrowInfo) -> ! {
    // This is a reimplementation of `_CxxThrowException`, with quite a few information hardcoded
    // and functions calls inlined.

    #[expect(clippy::cast_possible_truncation, reason = "This is a constant")]
    const N_PARAMETERS: u32 =
        (core::mem::size_of::<ExceptionRecordParameters>() / core::mem::size_of::<usize>()) as u32;

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
            N_PARAMETERS,
            &raw mut parameters,
        );
    }
}
