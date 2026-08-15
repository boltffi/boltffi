//! Ergonomic wrappers for free functions.
//!
//! A sync `#[export] fn` maps to one ABI symbol (`boltffi_function_*`). The host
//! layers a `static inline` wrapper under the source name so callers get a clean,
//! keyword-safe API that lines up with the ABI.

use boltffi_binding::{ErrorChannel, FunctionDecl, Native, TypeRef};

use crate::{
    bridge::c::{self, Identifier},
    core::{Emitted, Error, RenderContext, Result},
};

use super::{prefix::PackagePrefix, wrapper};
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
    let abi = wrapper::find_abi(bridge, decl.symbol())?;
    let prefix = PackagePrefix::from_context(context);
    let wrapper_name = Identifier::escape(prefix.member(&Name::new(decl.name()).member()))?;
    if matches!(
        decl.callable().error().channel(),
        ErrorChannel::Encoded {
            ty: TypeRef::Enum(_),
            ..
        }
    ) {
        return super::result::render_function(decl, bridge, context, wrapper_name.as_str())?
            .ok_or(Error::UnsupportedTarget {
                target: "c",
                shape: "C-style enum result",
            });
    }
    super::callable::render(
        abi,
        decl.callable(),
        wrapper_name.as_str(),
        &prefix.type_name(&Name::new(decl.name()).r#type()),
        super::callable::Receiver::None,
        context,
    )
}
