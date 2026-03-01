use crate::model::{AsyncIteratorMethod, CallbackTrait, Class, Enumeration, Function, Method, Module, Parameter, Primitive, ReturnType, StreamMethod, TraitMethod, Type};
use boltffi_ffi_rules::naming;
use std::collections::BTreeSet;

pub struct CHeaderGenerator;

struct CollectedFfiTypes {
    primitive_buf_types: Vec<String>,
    named_buf_types: Vec<String>,
    option_primitive_buf_types: Vec<String>,
    option_named_buf_types: Vec<String>,
    primitive_option_types: Vec<String>,
    named_option_types: Vec<String>,
}

impl CHeaderGenerator {
    pub fn generate(module: &Module) -> String {
        let prefix = naming::ffi_prefix();
        let mut out = String::new();

        out.push_str(&Self::generate_preamble(prefix));
        out.push_str(&Self::generate_forward_declarations(module));
        out.push_str(&Self::generate_async_types_if_needed(module, prefix));
        out.push_str(&Self::generate_stream_types_if_needed(module));
        out.push_str(&Self::generate_ffi_primitive_types(module));
        out.push_str(&Self::generate_enums(&module.enums));
        out.push_str(&Self::generate_ffi_named_buf_types(module));
        out.push_str(&Self::generate_ffi_named_option_types(module));
        out.push_str(&Self::generate_traits(&module.callback_traits, prefix, module));
        out.push_str(&Self::generate_functions(&module.functions, module));
        out.push_str(&Self::generate_classes(&module.classes, prefix, module));
        out.push_str(&Self::generate_free_functions(prefix));

        out
    }

    fn generate_forward_declarations(module: &Module) -> String {
        let names = Self::collect_forward_decl_struct_names(module);
        if names.is_empty() {
            return String::new();
        }

        let decls = names.into_iter().map(|name| format!("struct {};\n", name)).collect::<String>();
        format!("\n{}\n", decls)
    }

    fn collect_forward_decl_struct_names(module: &Module) -> BTreeSet<String> {
        let mut names = BTreeSet::new();

        module.classes.iter().map(|class| class.name.clone()).for_each(|name| {
            names.insert(name);
        });

        module.functions.iter().for_each(|func| {
            func.inputs
                .iter()
                .map(|param| &param.param_type)
                .chain(func.returns.ok_type())
                .chain(func.returns.err_type())
                .for_each(|ty| Self::collect_forward_decl_names_from_type(ty, &mut names));
        });

        module.classes.iter().for_each(|class| {
            class
                .constructors
                .iter()
                .flat_map(|ctor| ctor.inputs.iter().map(|param| &param.param_type))
                .for_each(|ty| Self::collect_forward_decl_names_from_type(ty, &mut names));

            class.methods.iter().for_each(|method| {
                method
                    .inputs
                    .iter()
                    .map(|param| &param.param_type)
                    .chain(method.returns.ok_type())
                    .chain(method.returns.err_type())
                    .for_each(|ty| Self::collect_forward_decl_names_from_type(ty, &mut names));
            });
        });

        module.callback_traits.iter().for_each(|callback| {
            callback.methods.iter().for_each(|method| {
                method
                    .inputs
                    .iter()
                    .map(|param| &param.param_type)
                    .chain(method.returns.ok_type())
                    .chain(method.returns.err_type())
                    .for_each(|ty| Self::collect_forward_decl_names_from_type(ty, &mut names));
            });
        });

        names
    }

    fn collect_forward_decl_names_from_type(ty: &Type, names: &mut BTreeSet<String>) {
        match ty {
            Type::Object(name) => {
                names.insert(name.clone());
            }
            Type::Slice(inner) | Type::MutSlice(inner) | Type::Vec(inner) | Type::Option(inner) => Self::collect_forward_decl_names_from_type(inner, names),
            Type::Result { ok, err } => {
                Self::collect_forward_decl_names_from_type(ok, names);
                Self::collect_forward_decl_names_from_type(err, names);
            }
            Type::Custom { repr, .. } => Self::collect_forward_decl_names_from_type(repr, names),
            Type::Primitive(_) | Type::String | Type::Bytes | Type::Builtin(_) | Type::Closure(_) | Type::Record(_) | Type::Enum(_) | Type::BoxedTrait(_) | Type::Void => {}
        }
    }

    fn generate_free_functions(prefix: &str) -> String {
        format!(
            "\nvoid {}_free_string(FfiString s);\n\
             void {}_free_buf_u8(FfiBuf_u8 buf);\n\
             FfiStatus {}_last_error_message(FfiString *out);\n\
             void {}_clear_last_error(void);\n",
            prefix, prefix, prefix, prefix
        )
    }

