//! Direct-record parameters for JNI native methods.
//!
//! Java passes direct records as fixed-size byte arrays. The C bridge expects
//! the concrete record ABI type, so the generated method copies the Java bytes
//! into native storage before calling Rust. Mutable record parameters need a
//! second native local and a byte-array writeback after the call.
//!
//! This module owns that parameter contract: input byte array, native record
//! value, and optional writeback. Templates receive those facts directly instead
//! of deciding record mutability from the source type.

use crate::{
    bridge::c::{self, Expression, Identifier},
    core::{Error, Result},
};

use super::{RecordValue, RecordWriteback};

const JNI_BRIDGE: &str = "jni";

/// JNI parameter carrying one direct record value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RecordParameter {
    name: Identifier,
    value: RecordValue,
    local: Identifier,
    writeback: Option<RecordWriteback>,
}

impl RecordParameter {
    /// Returns the generated JNI byte-array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C typedef used by the C bridge.
    pub fn c_type(&self) -> &Identifier {
        self.value.c_type()
    }

    /// Returns the local C record value passed to the C bridge.
    pub fn local(&self) -> &Identifier {
        &self.local
    }

    /// Returns the expression passed to the C bridge function.
    pub fn c_argument(&self) -> Expression {
        self.value.c_argument(self.local.clone())
    }

    /// Returns the mutation writeback contract when the C bridge expects one.
    pub fn writeback(&self) -> Option<&RecordWriteback> {
        self.writeback.as_ref()
    }

    /// Creates a record parameter from one direct-record C ABI parameter.
    pub fn from_c_parameter(parameter: &c::Parameter) -> Result<Option<Self>> {
        let Some(value) = RecordValue::from_c_type(parameter.ty()) else {
            return Ok(None);
        };
        let name = Identifier::escape(parameter.name())?;
        Ok(Some(Self {
            local: Identifier::parse(format!("__boltffi_{}_value", name.as_str()))?,
            name,
            value,
            writeback: None,
        }))
    }

    /// Creates a record parameter from one direct-record C ABI writeback group.
    pub fn from_c_writeback(
        writeback: &c::DirectWritebackParameter,
        function: &c::Function,
    ) -> Result<Self> {
        let input = function.parameter(writeback.input());
        let Some(value) = RecordValue::from_c_type(input.ty()) else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "direct writeback input is not a direct record",
            });
        };
        let name = Identifier::escape(writeback.name())?;
        Ok(Self {
            local: Identifier::parse(format!("__boltffi_{}_value", name.as_str()))?,
            name,
            value,
            writeback: Some(RecordWriteback::from_c_parameter(
                function.parameter(writeback.output()),
            )?),
        })
    }
}
