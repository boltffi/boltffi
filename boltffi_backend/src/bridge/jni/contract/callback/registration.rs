//! Registration contract for one JVM-implemented callback trait.
//!
//! A callback trait implemented on the JVM side needs more than method
//! wrappers. The generated C file must cache the callback class and method ids,
//! expose clone and free functions to the C bridge, and build one vtable with a
//! slot for every callback method.
//!
//! This module collects those pieces for one callback trait. It is built from
//! the C callback contract and then rendered as a single registration block in
//! the JNI source file.

use boltffi_binding::CallbackId;

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::{CallbackHandleMethod, CallbackMethod, ClosureRegistration, JvmClassPath},
    },
    core::Result,
};

/// JNI registration for one native callback vtable.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackRegistration {
    id: CallbackId,
    class: JvmClassPath,
    global_class: Identifier,
    free_method: Identifier,
    clone_method: Identifier,
    load: Identifier,
    unload: Identifier,
    vtable_type: Identifier,
    vtable: Identifier,
    register: Identifier,
    free: Identifier,
    clone: Identifier,
    methods: Vec<CallbackMethod>,
    handle_methods: Vec<CallbackHandleMethod>,
}

impl CallbackRegistration {
    /// Creates JNI callback registration from one C callback contract.
    pub fn from_c_callback(
        class: &JvmClassPath,
        callback: &c::Callback,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
        render_handle_methods: bool,
    ) -> Result<Self> {
        let stem = callback.vtable().name();
        Ok(Self {
            id: callback.id(),
            class: class.callback_class(callback.name())?,
            global_class: Identifier::parse(format!("g_{stem}_class"))?,
            free_method: Identifier::parse(format!("g_{stem}_free_method"))?,
            clone_method: Identifier::parse(format!("g_{stem}_clone_method"))?,
            load: Identifier::parse(format!("boltffi_jni_load_{stem}_callbacks"))?,
            unload: Identifier::parse(format!("boltffi_jni_unload_{stem}_callbacks"))?,
            vtable_type: Identifier::parse(stem)?,
            vtable: Identifier::parse(format!("g_{stem}_vtable"))?,
            register: Identifier::parse(callback.register().name())?,
            free: Identifier::parse(format!("{stem}_free"))?,
            clone: Identifier::parse(format!("{stem}_clone"))?,
            methods: callback
                .methods()
                .iter()
                .map(|slot| CallbackMethod::from_slot(stem, slot, callbacks, closures))
                .collect::<Result<Vec<_>>>()?,
            handle_methods: render_handle_methods
                .then(|| CallbackHandleMethod::from_callback(class, callback, callbacks, closures))
                .transpose()?
                .unwrap_or_default(),
        })
    }

    /// Returns the source callback id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the JVM callback bridge class.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the global class reference symbol.
    pub fn global_class(&self) -> &Identifier {
        &self.global_class
    }

    /// Returns the static `free(long)` method id symbol.
    pub fn free_method(&self) -> &Identifier {
        &self.free_method
    }

    /// Returns the static `clone(long)` method id symbol.
    pub fn clone_method(&self) -> &Identifier {
        &self.clone_method
    }

    /// Returns the load hook called from `JNI_OnLoad`.
    pub fn load(&self) -> &Identifier {
        &self.load
    }

    /// Returns the unload hook called from `JNI_OnUnload`.
    pub fn unload(&self) -> &Identifier {
        &self.unload
    }

    /// Returns the C callback vtable type.
    pub fn vtable_type(&self) -> &Identifier {
        &self.vtable_type
    }

    /// Returns the static C vtable instance symbol.
    pub fn vtable(&self) -> &Identifier {
        &self.vtable
    }

    /// Returns the C callback registration function.
    pub fn register(&self) -> &Identifier {
        &self.register
    }

    /// Returns the C vtable `free` slot implementation.
    pub fn free(&self) -> &Identifier {
        &self.free
    }

    /// Returns the C vtable `clone` slot implementation.
    pub fn clone_callback(&self) -> &Identifier {
        &self.clone
    }

    /// Returns registered callback method slots.
    pub fn methods(&self) -> &[CallbackMethod] {
        &self.methods
    }

    /// Returns native methods that call Rust-owned callback handles.
    pub fn handle_methods(&self) -> &[CallbackHandleMethod] {
        &self.handle_methods
    }
}
