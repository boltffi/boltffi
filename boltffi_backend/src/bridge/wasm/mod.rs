use boltffi_binding::{Bindings, Wasm32};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, GeneratedOutput, Result, bridge,
    contract::sealed,
};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct WasmBridge;

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct WasmBridgeContract {
    capabilities: BridgeCapabilities,
}

impl WasmBridgeContract {
    fn new() -> Self {
        Self {
            capabilities: BridgeCapabilities::new().stable(BridgeCapability::Wasm),
        }
    }
}

impl bridge::BridgeBackend for WasmBridge {
    type Surface = Wasm32;
    type Input = Bindings<Wasm32>;
    type Contract = WasmBridgeContract;

    fn build_contract(&self, _input: &Self::Input) -> Result<Self::Contract> {
        Ok(WasmBridgeContract::new())
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        _contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::empty())
    }
}

impl sealed::BridgeBackend for WasmBridge {}

impl BridgeContract for WasmBridgeContract {
    type Surface = Wasm32;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for WasmBridgeContract {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Wasm32, lower};

    use crate::core::{BridgeCapability, BridgeContract, bridge::BridgeStack};

    use super::WasmBridge;

    fn bindings() -> Bindings<Wasm32> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Wasm32>(&source).expect("source lowers")
    }

    #[test]
    fn preserves_the_wasm_surface_without_emitting_bridge_files() {
        let output = WasmBridge.build(&bindings()).expect("bridge builds");

        assert!(output.output().files().is_empty());
        assert!(
            output
                .contract()
                .capabilities()
                .status(BridgeCapability::Wasm)
                .is_stable()
        );
    }
}
