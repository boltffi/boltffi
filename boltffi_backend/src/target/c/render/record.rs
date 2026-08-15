//! Ergonomic package-prefixed record callables.

use super::{callable, prefix::PackagePrefix, wrapper};
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
    let owner_type = prefix.type_name(&Name::new(decl.name()).r#type());
    let mut emitted = Emitted::primary("");
    for init in decl.initializers() {
        let abi = wrapper::find_abi(bridge, init.symbol())?;
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
        let abi = wrapper::find_abi(bridge, method.target())?;
        let name = prefix.member(&format!(
            "{}_{}",
            owner_member,
            Name::new(method.name()).member()
        ));
        let receiver = match decl {
            RecordDecl::Encoded(r) => callable::Receiver::EncodedRecord {
                c_type: &owner_type,
                id: r.id(),
                receive: method
                    .callable()
                    .receiver()
                    .expect("record method receiver"),
            },
            RecordDecl::Direct(_) => callable::Receiver::DirectRecord {
                c_type: &owner_type,
                receive: method
                    .callable()
                    .receiver()
                    .expect("record method receiver"),
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
