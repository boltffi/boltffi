//! Ergonomic owned class handles and semantic sync callables.
use super::{callable, prefix::PackagePrefix, wrapper};
use crate::{
    bridge::c,
    core::{Emitted, RenderContext, Result},
    target::c::name_style::Name,
};
use boltffi_binding::{ClassDecl, Native};
pub fn render(
    decl: &ClassDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Emitted> {
    let prefix = PackagePrefix::from_context(context);
    let member = Name::new(decl.name()).member();
    let ty = prefix.type_name(&Name::new(decl.name()).r#type());
    let mut out = Emitted::primary("");
    for init in decl.initializers() {
        let abi = wrapper::find_abi(bridge, init.symbol())?;
        let name = prefix.member(&format!("{}_{}", member, Name::new(init.name()).member()));
        out.append(callable::render(
            abi,
            init.callable(),
            &name,
            &format!("{}{}", ty, Name::new(init.name()).r#type()),
            callable::Receiver::None,
            context,
        )?)
    }
    for method in decl.methods() {
        let abi = wrapper::find_abi(bridge, method.target())?;
        let name = prefix.member(&format!("{}_{}", member, Name::new(method.name()).member()));
        let recv = match method.callable().receiver() {
            Some(r) => callable::Receiver::Class {
                c_type: &ty,
                receive: r,
            },
            None => callable::Receiver::None,
        };
        out.append(callable::render(
            abi,
            method.callable(),
            &name,
            &format!("{}{}", ty, Name::new(method.name()).r#type()),
            recv,
            context,
        )?)
    }
    let release = wrapper::find_abi(bridge, decl.release())?;
    let free = prefix.member(&format!("{}_free", member));
    out.append(Emitted::primary(format!("static inline void {free}({ty} *value) {{ if (value == NULL || value->_boltffi_handle == 0) return; {}(value->_boltffi_handle); value->_boltffi_handle=0; }}\n",release.name())));
    Ok(out)
}