    fn generate_preamble(prefix: &str) -> String {
        format!(
            r#"#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdatomic.h>

	typedef struct {{ int32_t code; }} FfiStatus;
	typedef struct {{ uint8_t* ptr; size_t len; size_t cap; }} FfiString;
	typedef struct {{ uint8_t* ptr; size_t len; size_t cap; }} FfiBuf_u8;
	typedef struct {{ FfiString message; }} FfiError;
	typedef struct {{ uint64_t handle; const void* vtable; }} BoltFFICallbackHandle;

	static inline bool {prefix}_atomic_u8_cas(uint8_t* state, uint8_t expected, uint8_t desired) {{
	    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t*)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
	}}

static inline uint64_t {prefix}_atomic_u64_exchange(uint64_t* slot, uint64_t value) {{
    return atomic_exchange_explicit((_Atomic uint64_t*)slot, value, memory_order_acq_rel);
}}

static inline bool {prefix}_atomic_u64_cas(uint64_t* slot, uint64_t expected, uint64_t desired) {{
    return atomic_compare_exchange_strong_explicit((_Atomic uint64_t*)slot, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}}

static inline uint64_t {prefix}_atomic_u64_load(uint64_t* slot) {{
    return atomic_load_explicit((_Atomic uint64_t*)slot, memory_order_acquire);
}}

"#
        )
    }

    fn generate_async_types_if_needed(module: &Module, _prefix: &str) -> String {
        if !module.has_async() {
            return String::new();
        }

        "typedef const void* RustFutureHandle;\n\
         typedef void (*RustFutureContinuationCallback)(uint64_t callback_data, int8_t poll_result);\n\n"
            .to_string()
    }

    fn generate_stream_types_if_needed(module: &Module) -> String {
        if !module.has_streams() {
            return String::new();
        }

        "typedef void* SubscriptionHandle;\n\
         typedef void (*StreamContinuationCallback)(uint64_t callback_data, int8_t poll_result);\n\n"
            .to_string()
    }

