//! Shared emission for `#[data(impl)]`/class associated callables.

use boltffi_binding::{CanonicalName, ExportedMethodDecl, InitializerDecl, Native, NativeSymbol};

use crate::{
    bridge::c::{self, Identifier},
    core::{Emitted, Result},
};

use super::{prefix::PackagePrefix, wrapper};
use crate::target::c::name_style::Name;

/// Renders forwarding wrappers for an owner's initializers and methods.
pub fn render_associated(
    owner: &CanonicalName,
    bridge: &c::CBridgeContract,
    initializers: &[InitializerDecl<Native>],
    methods: &[ExportedMethodDecl<Native, NativeSymbol>],
    prefix: &PackagePrefix,
) -> Result<Vec<Emitted>> {
    let owner_member = Name::new(owner).member();
    let mut emitted = Vec::new();
    for initializer in initializers {
        let abi = wrapper::find_abi(bridge, initializer.symbol())?;
        let init = Name::new(initializer.name()).member();
        let wrapper_name = Identifier::escape(prefix.member(&format!("{owner_member}_{init}")))?;
        emitted.push(wrapper::forward(abi, wrapper_name.as_str())?);
    }
    for method in methods {
        let abi = wrapper::find_abi(bridge, method.target())?;
        let method_name = Name::new(method.name()).member();
        let wrapper_name =
            Identifier::escape(prefix.member(&format!("{owner_member}_{method_name}")))?;
        emitted.push(wrapper::forward(abi, wrapper_name.as_str())?);
    }
    Ok(emitted)
}
