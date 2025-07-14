#[cfg(debug_assertions)]
mod imp {
    use crate::abort;
    use core::any::{TypeId, type_name};
    use core::marker::PhantomData;

    /// A best-effort runtime type checker for thrown and caught exception.
    ///
    /// Only enabled in debug, zero-cost in release.
    pub struct TypeChecker {
        id: TypeId,
        name: &'static str,
    }

    impl TypeChecker {
        // Create RTTI for `T`.
        pub fn new<T: ?Sized>() -> Self {
            Self {
                id: type_id::<T>(),
                name: type_name::<T>(),
            }
        }

        // Validate that `T` matches the stored RTTI (best-effort only).
        pub fn expect<T: ?Sized>(&self) {
            if self.id != type_id::<T>() {
                abort(&alloc::format!(
                    "lithium::catch::<_, {}> caught an exception of type {}. This is undefined behavior. The process will now terminate.\n",
                    core::any::type_name::<T>(),
                    self.name,
                ));
            }
        }
    }

    fn type_id<T: ?Sized>() -> TypeId {
        // This implements `core::any::type_id` for non-`'static` types and is simply copied from
        // dtolnay's `typeid` crate. That's a small crate, but keeping Lithium zero-dep sounds
        // worthwhile, and it'll probably slightly improve compilation times.
        trait NonStaticAny {
            fn get_type_id(&self) -> TypeId
            where
                Self: 'static;
        }

        impl<T: ?Sized> NonStaticAny for PhantomData<T> {
            fn get_type_id(&self) -> TypeId
            where
                Self: 'static,
            {
                TypeId::of::<T>()
            }
        }

        NonStaticAny::get_type_id(
            // SAFETY: Just a lifetime transmute, we never handle references with the extended
            // lifetime.
            unsafe {
                core::mem::transmute::<&dyn NonStaticAny, &(dyn NonStaticAny + 'static)>(
                    &PhantomData::<T>,
                )
            },
        )
    }
}

#[cfg(not(debug_assertions))]
mod imp {
    /// A best-effort runtime type checker for thrown and caught exception.
    ///
    /// Only enabled in debug, zero-cost in release.
    pub struct TypeChecker;

    impl TypeChecker {
        // Create RTTI for `T`.
        pub fn new<T: ?Sized>() -> Self {
            Self
        }

        // Validate that `T` matches the stored RTTI (best-effort only).
        pub fn expect<T: ?Sized>(&self) {}
    }
}

pub use imp::TypeChecker;
