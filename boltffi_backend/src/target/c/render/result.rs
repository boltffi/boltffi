//! Typed ergonomic C rendering for fallible free functions.

use boltffi_binding::{
    EnumDecl, ErrorChannel, ErrorPlacement, FunctionDecl, Native, ReturnPlan, TypeRef,
};

use crate::{
    bridge::c::{self, Type, TypeFragment},
    core::{Emitted, Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::{callable, prefix::PackagePrefix, surface, wrapper};

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
    let mut args = abi
        .params()
        .iter()
        .map(|p| p.name().to_owned())
        .collect::<Vec<_>>();
    let encoded_success = match decl.callable().returns().plan() {
        ReturnPlan::EncodedViaOutPointer { ty, .. } => Some(ty.clone()),
        _ => None,
    };
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
    // The union's success member holds the semantic value, not the raw ABI
    // transport: an encoded success (string, bytes, record, sequence) is
    // decoded out of its wire buffer before it reaches the caller.
    let value_member = match &encoded_success {
        Some(ty) => surface::value_type(ty, context, surface::ValueUse::Return)?,
        None => success_ty.clone(),
    };
    let (raw_decl, success_ok, return_out_arg) = match &encoded_success {
        Some(ty) => {
            let raw_name = wrapper::local_name(abi, "raw");
            let decode = callable::decode_owned(
                ty,
                &raw_name,
                &format!("{result_name}.data.value"),
                context,
                false,
                &mut |stem| wrapper::local_name(abi, stem),
            )?;
            (
                format!("    FfiBuf_u8 {raw_name};\n"),
                decode,
                format!("&{raw_name}"),
            )
        }
        None => (
            format!("    {success_ty} {value_name};\n"),
            format!("    {result_name}.data.value = {value_name};\n"),
            format!("&{value_name}"),
        ),
    };
    *args
        .iter_mut()
        .find(|arg| *arg == "return_out")
        .expect("return_out argument") = return_out_arg;
    let args = args.join(", ");
    let raw_free = encoded_success
        .as_ref()
        .map(|_| {
            format!(
                "    boltffi_free_buf({});\n",
                wrapper::local_name(abi, "raw")
            )
        })
        .unwrap_or_default();
    let body = format!(
        r#"{error_comment}typedef struct {{
    bool ok;
    union {{
        {value_member} value;
        {error_ty} error;
    }} data;
}} {result_ty};
static inline {result_ty} {wrapper_name}({params}) {{
{raw_decl}    FfiBuf_u8 {encoded_error_name} = {}({args});
    {result_ty} {result_name};
    if ({encoded_error_name}.len == 0) {{
        {result_name}.ok = true;
{success_ok}        return {result_name};
    }}
    {result_name}.ok = false;
{error_decode}{raw_free}    return {result_name};
}}
"#,
        abi.name()
    );
    Ok(Some(Emitted::primary(body)))
}
