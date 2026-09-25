//! C parameters accepted by generated callback vtable slots.
//!
//! JNI dispatch is wrapped around a C callback ABI. Rust still calls a function
//! pointer whose parameter list must exactly match the lower C bridge contract.
//! The JVM bridge may group those parameters into Java arguments, but the C
//! function signature itself cannot be approximated.
//!
//! This module keeps each callback slot parameter as a typed C declaration. The
//! callback templates use it when printing the vtable function signature and
//! when passing raw C values into grouped JVM setup.

use crate::{
    bridge::c::{self, Identifier, Statement, TypeFragment},
    core::Result,
};

/// One C ABI parameter accepted by a generated callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCParameter {
    name: Identifier,
    ty: TypeFragment,
    declaration: Statement,
    class_release: Option<Identifier>,
}

impl CallbackCParameter {
    pub(crate) fn class_release(&self) -> Option<&Identifier> {
        self.class_release.as_ref()
    }

    /// Returns the C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C parameter type.
    pub fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    /// Returns this parameter as a C declaration.
    pub fn declaration(&self) -> &Statement {
        &self.declaration
    }

    pub(in crate::bridge::jni::contract::callback) fn from_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(parameter.name())?,
            ty: TypeFragment::anonymous(parameter.ty())?,
            declaration: TypeFragment::declaration(parameter.ty(), parameter.name())?,
            class_release: parameter.class_release().cloned(),
        })
    }
}
