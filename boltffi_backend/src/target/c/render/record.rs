//! Ergonomic package-prefixed record callables.

use super::{callable, prefix::PackagePrefix};
use crate::{
    bridge::c,
    core::{Emitted, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{Native, RecordDecl};

pub fn render(
    decl: &RecordDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let prefix = PackagePrefix::from_context(context);
    let owner_member = Name::new(decl.name()).member();
    let owner_type = prefix.type_name(decl.name());
    let mut emitted = Emitted::primary(default_initializer(decl, context)?);
    for init in decl.initializers() {
        let abi = bridge.function(init.symbol())?;
        let name = prefix.member(&format!(
            "{}_{}",
            owner_member,
            Name::new(init.name()).member()
        ));
        emitted.append(callable::render(
            abi,
            init.callable(),
            &name,
            &format!("{}{}", owner_type, Name::new(init.name()).r#type()),
            callable::Receiver::None,
            context,
        )?);
    }
    for method in decl.methods() {
        let abi = bridge.function(method.target())?;
        let name = prefix.member(&format!(
            "{}_{}",
            owner_member,
            Name::new(method.name()).member()
        ));
        let receiver = match (method.callable().receiver(), decl) {
            (None, _) => callable::Receiver::None,
            (Some(receive), RecordDecl::Encoded(record)) => callable::Receiver::Encoded {
                ty: boltffi_binding::TypeRef::Record(record.id()),
                codec: record.read(),
                receive,
            },
            (Some(receive), RecordDecl::Direct(_)) => callable::Receiver::DirectValue {
                c_type: &owner_type,
                receive,
            },
            _ => {
                return Err(crate::core::Error::UnsupportedTarget {
                    target: "c",
                    shape: "record declaration",
                });
            }
        };
        emitted.append(callable::render(
            abi,
            method.callable(),
            &name,
            &format!("{}{}", owner_type, Name::new(method.name()).r#type()),
            receiver,
            context,
        )?);
    }
    Ok(emitted)
}

fn default_initializer(
    decl: &RecordDecl<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let fields = match decl {
        RecordDecl::Direct(record) => record
            .fields()
            .iter()
            .map(|field| {
                (
                    field.key(),
                    boltffi_binding::TypeRef::Primitive(field.ty().primitive()),
                    field.meta(),
                )
            })
            .collect::<Vec<_>>(),
        RecordDecl::Encoded(record) => record
            .fields()
            .iter()
            .map(|field| (field.key(), field.ty().clone(), field.meta()))
            .collect::<Vec<_>>(),
        _ => return Ok(String::new()),
    };
    if !fields.iter().any(|(_, _, meta)| meta.default().is_some()) {
        return Ok(String::new());
    }
    let mut parameters = Vec::new();
    let mut initializers = Vec::new();
    fields
        .iter()
        .try_for_each(|(key, ty, meta)| -> Result<()> {
            let field = Name::field(key)?;
            let value = match meta.default() {
                Some(value) => super::default_value::render(
                    ty,
                    value,
                    super::surface::ValueUse::Param,
                    context,
                )?
                .to_string(),
                None => {
                    let field_type =
                        super::surface::value_type(ty, context, super::surface::ValueUse::Param)?;
                    parameters.push(format!("{field_type} {field}"));
                    field.to_string()
                }
            };
            initializers.push(format!(".{field} = {value}"));
            Ok(())
        })?;
    let record_type = super::surface::value_type(
        &boltffi_binding::TypeRef::Record(decl.id()),
        context,
        super::surface::ValueUse::Param,
    )?;
    let name = PackagePrefix::from_context(context)
        .member(&format!("{}_init", Name::new(decl.name()).member()));
    let parameters = if parameters.is_empty() {
        "void".to_owned()
    } else {
        parameters.join(", ")
    };
    let initializers = initializers.join(", ");
    Ok(format!(
        "static inline {record_type} {name}({parameters}) {{\nreturn ({record_type}){{{initializers}}};\n}}\n"
    ))
}
