//! Ergonomic wrappers for free functions.
//!
//! A sync `#[export] fn` maps to one ABI symbol (`boltffi_function_*`). The host
//! layers a `static inline` wrapper under the source name so callers get a clean,
//! keyword-safe API that lines up with the ABI.

use boltffi_binding::{FunctionDecl, Native};

use crate::{
    bridge::c::{self, Identifier},
    core::{Emitted, Error, RenderContext, Result},
};

use super::prefix::PackagePrefix;
use crate::target::c::name_style::Name;

/// Renders one free function's ergonomic wrapper.
pub fn render(
    decl: &FunctionDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    if decl.callable().execution().uses_async_execution() {
        return Err(Error::UnsupportedTarget {
            target: "c",
            shape: "async free functions are out of scope",
        });
    }
    let abi = bridge.function(decl.symbol())?;
    let prefix = PackagePrefix::from_context(context);
    let wrapper_name = Identifier::escape(prefix.member(&Name::new(decl.name()).member()))?;
    super::callable::render(
        abi,
        decl.callable(),
        wrapper_name.as_str(),
        &prefix.type_name(decl.name()),
        super::callable::Receiver::None,
        context,
    )
}
