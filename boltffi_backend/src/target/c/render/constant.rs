//! Ergonomic C wrappers for exported constants.
//!
//! Scalar inline constants are emitted as `#define` literals; accessor-backed
//! constants (`boltffi_const_*`) are re-exposed as a `static inline` wrapper.
//! Macro `#define`s are global to a translation unit, so every constant is
//! prefixed with the library name to avoid colliding with macros and symbols
//! from other headers.

use boltffi_binding::{ConstantDecl, ConstantValueDecl, DefaultValue, Native};

use crate::{
    bridge::c::{self, Identifier},
    core::{Emitted, RenderContext, Result},
    target::c::name_style::Name,
};

use super::{prefix::PackagePrefix, wrapper};

/// Renders one constant's ergonomic surface.
pub fn render(
    decl: &ConstantDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let prefix = PackagePrefix::from_context(context);
    match decl.value() {
        ConstantValueDecl::Inline { value, .. } => {
            let constant = prefix.constant(&Name::new(decl.name()).constant());
            let literal = render_default(value);
            Ok(Emitted::primary(format!("#define {constant} {literal}\n")))
        }
        ConstantValueDecl::Accessor { symbol, .. } => {
            let abi = wrapper::find_abi(bridge, symbol)?;
            let name = prefix.member(&Name::new(decl.name()).member());
            let wrapper_name = Identifier::escape(name)?;
            wrapper::forward(abi, wrapper_name.as_str())
        }
        _ => Ok(Emitted::primary(String::new())),
    }
}

fn render_default(value: &DefaultValue) -> String {
    match value {
        DefaultValue::Bool(true) => "true".to_owned(),
        DefaultValue::Bool(false) => "false".to_owned(),
        DefaultValue::Integer(integer) => integer.get().to_string(),
        DefaultValue::Float(float) => float.to_f64().to_string(),
        DefaultValue::String(string) => format!("{string:?}"),
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
            ..
        } => {
            // The ABI spellings join owner and variant (e.g. `MODE_FAST`),
            // unchanged by the ergonomic prefix.
            format!(
                "{}_{}",
                Name::new(enum_name).constant(),
                Name::new(variant_name).constant()
            )
        }
        DefaultValue::Null => "0".to_owned(),
        _ => "0".to_owned(),
    }
}
