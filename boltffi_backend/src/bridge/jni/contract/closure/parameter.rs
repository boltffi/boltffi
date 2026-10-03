//! Closure parameters accepted by generated native methods.
//!
//! Java passes an inline closure to Rust as an opaque handle token. The C bridge
//! expects the native call function, context pointer, and release function for
//! the registered closure signature.
//!
//! This module records that expansion from one JNI parameter into the C bridge
//! arguments. Native method rendering can pass the prepared argument list
//! forward without knowing how closure handles are stored.

use boltffi_binding::HandlePresence;

use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::ClosureRegistration,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI parameter carrying a foreign closure handle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureParameter {
    name: Identifier,
    call: Expression,
    release: Expression,
}

impl ClosureParameter {
    /// Returns the generated JNI parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the C bridge arguments for this closure parameter.
    pub fn c_arguments(&self) -> Vec<Expression> {
        vec![
            self.call.clone(),
            Expression::cast(
                TypeFragment::new("void *"),
                Expression::identifier(self.name.clone()),
            ),
            self.release.clone(),
        ]
    }

    /// Creates a JNI closure parameter from one C closure parameter group.
    pub fn from_c_group(
        group: &c::ClosureParameter,
        registrations: &[ClosureRegistration],
    ) -> Result<Self> {
        let registration = registrations
            .iter()
            .find(|registration| registration.signature() == group.signature())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure parameter has no JNI closure registration",
            })?;
        let name = Identifier::escape(group.name())?;
        let (call, release) = match group.presence() {
            HandlePresence::Required => (
                Expression::identifier(registration.call().clone()),
                Expression::identifier(registration.release().clone()),
            ),
            HandlePresence::Nullable => (
                Expression::new(format!("({name} == 0 ? NULL : {})", registration.call())),
                Expression::new(format!("({name} == 0 ? NULL : {})", registration.release())),
            ),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: JNI_BRIDGE,
                    shape: "closure handle presence",
                });
            }
        };
        Ok(Self {
            name,
            call,
            release,
        })
    }
}
