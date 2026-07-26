//! Bridge renderers and bridge stack composition.
//!
//! A bridge converts classified bindings into the ABI contract a host
//! language renderer consumes. Base bridges consume
//! [`boltffi_binding::Bindings`] directly; layered bridges consume the
//! contract produced by the layer below them. Bridge stack composition lives
//! in [`crate::core::target`].

use boltffi_binding::{Bindings, Surface};

use crate::core::{BridgeContract, GeneratedOutput, Result, contract::sealed};

/// Backend for one bridge layer.
#[allow(private_bounds)]
pub trait BridgeBackend: sealed::BridgeBackend {
    /// Binding surface this bridge layer serves.
    type Surface: Surface;
    /// Input value consumed by this bridge layer.
    type Input;
    /// Contract produced for layers or hosts above this bridge.
    type Contract: BridgeContract<Surface = Self::Surface>;
    /// Builds the bridge contract from the input value.
    fn build_contract(&self, input: &Self::Input) -> Result<Self::Contract>;

    /// Renders files owned by this bridge layer.
    fn render_bridge(
        &self,
        input: &Self::Input,
        contract: &Self::Contract,
    ) -> Result<GeneratedOutput>;
}

/// A fully composed bridge stack.
pub trait BridgeStack: sealed::BridgeStack {
    /// Binding surface this bridge stack serves.
    type Surface: Surface;
    /// Topmost bridge contract this stack exposes to a host.
    type Contract: BridgeContract<Surface = Self::Surface>;

    /// Builds the bridge stack for a binding contract.
    fn build(&self, bindings: &Bindings<Self::Surface>) -> Result<BridgeOutput<Self::Contract>>;
}

/// Output of a bridge stack build.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BridgeOutput<C> {
    contract: C,
    output: GeneratedOutput,
}

impl<C> BridgeOutput<C> {
    /// Creates bridge stack output.
    pub fn new(contract: C, output: GeneratedOutput) -> Self {
        Self { contract, output }
    }

    /// Returns the produced bridge contract.
    pub const fn contract(&self) -> &C {
        &self.contract
    }

    /// Returns files emitted by bridge layers.
    pub const fn output(&self) -> &GeneratedOutput {
        &self.output
    }

    /// Splits this output into contract and rendered fragments.
    pub fn into_parts(self) -> (C, GeneratedOutput) {
        (self.contract, self.output)
    }
}