    fn collect_ffi_types(module: &Module) -> CollectedFfiTypes {
        use std::collections::HashSet;
        let mut primitive_buf_types: HashSet<String> = HashSet::new();
        let mut named_buf_types: HashSet<String> = HashSet::new();
        let mut option_primitive_buf_types: HashSet<String> = HashSet::new();
        let mut option_named_buf_types: HashSet<String> = HashSet::new();
        let mut primitive_option_types: HashSet<String> = HashSet::new();
        let mut named_option_types: HashSet<String> = HashSet::new();

        fn insert_vec_buf_type(inner: &Type, primitive_buf: &mut HashSet<String>, named_buf: &mut HashSet<String>) {
            match inner {
                Type::Primitive(p) => {
                    primitive_buf.insert(p.rust_name().to_string());
                }
                Type::Record(name) | Type::Enum(name) => {
                    named_buf.insert(name.clone());
                }
                Type::String => {
                    named_buf.insert("FfiString".to_string());
                }
                _ => {}
            }
        }

        fn collect_from_type(
            ty: &Type,
            primitive_buf: &mut HashSet<String>,
            named_buf: &mut HashSet<String>,
            option_primitive_buf: &mut HashSet<String>,
            option_named_buf: &mut HashSet<String>,
            prim_opts: &mut HashSet<String>,
            named_opts: &mut HashSet<String>,
        ) {
            match ty {
                Type::Vec(inner) => {
                    insert_vec_buf_type(inner, primitive_buf, named_buf);
                }
                Type::Option(inner) => match inner.as_ref() {
                    Type::Vec(vec_inner) => {
                        insert_vec_buf_type(vec_inner, primitive_buf, named_buf);
                        insert_vec_buf_type(vec_inner, option_primitive_buf, option_named_buf);
                    }
                    Type::Primitive(p) => {
                        prim_opts.insert(p.rust_name().to_string());
                    }
                    Type::String => {
                        prim_opts.insert("FfiString".to_string());
                    }
                    Type::Record(name) | Type::Enum(name) => {
                        named_opts.insert(name.clone());
                    }
                    _ => {}
                },
                Type::Result { ok, .. } => {
                    if let Type::Vec(inner) = ok.as_ref() {
                        insert_vec_buf_type(inner, primitive_buf, named_buf);
                    }
                    if let Type::Option(inner) = ok.as_ref() {
                        match inner.as_ref() {
                            Type::Vec(vec_inner) => {
                                insert_vec_buf_type(vec_inner, primitive_buf, named_buf);
                                insert_vec_buf_type(vec_inner, option_primitive_buf, option_named_buf);
                            }
                            Type::Primitive(p) => {
                                prim_opts.insert(p.rust_name().to_string());
                            }
                            Type::String => {
                                prim_opts.insert("FfiString".to_string());
                            }
                            Type::Record(name) | Type::Enum(name) => {
                                named_opts.insert(name.clone());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        for func in &module.functions {
            if let Some(ty) = func.returns.ok_type() {
                collect_from_type(
                    ty,
                    &mut primitive_buf_types,
                    &mut named_buf_types,
                    &mut option_primitive_buf_types,
                    &mut option_named_buf_types,
                    &mut primitive_option_types,
                    &mut named_option_types,
                );
            }
        }

        for class in &module.classes {
            for method in &class.methods {
                if let Some(ty) = method.returns.ok_type() {
                    collect_from_type(
                        ty,
                        &mut primitive_buf_types,
                        &mut named_buf_types,
                        &mut option_primitive_buf_types,
                        &mut option_named_buf_types,
                        &mut primitive_option_types,
                        &mut named_option_types,
                    );
                }
            }
        }

        let mut primitive_buf_vec: Vec<_> = primitive_buf_types.into_iter().collect();
        let mut named_buf_vec: Vec<_> = named_buf_types.into_iter().collect();
        let mut option_primitive_buf_vec: Vec<_> = option_primitive_buf_types.into_iter().collect();
        let mut option_named_buf_vec: Vec<_> = option_named_buf_types.into_iter().collect();
        let mut prim_vec: Vec<_> = primitive_option_types.into_iter().collect();
        let mut named_vec: Vec<_> = named_option_types.into_iter().collect();

        primitive_buf_vec.sort();
        named_buf_vec.sort();
        option_primitive_buf_vec.sort();
        option_named_buf_vec.sort();
        prim_vec.sort();
        named_vec.sort();

        CollectedFfiTypes {
            primitive_buf_types: primitive_buf_vec,
            named_buf_types: named_buf_vec,
            option_primitive_buf_types: option_primitive_buf_vec,
            option_named_buf_types: option_named_buf_vec,
            primitive_option_types: prim_vec,
            named_option_types: named_vec,
        }
    }

    fn generate_ffi_primitive_types(module: &Module) -> String {
        let types = Self::collect_ffi_types(module);
        let mut out = String::new();

        for ty_name in &types.primitive_option_types {
            let c_type = Self::option_type_to_c_type(ty_name);
            out.push_str(&format!(
                "typedef struct FfiOption_{} {{ bool isSome; {} value; }} FfiOption_{};\n",
                ty_name, c_type, ty_name
            ));
        }

        if !out.is_empty() {
            out.push('\n');
        }

        out
    }

    fn generate_ffi_named_buf_types(_module: &Module) -> String {
        String::new()
    }

    fn generate_ffi_named_option_types(_module: &Module) -> String {
        String::new()
    }

    fn cbindgen_name_to_c_type(name: &str) -> &'static str {
        match name {
            "i8" => "int8_t",
            "i16" => "int16_t",
            "i32" => "int32_t",
            "i64" => "int64_t",
            "u8" => "uint8_t",
            "u16" => "uint16_t",
            "u32" => "uint32_t",
            "u64" => "uint64_t",
            "f32" => "float",
            "f64" => "double",
            "bool" => "bool",
            "usize" => "size_t",
            "isize" => "ptrdiff_t",
            _ => "void",
        }
    }

    fn option_type_to_c_type(name: &str) -> String {
        match name {
            "i8" => "int8_t".to_string(),
            "i16" => "int16_t".to_string(),
            "i32" => "int32_t".to_string(),
            "i64" => "int64_t".to_string(),
            "u8" => "uint8_t".to_string(),
            "u16" => "uint16_t".to_string(),
            "u32" => "uint32_t".to_string(),
            "u64" => "uint64_t".to_string(),
            "f32" => "float".to_string(),
            "f64" => "double".to_string(),
            "bool" => "bool".to_string(),
            "usize" => "size_t".to_string(),
            "isize" => "ptrdiff_t".to_string(),
            other => other.to_string(),
        }
    }

    fn generate_enums(enums: &[Enumeration]) -> String {
        enums.iter().filter(|e| !Self::is_internal_enum(&e.name)).map(Self::generate_enum).collect()
    }

    fn is_internal_enum(name: &str) -> bool {
        matches!(name, "StreamPollResult" | "ContinuationState" | "WaitResult" | "RustFuturePoll" | "SchedulerStateTag")
    }

    fn generate_enum(e: &Enumeration) -> String {
        if !e.is_c_style() {
            return String::new();
        }

        let hidden_name = format!("___{}", e.name);
        let mut out = format!("typedef int32_t {};\n", hidden_name);

        let mut next_value: i64 = 0;
        for variant in &e.variants {
            let value = variant.discriminant.unwrap_or(next_value);
            out.push_str(&format!("#define {}_{} {}\n", hidden_name, variant.name, value));
            next_value = value + 1;
        }
        out.push('\n');
        out
    }

    fn generate_traits(traits: &[CallbackTrait], prefix: &str, module: &Module) -> String {
        traits.iter().map(|t| Self::generate_trait(t, prefix, module)).collect()
    }

    fn generate_trait(t: &CallbackTrait, prefix: &str, module: &Module) -> String {
        let trait_name = &t.name;
        let vtable_name = format!("{}VTable", trait_name);
        let snake_name = naming::to_snake_case(trait_name);

        let mut vtable_fields = vec!["  void (*free)(uint64_t handle);".to_string(), "  uint64_t (*clone)(uint64_t handle);".to_string()];

        for method in &t.methods {
            vtable_fields.push(Self::generate_trait_method_field(method, module));
        }

        format!(
            "typedef struct {} {{\n{}\n}} {};\n\n\
             void {}_register_{}_vtable(const struct {} *vtable);\n\
             BoltFFICallbackHandle {}_create_{}_handle(uint64_t handle);\n\n",
            vtable_name,
            vtable_fields.join("\n"),
            vtable_name,
            prefix,
            snake_name,
            vtable_name,
            prefix,
            snake_name,
        )
    }

    fn generate_trait_method_field(method: &TraitMethod, module: &Module) -> String {
        let method_snake = naming::vtable_field_name(&method.name);
        let mut params = vec!["uint64_t handle".to_string()];

        for param in &method.inputs {
            Self::trait_param_to_c(&param.name, &param.param_type, module)
                .into_iter()
                .map(|(name, ty)| format!("{} {}", ty, name))
                .for_each(|decl| params.push(decl));
        }

        if method.is_async {
            let callback_return = Self::trait_callback_return_params(&method.returns);
            params.push(format!("void (*callback)(uint64_t{}, FfiStatus)", callback_return));
            params.push("uint64_t callback_data".to_string());
        } else {
            if let Some(ret_ty) = method.returns.ok_type() {
                Self::trait_method_out_params(ret_ty, &mut params);
            }
            params.push("FfiStatus *status".to_string());
        }

        format!("  void (*{})({});", method_snake.as_str(), params.join(", "))
    }

    fn trait_callback_return_params(returns: &ReturnType) -> String {
        match returns {
            ReturnType::Void => String::new(),
            ReturnType::Fallible { .. } => ", const uint8_t*, uintptr_t".to_string(),
            ReturnType::Value(ty) => match ty {
                Type::Primitive(_) => format!(", {}", Self::type_to_c(ty)),
                _ => ", const uint8_t*, uintptr_t".to_string(),
            },
        }
    }

    fn trait_method_out_params(ty: &Type, params: &mut Vec<String>) {
        match ty {
            Type::Primitive(_) => {
                params.push(format!("{} *out", Self::type_to_c(ty)));
            }
            _ => {
                params.push("uint8_t *out_ptr".to_string());
                params.push("uintptr_t *out_len".to_string());
            }
        }
    }

    fn trait_param_to_c(name: &str, ty: &Type, module: &Module) -> Vec<(String, String)> {
        let name = naming::escape_c_keyword(name);
        match ty {
            Type::Primitive(_) | Type::Object(_) | Type::BoxedTrait(_) => {
                vec![(name.to_string(), Self::type_to_c(ty))]
            }
            Type::Closure(signature) => {
                let params_c: Vec<String> = signature.params.iter().flat_map(Self::closure_param_to_c).collect();
                let params_str = params_c.join(", ");
                let ret_c = Self::closure_return_c_type(&signature.returns, module);
                let callback_type = if params_str.is_empty() {
                    format!("{} (*)(void*)", ret_c)
                } else {
                    format!("{} (*)(void*, {})", ret_c, params_str)
                };
                vec![(format!("{}_cb", name), callback_type), (format!("{}_ud", name), "void*".to_string())]
            }
            _other => {
                vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())]
            }
        }
    }

    fn generate_functions(functions: &[Function], module: &Module) -> String {
        functions.iter().map(|f| Self::generate_function(f, module)).collect()
    }

    fn generate_function(func: &Function, module: &Module) -> String {
        let ffi_name = naming::function_ffi_name(&func.name);
        let params = Self::build_params(&func.inputs, module);

        if func.is_async {
            Self::generate_async_function(ffi_name.as_str(), &params, &func.returns)
        } else {
            Self::generate_sync_function(ffi_name.as_str(), &params, &func.returns, module)
        }
    }

    fn generate_sync_function(ffi_name: &str, params: &[(String, String)], returns: &ReturnType, module: &Module) -> String {
        let params_str = Self::format_params(params);

        match returns {
            ReturnType::Void => format!("FfiStatus {}({});\n", ffi_name, params_str),
            ReturnType::Fallible { .. } => format!("FfiBuf_u8 {}({});\n", ffi_name, params_str),
            ReturnType::Value(ty) => {
                if Self::is_wire_encoded_type(ty, module) {
                    format!("FfiBuf_u8 {}({});\n", ffi_name, params_str)
                } else {
                    let ret_type = Self::type_to_c_return(ty);
                    format!("{} {}({});\n", ret_type, ffi_name, params_str)
                }
            }
        }
    }

    fn type_to_c_return(ty: &Type) -> String {
        match ty {
            Type::Enum(name) => format!("___{}", name),
            _ => Self::type_to_c(ty),
        }
    }

    fn is_wire_encoded_type(ty: &Type, _module: &Module) -> bool {
        match ty {
            Type::String | Type::Vec(_) | Type::Option(_) | Type::Builtin(_) | Type::Record(_) | Type::Enum(_) | Type::Custom { .. } => true,
            Type::Primitive(_) | Type::Void | Type::Object(_) => false,
            Type::Bytes => true,
            Type::Slice(_) | Type::MutSlice(_) => false,
            Type::Closure(_) | Type::BoxedTrait(_) => false,
            Type::Result { .. } => true,
        }
    }

    fn closure_return_c_type(ty: &Type, module: &Module) -> String {
        if ty.is_void() {
            "void".to_string()
        } else if Self::is_wire_encoded_type(ty, module) {
            "FfiBuf_u8".to_string()
        } else {
            Self::type_to_c_return(ty)
        }
    }

    fn generate_async_function(ffi_name: &str, params: &[(String, String)], returns: &ReturnType) -> String {
        let params_str = Self::format_params(params);
        let result_type = Self::async_result_type(returns);

        format!(
            "RustFutureHandle {}({});\n\
             void {}_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);\n\
             {} {}_complete(RustFutureHandle handle, FfiStatus* out_status);\n\
             void {}_cancel(RustFutureHandle handle);\n\
             void {}_free(RustFutureHandle handle);\n",
            ffi_name, params_str, ffi_name, result_type, ffi_name, ffi_name, ffi_name
        )
    }

    fn generate_vec_return_function(ffi_name: &str, params: &[(String, String)], inner: &Type) -> String {
        let params_str = Self::format_params(params);
        let buf_type = format!("FfiBuf_{}", Self::primitive_to_cbindgen_name(inner));
        format!("{} {}({});\n", buf_type, ffi_name, params_str)
    }

    fn generate_string_return_function(ffi_name: &str, params: &[(String, String)]) -> String {
        let mut new_params = params.to_vec();
        new_params.push(("out".to_string(), "FfiString *".to_string()));
        let params_str = Self::format_params(&new_params);
        format!("FfiStatus {}({});\n", ffi_name, params_str)
    }

    fn generate_result_return_function_with_err(ffi_name: &str, params: &[(String, String)], ok_type: &Type, err_type: Option<&Type>) -> String {
        let err_out_type = err_type.and_then(Self::error_out_type);

        match ok_type {
            Type::Void => {
                let mut new_params = params.to_vec();
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
            Type::String => {
                let mut new_params = params.to_vec();
                let out_name = if err_out_type.is_some() { "out_ok" } else { "out" };
                new_params.push((out_name.to_string(), "FfiString *".to_string()));
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
            _ => {
                let mut new_params = params.to_vec();
                let out_name = if err_out_type.is_some() { "out_ok" } else { "out" };
                new_params.push((out_name.to_string(), format!("{} *", Self::type_to_c(ok_type))));
                if let Some(err_type_str) = &err_out_type {
                    new_params.push(("out_err".to_string(), format!("{} *", err_type_str)));
                }
                let params_str = Self::format_params(&new_params);
                format!("FfiStatus {}({});\n", ffi_name, params_str)
            }
        }
    }

    fn error_out_type(err: &Type) -> Option<String> {
        match err {
            Type::Enum(name) | Type::Record(name) => Some(name.clone()),
            Type::String => Some("FfiError".to_string()),
            _ => None,
        }
    }

    fn generate_option_return_function(ffi_name: &str, params: &[(String, String)], inner: &Type) -> String {
        let option_type = Self::option_type_name(inner);
        let params_str = Self::format_params(params);
        format!("{} {}({});\n", option_type, ffi_name, params_str)
    }

    fn option_type_name(inner: &Type) -> String {
        match inner {
            Type::Primitive(p) => format!("FfiOption_{}", p.rust_name()),
            Type::String => "FfiOption_FfiString".to_string(),
            Type::Record(name) | Type::Enum(name) => format!("FfiOption_{}", name),
            other => format!("FfiOption_{}", Self::type_to_c(other)),
        }
    }

    fn generate_option_string_return_function(ffi_name: &str, params: &[(String, String)]) -> String {
        let params_str = Self::format_params(params);
        format!("FfiOption_FfiString {}({});\n", ffi_name, params_str)
    }

    fn generate_option_vec_return_function(ffi_name: &str, params: &[(String, String)], inner: &Type) -> String {
        let params_str = Self::format_params(params);
        let buf_type = format!("FfiBuf_{}", Self::primitive_to_cbindgen_name(inner));
        format!("FfiOption_{} {}({});\n", buf_type, ffi_name, params_str)
    }

    fn generate_classes(classes: &[Class], prefix: &str, module: &Module) -> String {
        classes.iter().map(|c| Self::generate_class(c, prefix, module)).collect()
    }

    fn generate_class(class: &Class, prefix: &str, module: &Module) -> String {
        let mut out = String::new();
        let snake_name = naming::to_snake_case(&class.name);
        let class_prefix = format!("{}_{}", prefix, snake_name);

        for ctor in &class.constructors {
            let ffi_name = if ctor.is_default() {
                format!("{}_new", class_prefix)
            } else {
                naming::method_ffi_name(&class.name, &ctor.name).into_string()
            };
            if ctor.inputs.is_empty() {
                out.push_str(&format!("struct {} * {}(void);\n", class.name, ffi_name));
            } else {
                let params: Vec<String> = ctor
                    .inputs
                    .iter()
                    .flat_map(|p| Self::param_to_c(&p.name, &p.param_type, module))
                    .map(|(n, t)| format!("{} {}", t, n))
                    .collect();
                out.push_str(&format!("struct {} * {}({});\n", class.name, ffi_name, params.join(", ")));
            }
        }
        out.push_str(&format!("void {}_free(struct {} * handle);\n", class_prefix, class.name));

        for method in &class.methods {
            out.push_str(&Self::generate_method(method, &class.name, &class_prefix, module));
        }

        for stream in &class.streams {
            out.push_str(&Self::generate_stream(stream, &class.name, &class_prefix));
        }

        for iter in &class.async_iterators {
            out.push_str(&Self::generate_async_iterator(iter, &class.name, &class_prefix));
        }

        out
    }

    fn generate_method(method: &Method, class_name: &str, class_prefix: &str, module: &Module) -> String {
        let ffi_name = format!("{}_{}", class_prefix, method.name);

        let mut params: Vec<(String, String)> = if method.is_static() {
            Vec::new()
        } else {
            vec![("handle".to_string(), format!("struct {} *", class_name))]
        };

        for p in &method.inputs {
            params.extend(Self::param_to_c(&p.name, &p.param_type, module));
        }

        if method.is_async {
            Self::generate_async_function(&ffi_name, &params, &method.returns)
        } else {
            Self::generate_sync_function(&ffi_name, &params, &method.returns, module)
        }
    }

    fn generate_stream(stream: &StreamMethod, class_name: &str, class_prefix: &str) -> String {
        let base_name = format!("{}_{}", class_prefix, stream.name);

        format!(
            "SubscriptionHandle {}(const struct {} *handle);\n\
             FfiBuf_u8 {}_pop_batch(SubscriptionHandle subscription_handle, uintptr_t max_count);\n\
             int32_t {}_wait(SubscriptionHandle subscription_handle, uint32_t timeout_milliseconds);\n\
             void {}_poll(SubscriptionHandle subscription_handle, uint64_t callback_data, StreamContinuationCallback callback);\n\
             void {}_unsubscribe(SubscriptionHandle subscription_handle);\n\
             void {}_free(SubscriptionHandle subscription_handle);\n",
            base_name, class_name, base_name, base_name, base_name, base_name, base_name,
        )
    }

    fn generate_async_iterator(iter: &AsyncIteratorMethod, class_name: &str, class_prefix: &str) -> String {
        let base_name = format!("{}_{}", class_prefix, iter.name);
        let iter_handle_type = format!("struct {}_{}_Iterator", class_name, iter.name);

        format!(
            "{iter_handle_type}* {base_name}(const struct {class_name} *handle);\n\
             RustFutureHandle {base_name}_next({iter_handle_type}* iter_handle);\n\
             void {base_name}_next_poll(RustFutureHandle handle, uint64_t callback_data, RustFutureContinuationCallback callback);\n\
             FfiBuf_u8 {base_name}_next_complete(RustFutureHandle handle, FfiStatus* out_status);\n\
             void {base_name}_next_cancel(RustFutureHandle handle);\n\
             void {base_name}_next_free(RustFutureHandle handle);\n\
             void {base_name}_free({iter_handle_type}* iter_handle);\n",
        )
    }

    fn build_params(inputs: &[Parameter], module: &Module) -> Vec<(String, String)> {
        inputs.iter().flat_map(|p| Self::param_to_c(&p.name, &p.param_type, module)).collect()
    }

    fn param_to_c(name: &str, ty: &Type, module: &Module) -> Vec<(String, String)> {
        let name = naming::escape_c_keyword(name);
        match ty {
            Type::String => vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())],
            Type::Builtin(_) => vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())],
            Type::Slice(inner) => {
                if matches!(inner.as_ref(), Type::Record(_) | Type::Builtin(_)) {
                    vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())]
                } else {
                    vec![
                        (format!("{}_ptr", name), format!("const {}*", Self::type_to_c(inner))),
                        (format!("{}_len", name), "uintptr_t".to_string()),
                    ]
                }
            }
            Type::MutSlice(inner) => {
                if matches!(inner.as_ref(), Type::Record(_) | Type::Builtin(_)) {
                    vec![(format!("{}_ptr", name), "uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())]
                } else {
                    vec![
                        (format!("{}_ptr", name), format!("{}*", Self::type_to_c(inner))),
                        (format!("{}_len", name), "uintptr_t".to_string()),
                    ]
                }
            }
            Type::Vec(inner) => {
                if matches!(inner.as_ref(), Type::Builtin(_) | Type::Record(_) | Type::Vec(_) | Type::Enum(_)) {
                    vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())]
                } else {
                    vec![
                        (format!("{}_ptr", name), format!("const {}*", Self::type_to_c(inner))),
                        (format!("{}_len", name), "uintptr_t".to_string()),
                    ]
                }
            }
            Type::Option(inner) => match inner.as_ref() {
                Type::Object(_) | Type::BoxedTrait(_) | Type::Closure(_) => Self::param_to_c(&name, inner, module),
                _ => {
                    vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())]
                }
            },
            Type::Closure(sig) => {
                let params_c: Vec<String> = sig.params.iter().flat_map(Self::closure_param_to_c).collect();
                let params_str = params_c.join(", ");
                let ret_c = Self::closure_return_c_type(&sig.returns, module);
                let callback_type = if params_str.is_empty() {
                    format!("{} (*)(void*)", ret_c)
                } else {
                    format!("{} (*)(void*, {})", ret_c, params_str)
                };
                vec![(format!("{}_cb", name), callback_type), (format!("{}_ud", name), "void*".to_string())]
            }
            Type::Record(_) => vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())],
            Type::Enum(_) => vec![(format!("{}_ptr", name), "const uint8_t*".to_string()), (format!("{}_len", name), "uintptr_t".to_string())],
            _ => vec![(name.to_string(), Self::type_to_c(ty))],
        }
    }

    fn closure_param_to_c(ty: &Type) -> Vec<String> {
        match ty {
            Type::Record(_) => vec!["const uint8_t*".to_string(), "uintptr_t".to_string()],
            Type::String => vec!["const uint8_t*".to_string(), "uintptr_t".to_string()],
            Type::Builtin(_) => vec!["const uint8_t*".to_string(), "uintptr_t".to_string()],
            _ => vec![Self::type_to_c(ty)],
        }
    }

    fn format_params(params: &[(String, String)]) -> String {
        if params.is_empty() {
            "void".to_string()
        } else {
            params
                .iter()
                .map(|(name, ty)| {
                    if ty.contains("(*)") {
                        ty.replace("(*)", &format!("(*{})", name))
                    } else {
                        format!("{} {}", ty, name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn async_result_type(returns: &ReturnType) -> String {
        match returns {
            ReturnType::Void => "void".to_string(),
            ReturnType::Value(ty) => Self::async_result_type_from_type(ty),
            ReturnType::Fallible { .. } => "FfiBuf_u8".to_string(),
        }
    }

    fn async_result_type_from_type(ty: &Type) -> String {
        match ty {
            Type::Void => "void".to_string(),
            Type::Primitive(_) => Self::type_to_c(ty),
            _ => "FfiBuf_u8".to_string(),
        }
    }

    fn primitive_to_cbindgen_name(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => match p {
                Primitive::I8 => "i8",
                Primitive::I16 => "i16",
                Primitive::I32 => "i32",
                Primitive::I64 => "i64",
                Primitive::U8 => "u8",
                Primitive::U16 => "u16",
                Primitive::U32 => "u32",
                Primitive::U64 => "u64",
                Primitive::F32 => "f32",
                Primitive::F64 => "f64",
                Primitive::Bool => "bool",
                Primitive::Usize => "usize",
                Primitive::Isize => "isize",
            }
            .to_string(),
            _ => Self::type_to_c(ty),
        }
    }

    fn type_to_c(ty: &Type) -> String {
        match ty {
            Type::Primitive(p) => p.c_type_name().to_string(),
            Type::String => "FfiString".to_string(),
            Type::Bytes => "uint8_t*".to_string(),
            Type::Builtin(_) => "uint8_t".to_string(),
            Type::Record(name) | Type::Enum(name) => name.clone(),
            Type::Custom { repr, .. } => Self::type_to_c(repr),
            Type::Object(name) => format!("struct {}*", name),
            Type::BoxedTrait(_) => "BoltFFICallbackHandle".to_string(),
            Type::Vec(inner) => format!("{}*", Self::type_to_c(inner)),
            Type::Option(inner) => Self::type_to_c(inner),
            Type::Slice(inner) => format!("const {}*", Self::type_to_c(inner)),
            Type::MutSlice(inner) => format!("{}*", Self::type_to_c(inner)),
            Type::Closure(_) => "void*".to_string(),
            Type::Void => "void".to_string(),
            Type::Result { ok, .. } => Self::type_to_c(ok),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ClosureSignature;
    use std::collections::{HashMap, HashSet};

    fn empty_module() -> Module {
        Module {
            name: "test".to_string(),
            classes: vec![],
            records: vec![],
            enums: vec![],
            functions: vec![],
            callback_traits: vec![],
            custom_types: vec![],
            used_builtins: HashSet::new(),
            closures: HashMap::new(),
        }
    }

    #[test]
    fn option_primitive_is_wire_encoded() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("value", &Type::Option(Box::new(Type::Primitive(Primitive::I32))), &module);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "value_ptr");
        assert_eq!(params[0].1, "const uint8_t*");
        assert_eq!(params[1].0, "value_len");
        assert_eq!(params[1].1, "uintptr_t");
    }

    #[test]
    fn option_string_is_wire_encoded() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("name", &Type::Option(Box::new(Type::String)), &module);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].1, "const uint8_t*");
    }

    #[test]
    fn option_record_is_wire_encoded() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("point", &Type::Option(Box::new(Type::Record("Point".into()))), &module);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].1, "const uint8_t*");
    }

    #[test]
    fn option_enum_is_wire_encoded() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("color", &Type::Option(Box::new(Type::Enum("Color".into()))), &module);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].1, "const uint8_t*");
    }

    #[test]
    fn option_object_is_nullable_pointer() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("handle", &Type::Option(Box::new(Type::Object("Player".into()))), &module);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].1, "struct Player*");
    }

    #[test]
    fn option_boxed_trait_is_nullable_pointer() {
        let module = empty_module();
        let params = CHeaderGenerator::param_to_c("listener", &Type::Option(Box::new(Type::BoxedTrait("Listener".into()))), &module);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].1, "BoltFFICallbackHandle");
    }

    #[test]
    fn option_closure_is_nullable_function_pointer() {
        let module = empty_module();
        let sig = ClosureSignature {
            params: vec![],
            returns: Box::new(Type::Void),
        };
        let params = CHeaderGenerator::param_to_c("on_done", &Type::Option(Box::new(Type::Closure(sig))), &module);
        assert_eq!(params.len(), 2);
        assert!(params[0].0.contains("cb"));
        assert!(params[1].0.contains("ud"));
    }

    #[test]
    fn closure_return_void_is_void() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Void, &module), "void");
    }

    #[test]
    fn closure_return_primitive_is_direct() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Primitive(Primitive::I32), &module), "int32_t");
    }

    #[test]
    fn closure_return_string_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::String, &module), "FfiBuf_u8");
    }

    #[test]
    fn closure_return_bytes_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Bytes, &module), "FfiBuf_u8");
    }

    #[test]
    fn closure_return_record_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Record("Point".into()), &module), "FfiBuf_u8");
    }

    #[test]
    fn closure_return_enum_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Enum("Color".into()), &module), "FfiBuf_u8");
    }

    #[test]
    fn closure_return_vec_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(
            CHeaderGenerator::closure_return_c_type(&Type::Vec(Box::new(Type::Primitive(Primitive::I32))), &module),
            "FfiBuf_u8"
        );
    }

    #[test]
    fn closure_return_option_is_wire_encoded() {
        let module = empty_module();
        assert_eq!(
            CHeaderGenerator::closure_return_c_type(&Type::Option(Box::new(Type::Primitive(Primitive::I32))), &module),
            "FfiBuf_u8"
        );
    }

    #[test]
    fn closure_return_object_is_struct_pointer() {
        let module = empty_module();
        assert_eq!(CHeaderGenerator::closure_return_c_type(&Type::Object("Player".into()), &module), "struct Player*");
    }

    #[test]
    fn closure_return_boxed_trait_is_callback_handle() {
        let module = empty_module();
        assert_eq!(
            CHeaderGenerator::closure_return_c_type(&Type::BoxedTrait("Listener".into()), &module),
            "BoltFFICallbackHandle"
        );
    }
}
