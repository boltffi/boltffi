use boltffi_binding::{Bindings, Native};

use crate::core::{
    BridgeCapabilities, BridgeContract, GeneratedOutput, Result, bridge, contract::sealed,
};

/// No-op bridge used while KMP target emission is moving into the IR backend.
///
/// Later milestones can replace this bridge contract with the concrete ABI
/// model needed for shared JNI and native output. Keeping it explicit lets
/// [`crate::Target`] exercise the same typed host/bridge composition path as
/// the Python backend without implying Apple output ownership yet.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpBridge;

/// Bridge contract consumed by the KMP host.
///
/// The contract advertises no bridge capabilities because the current KMP
/// emitter owns only the JVM/Android project surface; callable JNI delegation
/// remains fail-closed until JVM/Android parity is rebuilt around the shared
/// bridge model.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpBridgeContract {
    capabilities: BridgeCapabilities,
}

impl KmpBridgeContract {
    fn empty() -> Self {
        Self {
            capabilities: BridgeCapabilities::new(),
        }
    }
}

impl bridge::BridgeBackend for KmpBridge {
    type Surface = Native;
    type Input = Bindings<Native>;
    type Contract = KmpBridgeContract;

    fn build_contract(&self, _input: &Self::Input) -> Result<Self::Contract> {
        Ok(KmpBridgeContract::empty())
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        _contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::empty())
    }
}

impl sealed::BridgeBackend for KmpBridge {}

impl BridgeContract for KmpBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for KmpBridgeContract {}
