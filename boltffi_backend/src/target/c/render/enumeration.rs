//! Ergonomic package-prefixed C-style enum callables.
use super::{callable, prefix::PackagePrefix, wrapper};
use crate::{
    bridge::c,
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{EnumDecl, Native};
pub fn render(
    decl: &EnumDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let EnumDecl::CStyle(e) = decl else {
        return Err(Error::UnsupportedTarget {
            target: "c",
            shape: "data enum",
        });
    };
    let prefix = PackagePrefix::from_context(context);
    let owner = Name::new(e.name()).member();
    let ty = prefix.type_name(&Name::new(e.name()).r#type());
    let mut out = Emitted::primary("");
    for init in e.initializers() {
        let abi = wrapper::find_abi(bridge, init.symbol())?;
        let name = prefix.member(&format!("{}_{}", owner, Name::new(init.name()).member()));
        out.append(callable::render(
            abi,
            init.callable(),
            &name,
            &format!("{}{}", ty, Name::new(init.name()).r#type()),
            callable::Receiver::None,
            context,
        )?)
    }
    for method in e.methods() {
        let abi = wrapper::find_abi(bridge, method.target())?;
        let name = prefix.member(&format!("{}_{}", owner, Name::new(method.name()).member()));
        out.append(callable::render(
            abi,
            method.callable(),
            &name,
            &format!("{}{}", ty, Name::new(method.name()).r#type()),
            callable::Receiver::DirectRecord {
                c_type: &ty,
                receive: method.callable().receiver().expect("enum receiver"),
            },
            context,
        )?)
    }
    Ok(out)
}
