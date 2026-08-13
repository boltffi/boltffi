//! Shared `static inline` forwarding wrapper emission.

use crate::{
    bridge::c::{self, Type, TypeFragment},
    core::{Emitted, Error, Result},
};

/// Allocates a wrapper-local identifier that cannot shadow an ABI parameter.
pub fn local_name(function: &c::Function, stem: &str) -> String {
    let mut name = format!("boltffi_{stem}");
    while function
        .params()
        .iter()
        .any(|parameter| parameter.name() == name)
    {
        name.push('_');
    }
    name
}

/// Emits a `static inline` wrapper under `wrapper_name` that forwards to the
/// given ABI symbol with the same parameter list and return type.
pub fn forward(function: &c::Function, wrapper_name: &str) -> Result<Emitted> {
    let returns = TypeFragment::anonymous(function.returns())?.to_string();
    let params = function
        .params()
        .iter()
        .map(|parameter| {
            TypeFragment::declaration(parameter.ty(), parameter.name()).map(|s| s.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let params_text = if params.is_empty() {
        "void".to_owned()
    } else {
        params.join(", ")
    };
    let args = function
        .params()
        .iter()
        .map(|parameter| parameter.name().to_owned())
        .collect::<Vec<_>>()
        .join(", ");
    let mut body = format!("static inline {returns} {wrapper_name}({params_text}) {{\n");
    match function.returns() {
        Type::Void => {
            body.push_str(&format!("    {}({});\n", function.name(), args));
        }
        _ => {
            body.push_str(&format!("    return {}({});\n", function.name(), args));
        }
    }
    body.push_str("}\n");
    Ok(Emitted::primary(body))
}

/// Finds the ABI `Function` whose symbol name matches a source `NativeSymbol`.
pub fn find_abi<'a>(
    bridge: &'a c::CBridgeContract,
    symbol: &boltffi_binding::NativeSymbol,
) -> Result<&'a c::Function> {
    bridge
        .functions()
        .iter()
        .find(|function| function.name() == symbol.name().as_str())
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "missing ABI function for source callable",
        })
}
