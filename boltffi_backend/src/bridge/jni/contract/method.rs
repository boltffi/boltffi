//! Native method contracts.
//!
//! A native method is the JVM-facing entry point for one callable in the lower C
//! bridge. It has a `Java_*` symbol, a JNI signature, Java parameters, a
//! Java-visible return shape, and a call into the C bridge function that actually
//! talks to Rust.
//!
//! This module keeps those pieces together so method templates render from one
//! contract instead of stitching names, parameters, and return handling from
//! separate places.

use boltffi_binding::SymbolId;

use crate::{
    bridge::{
        c::{self, ArgumentList, Expression, Identifier},
        jni::{ClosureRegistration, JniSymbolName, JvmClassPath, NativeParameter, NativeReturn},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Native method exported to the JVM by a generated JNI source file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeMethod {
    source_symbol: SymbolId,
    c_function: c::Function,
    symbol: JniSymbolName,
    returns: NativeReturn,
    parameters: Vec<NativeParameter>,
}

impl NativeMethod {
    /// Creates a JNI native method from a C function declaration.
    pub fn new(
        class: &JvmClassPath,
        function: &c::Function,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        let source_symbol = function
            .source_symbol()
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "JNI native method has no source symbol",
            })?;
        Ok(Self {
            source_symbol,
            symbol: JniSymbolName::native_method(class, function.name())?,
            returns: NativeReturn::from_c_function(function)?,
            parameters: NativeParameter::from_c_function(function, callbacks, closures)?,
            c_function: function.clone(),
        })
    }

    /// Returns the source native symbol id.
    pub const fn source_symbol(&self) -> SymbolId {
        self.source_symbol
    }

    /// Returns the C bridge function this method calls.
    pub fn c_function(&self) -> &c::Function {
        &self.c_function
    }

    /// Returns the JNI exported C symbol.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the JNI return type.
    pub fn returns(&self) -> &NativeReturn {
        &self.returns
    }

    /// Returns parameters after `JNIEnv*` and `jclass`.
    pub fn parameters(&self) -> &[NativeParameter] {
        &self.parameters
    }

    /// Returns whether this method returns no value.
    pub fn returns_void(&self) -> bool {
        matches!(&self.returns, NativeReturn::Void)
    }

    /// Returns whether this method needs an explicit `jboolean` cast.
    pub fn returns_boolean(&self) -> bool {
        self.returns.is_boolean()
    }

    /// Returns whether this method returns an owned byte buffer.
    pub fn returns_bytes(&self) -> bool {
        self.returns.is_bytes()
    }

    /// Returns whether this method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        self.returns.is_record()
    }

    /// Returns whether this method returns a callback handle token.
    pub fn returns_callback(&self) -> bool {
        self.returns.is_callback()
    }

    /// Returns whether this method checks a returned `FfiStatus`.
    pub fn checks_status(&self) -> bool {
        matches!(
            &self.returns,
            NativeReturn::Status | NativeReturn::StatusWriteback(_) | NativeReturn::StatusValue(_)
        )
    }

    /// Returns whether this method checks an encoded error buffer.
    pub fn checks_error_buffer(&self) -> bool {
        self.returns.checks_error_buffer()
    }

    /// Returns whether this method passes bridge-owned status storage.
    pub fn checks_completion_status(&self) -> bool {
        self.c_function
            .parameter_groups()
            .iter()
            .any(|group| matches!(group, c::ParameterGroup::CompletionStatusOut(_)))
    }

    /// Returns the success value written through `return_out`.
    pub fn success_out(&self) -> Option<&crate::bridge::jni::SuccessOutReturn> {
        self.returns.success_out()
    }

    /// Returns the C bridge arguments passed by this native method.
    pub fn arguments(&self) -> Result<ArgumentList> {
        let mut parameters = self.parameters.iter();
        self.c_function
            .parameter_groups()
            .iter()
            .map(|group| match group {
                c::ParameterGroup::SuccessOut(_) => {
                    self.success_out().map(|value| vec![value.argument()]).ok_or(
                        Error::BrokenBridgeContract {
                            bridge: JNI_BRIDGE,
                            invariant: "success out-pointer has no matching JNI return value",
                        },
                    )
                }
                c::ParameterGroup::CompletionStatusOut(_) => {
                    Ok(vec![Expression::address_of(Expression::identifier(
                        Identifier::parse("__boltffi_status")?,
                    ))])
                }
                c::ParameterGroup::CallbackCompletion(_)
                | c::ParameterGroup::ClosureReturn(_) => Err(Error::BrokenBridgeContract {
                    bridge: JNI_BRIDGE,
                    invariant: "non-native parameter group cannot appear on a JNI native method",
                }),
                _ => parameters
                    .next()
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: "JNI native method is missing a parameter for a C group",
                    })
                    .and_then(NativeParameter::c_arguments),
            })
            .collect::<Result<Vec<_>>>()
            .map(|arguments| ArgumentList::from_iter(arguments.into_iter().flatten()))
    }
}
