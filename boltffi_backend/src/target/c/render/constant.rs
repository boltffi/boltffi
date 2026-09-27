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
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::{callable, prefix::PackagePrefix};

/// Renders one constant's ergonomic surface.
pub fn render(
    decl: &ConstantDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let prefix = PackagePrefix::from_context(context);
    let (owner_constant, owner_member) = match decl.owner() {
        Some(boltffi_binding::ConstantOwner::Record(id)) => {
            let record = context.record(id).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing constant owner record",
            })?;
            (
                Name::new(record.name()).constant(),
                Name::new(record.name()).member(),
            )
        }
        Some(boltffi_binding::ConstantOwner::Enum(id)) => {
            let enumeration = context.enumeration(id).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing constant owner enum",
            })?;
            (
                Name::new(enumeration.name()).constant(),
                Name::new(enumeration.name()).member(),
            )
        }
        Some(boltffi_binding::ConstantOwner::Class(id)) => {
            let class = context.class(id).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing constant owner class",
            })?;
            (
                Name::new(class.name()).constant(),
                Name::new(class.name()).member(),
            )
        }
        None => (String::new(), String::new()),
    };
    let join = |owner: &str, name: String| {
        if owner.is_empty() {
            name
        } else {
            format!("{owner}_{name}")
        }
    };
    let constant_name = Name::new(decl.name());
    match decl.value() {
        ConstantValueDecl::Inline { value, ty, .. } => {
            let constant = prefix.constant(&join(&owner_constant, constant_name.constant()));
            let literal = match value {
                DefaultValue::String(value) => {
                    c::Literal::byte_string(value.as_bytes()).to_string()
                }
                _ => super::default_value::render(
                    ty,
                    value,
                    super::surface::ValueUse::Return,
                    context,
                )?
                .to_string(),
            };
            Ok(Emitted::primary(format!("#define {constant} {literal}\n")))
        }
        ConstantValueDecl::Accessor {
            symbol,
            callable: accessor,
        } => {
            let abi = bridge.function(symbol)?;
            let name = prefix.member(&join(&owner_member, constant_name.member()));
            let wrapper_name = Identifier::escape(name)?;
            callable::render(
                abi,
                accessor,
                wrapper_name.as_str(),
                &format!("{}{}", owner_constant, constant_name.r#type()),
                callable::Receiver::None,
                context,
            )
        }
        _ => Ok(Emitted::primary(String::new())),
    }
}
