//! Typed ergonomic C rendering for fallible free functions.

use boltffi_binding::{EnumDecl, ErrorChannel, ErrorPlacement, FunctionDecl, Native, TypeRef};

use crate::{
    bridge::c::{self, Type, TypeFragment},
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::{prefix::PackagePrefix, wrapper};

pub fn preamble() -> &'static str {
    ""
}

pub fn render_function(
    decl: &FunctionDecl<Native>,
    bridge: &c::CBridgeContract,
    context: &RenderContext<Native>,
    wrapper_name: &str,
) -> Result<Option<Emitted>> {
    let ErrorChannel::Encoded {
        placement: ErrorPlacement::ReturnSlot,
        ty,
        ..
    } = decl.callable().error().channel()
    else {
        return Ok(None);
    };
    let prefix = PackagePrefix::from_context(context);
    let (error_ty, error_comment, string_error) = match ty {
        TypeRef::Enum(error_id) => {
            let Some(EnumDecl::CStyle(error_decl)) = context.enumeration(*error_id) else {
                return Err(Error::UnsupportedTarget {
                    target: "c",
                    shape: "fallible function error enum is not C-style",
                });
            };
            (
                prefix.type_name(&Name::new(error_decl.name()).r#type()),
                "",
                false,
            )
        }
        TypeRef::String => (
            "FfiString".to_owned(),
            "/* On error, data.error is caller-owned UTF-8. FfiString is NOT NUL-terminated; use ptr and len, then call boltffi_free_string. */\n",
            true,
        ),
        _ => {
            return Err(Error::UnsupportedTarget {
                target: "c",
                shape: "fallible function error must be String or an exported C-style enum",
            });
        }
    };
    let abi = wrapper::find_abi(bridge, decl.symbol())?;
    let value_name = wrapper::local_name(abi, "value");
    let encoded_error_name = wrapper::local_name(abi, "encoded_error");
    let result_name = wrapper::local_name(abi, "result");
    let error_raw_name = wrapper::local_name(abi, "error_raw");
    let index_name = wrapper::local_name(abi, "index");
    let result_ty = format!(
        "{}Result",
        prefix.type_name(&Name::new(decl.name()).r#type())
    );
    let success = abi
        .params()
        .iter()
        .find(|p| p.name() == "return_out")
        .ok_or(Error::UnsupportedTarget {
            target: "c",
            shape: "fallible C facade requires an out-pointer success value",
        })?;
    let Type::MutPointer(success_ty) = success.ty() else {
        return Err(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "fallible C function return_out is not a pointer",
        });
    };
    let success_ty = TypeFragment::anonymous(success_ty)?.to_string();
    let params = abi
        .params()
        .iter()
        .filter(|p| p.name() != "return_out")
        .map(|p| TypeFragment::declaration(p.ty(), p.name()).map(|v| v.to_string()))
        .collect::<Result<Vec<_>>>()?;
    let params = if params.is_empty() {
        "void".to_owned()
    } else {
        params.join(", ")
    };
    let args = abi
        .params()
        .iter()
        .map(|p| {
            if p.name() == "return_out" {
                format!("&{value_name}")
            } else {
                p.name().to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let error_decode = if string_error {
        format!("    {result_name}.data.error = boltffi_buf_into_string({encoded_error_name});\n")
    } else {
        format!(
            r#"    uint64_t {error_raw_name} = 0;
    if ({encoded_error_name}.len == sizeof({result_name}.data.error)) {{
        for (uintptr_t {index_name} = 0; {index_name} < {encoded_error_name}.len; ++{index_name}) {{
            {error_raw_name} |= ((uint64_t){encoded_error_name}.ptr[{index_name}]) << ({index_name} * 8);
        }}
    }}
    {result_name}.data.error = ({error_ty}){error_raw_name};
    boltffi_free_buf({encoded_error_name});
"#,
        )
    };
    let body = format!(
        r#"{error_comment}typedef struct {{
    bool ok;
    union {{
        {success_ty} value;
        {error_ty} error;
    }} data;
}} {result_ty};
static inline {result_ty} {wrapper_name}({params}) {{
    {success_ty} {value_name};
    FfiBuf_u8 {encoded_error_name} = {}({args});
    {result_ty} {result_name};
    if ({encoded_error_name}.len == 0) {{
        {result_name}.ok = true;
        {result_name}.data.value = {value_name};
        return {result_name};
    }}
    {result_name}.ok = false;
{error_decode}    return {result_name};
}}
"#,
        abi.name()
    );
    Ok(Some(Emitted::primary(body)))
}
