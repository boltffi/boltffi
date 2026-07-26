//! Direct stream batch method contract.
//!
//! Most stream protocol methods can render as ordinary JNI native methods.
//! Direct batches are different: the C bridge fills a native item buffer and the
//! JNI bridge copies the used bytes into a Java byte array. That needs extra
//! source fields and a dedicated method shape.
//!
//! This module records that direct-batch method from the C stream protocol. It
//! keeps the buffer allocation, C call, and Java byte-array return tied together
//! before templates render the stream helper.

use boltffi_binding::SymbolId;

use crate::{
    bridge::{
        c::{self, TypeFragment},
        jni::{JniSymbolName, JvmClassPath},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// JNI method that returns a direct stream batch as a byte array.
pub struct DirectStreamBatchMethod {
    source_symbol: SymbolId,
    symbol: JniSymbolName,
    c_function: c::Function,
    subscription_type: TypeFragment,
    item_type: TypeFragment,
    item_size: u64,
}

impl DirectStreamBatchMethod {
    /// Creates a JNI direct-batch method from a C stream batch function.
    pub fn from_c_batch(class: &JvmClassPath, batch: &c::DirectStreamBatch) -> Result<Self> {
        let source_symbol =
            batch
                .function()
                .source_symbol()
                .ok_or(Error::BrokenBridgeContract {
                    bridge: JNI_BRIDGE,
                    invariant: "JNI direct stream batch has no source symbol",
                })?;
        let subscription =
            batch
                .function()
                .params()
                .first()
                .ok_or(Error::BrokenBridgeContract {
                    bridge: JNI_BRIDGE,
                    invariant: "direct stream batch function is missing subscription parameter",
                })?;
        Ok(Self {
            source_symbol,
            symbol: JniSymbolName::native_method(class, batch.function().name())?,
            c_function: batch.function().clone(),
            subscription_type: TypeFragment::anonymous(subscription.ty())?,
            item_type: TypeFragment::anonymous(batch.item())?,
            item_size: batch.item_size().get(),
        })
    }

    /// Returns the source native symbol id.
    pub const fn source_symbol(&self) -> SymbolId {
        self.source_symbol
    }

    /// Returns the JNI export symbol.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the C stream batch function.
    pub fn c_function(&self) -> &c::Function {
        &self.c_function
    }

    /// Returns the C subscription handle type.
    pub fn subscription_type(&self) -> &TypeFragment {
        &self.subscription_type
    }

    /// Returns the C type stored in the native batch buffer.
    pub fn item_type(&self) -> &TypeFragment {
        &self.item_type
    }

    /// Returns the byte size of one direct stream item.
    pub const fn item_size(&self) -> u64 {
        self.item_size
    }
}
