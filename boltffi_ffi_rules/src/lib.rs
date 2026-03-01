pub const FFI_PREFIX: &str = "boltffi";

pub mod naming {
    use super::FFI_PREFIX;
    use std::{fmt, marker::PhantomData};

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct Name<K>(String, PhantomData<K>);

    impl<K> Name<K> {
        pub fn new(value: String) -> Self {
            Self(value, PhantomData)
        }

        pub fn as_str(&self) -> &str {
            self.0.as_str()
        }

        pub fn into_string(self) -> String {
            self.0
        }
    }

    impl<K> fmt::Display for Name<K> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.as_str())
        }
    }

    impl<K> AsRef<str> for Name<K> {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl<K> From<Name<K>> for String {
        fn from(name: Name<K>) -> Self {
            name.0
        }
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct GlobalSymbol;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct VtableField;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct VtableType;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct RegisterFn;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct CreateFn;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct ForeignType;
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct ClassPrefix;

    const C_KEYWORDS: &[&str] = &[
        "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum", "extern", "float", "for", "goto", "if", "int", "long", "register",
        "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void", "volatile", "while",
    ];

    pub fn escape_c_keyword(name: &str) -> String {
        if C_KEYWORDS.contains(&name) { format!("{}_", name) } else { name.to_string() }
    }

    pub fn ffi_prefix() -> &'static str {
        FFI_PREFIX
    }

    pub fn to_snake_case(name: &str) -> String {
        let mut result = String::with_capacity(name.len() + 4);
        for (i, ch) in name.chars().enumerate() {
            if ch.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                result.push(ch.to_ascii_lowercase());
            } else {
                result.push(ch);
            }
        }
        result
    }

    pub fn to_upper_camel_case(name: &str) -> String {
        let mut result = String::with_capacity(name.len());
        let mut capitalize_next = true;
        for ch in name.chars() {
            if ch == '_' || ch == '-' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    pub fn snake_to_camel(name: &str) -> String {
        let mut result = String::with_capacity(name.len());
        let mut capitalize_next = false;
        for ch in name.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    pub fn class_ffi_prefix(class_name: &str) -> Name<ClassPrefix> {
        Name::new(format!("{}_{}", FFI_PREFIX, to_snake_case(class_name)))
    }

    pub fn class_ffi_new(class_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_new", class_ffi_prefix(class_name)))
    }

    pub fn class_ffi_free(class_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", class_ffi_prefix(class_name)))
    }

    pub fn method_ffi_name(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", class_ffi_prefix(class_name), method_name))
    }

    pub fn method_ffi_poll(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", method_ffi_name(class_name, method_name)))
    }

    pub fn method_ffi_complete(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_complete", method_ffi_name(class_name, method_name)))
    }

    pub fn method_ffi_cancel(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_cancel", method_ffi_name(class_name, method_name)))
    }

    pub fn method_ffi_free(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", method_ffi_name(class_name, method_name)))
    }

    pub fn function_ffi_name(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", FFI_PREFIX, func_name))
    }

    pub fn function_ffi_poll(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", function_ffi_name(func_name)))
    }

    pub fn function_ffi_complete(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_complete", function_ffi_name(func_name)))
    }

    pub fn function_ffi_cancel(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_cancel", function_ffi_name(func_name)))
    }

    pub fn function_ffi_free(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", function_ffi_name(func_name)))
    }

    pub fn function_ffi_vec_len(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}{}", function_ffi_name(func_name), vec_len_suffix()))
    }

    pub fn function_ffi_vec_copy_into(func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}{}", function_ffi_name(func_name), vec_copy_into_suffix()))
    }

    pub fn iterator_ffi_new(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        method_ffi_name(class_name, method_name)
    }

    pub fn iterator_ffi_next(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_next", method_ffi_name(class_name, method_name)))
    }

    pub fn iterator_ffi_free(class_name: &str, method_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", method_ffi_name(class_name, method_name)))
    }

    pub fn stream_ffi_subscribe(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        method_ffi_name(class_name, stream_name)
    }

    pub fn stream_ffi_pop_batch(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_pop_batch", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_wait(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_wait", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_poll(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_poll", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_unsubscribe(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_unsubscribe", method_ffi_name(class_name, stream_name)))
    }

    pub fn stream_ffi_free(class_name: &str, stream_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_free", method_ffi_name(class_name, stream_name)))
    }

    pub fn free_buf_u8() -> Name<GlobalSymbol> {
        Name::new(format!("{}_free_buf_u8", FFI_PREFIX))
    }

    pub fn atomic_u8_cas() -> Name<GlobalSymbol> {
        Name::new(format!("{}_atomic_u8_cas", FFI_PREFIX))
    }

    pub fn trait_ffi_free(trait_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}_free", FFI_PREFIX, to_snake_case(trait_name)))
    }

    pub fn callback_vtable_name(trait_name: &str) -> Name<VtableType> {
        Name::new(format!("{}VTable", trait_name))
    }

    pub fn callback_foreign_name(trait_name: &str) -> Name<ForeignType> {
        Name::new(format!("Foreign{}", trait_name))
    }

    pub fn callback_register_fn(trait_name: &str) -> Name<RegisterFn> {
        Name::new(format!("{}_register_{}_vtable", FFI_PREFIX, to_snake_case(trait_name)))
    }

    pub fn callback_create_fn(trait_name: &str) -> Name<CreateFn> {
        Name::new(format!("{}_create_{}_handle", FFI_PREFIX, to_snake_case(trait_name)))
    }

    pub fn vtable_field_name(method_name: &str) -> Name<VtableField> {
        Name::new(to_snake_case(method_name))
    }

    pub fn module_name(crate_name: &str) -> String {
        to_upper_camel_case(crate_name)
    }

    pub fn ffi_module_name(crate_name: &str) -> String {
        format!("{}FFI", module_name(crate_name))
    }

    pub fn vec_len_suffix() -> &'static str {
        "_len"
    }

    pub fn vec_copy_into_suffix() -> &'static str {
        "_copy_into"
    }

    pub fn param_ptr_suffix() -> &'static str {
        "_ptr"
    }

    pub fn param_len_suffix() -> &'static str {
        "_len"
    }

    #[deprecated(note = "use function_ffi_name instead")]
    pub fn ffi_function_name(module_prefix: &str, func_name: &str) -> Name<GlobalSymbol> {
        Name::new(format!("{}_{}", module_prefix, func_name))
    }
}

pub mod c_types {
    pub fn primitive_to_c(rust_type: &str) -> &'static str {
        match rust_type {
            "bool" => "bool",
            "i8" => "int8_t",
            "u8" => "uint8_t",
            "i16" => "int16_t",
            "u16" => "uint16_t",
            "i32" => "int32_t",
            "u32" => "uint32_t",
            "i64" => "int64_t",
            "u64" => "uint64_t",
            "f32" => "float",
            "f64" => "double",
            "usize" => "uintptr_t",
            "isize" => "intptr_t",
            _ => "void*",
        }
    }

    pub fn string_c_type() -> &'static str {
        "FfiString"
    }

    pub fn status_c_type() -> &'static str {
        "FfiStatus"
    }

    pub fn size_c_type() -> &'static str {
        "uintptr_t"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamTransform {
    Direct,
    StringToPtr,
    SliceToPtr { mutable: bool },
    VecToPtr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReturnTransform {
    Direct,
    Status,
    StringOut,
    VecLenAndCopy,
    OptionOut,
    ResultOut,
}

pub mod transforms {
    use super::{ParamTransform, ReturnTransform};

    pub fn classify_param(type_str: &str) -> ParamTransform {
        let type_str = type_str.trim();

        if type_str == "&str" || type_str == "& str" {
            return ParamTransform::StringToPtr;
        }
        if type_str == "String" {
            return ParamTransform::StringToPtr;
        }
        if type_str.starts_with("&[") && type_str.ends_with("]") {
            return ParamTransform::SliceToPtr { mutable: false };
        }
        if type_str.starts_with("&mut [") && type_str.ends_with("]") {
            return ParamTransform::SliceToPtr { mutable: true };
        }
        if type_str.starts_with("Vec<") && type_str.ends_with(">") {
            return ParamTransform::VecToPtr;
        }

        ParamTransform::Direct
    }

    pub fn classify_return(type_str: &str) -> ReturnTransform {
        let type_str = type_str.trim();

        if type_str.is_empty() || type_str == "()" {
            return ReturnTransform::Status;
        }
        if type_str == "String" {
            return ReturnTransform::StringOut;
        }
        if type_str.starts_with("Vec<") && type_str.ends_with(">") {
            return ReturnTransform::VecLenAndCopy;
        }
        if type_str.starts_with("Option<") && type_str.ends_with(">") {
            return ReturnTransform::OptionOut;
        }
        if type_str.starts_with("Result<") {
            return ReturnTransform::ResultOut;
        }

        ReturnTransform::Direct
    }
}

pub mod signatures {
    use super::{c_types, naming};

    #[derive(Clone)]
    pub struct FfiParam {
        pub name: String,
        pub c_type: String,
    }

    #[derive(Clone)]
    pub struct FfiSignature {
        pub name: String,
        pub params: Vec<FfiParam>,
        pub return_type: String,
    }

    pub fn string_param(param_name: &str) -> Vec<FfiParam> {
        vec![
            FfiParam {
                name: format!("{}{}", param_name, naming::param_ptr_suffix()),
                c_type: "const uint8_t*".to_string(),
            },
            FfiParam {
                name: format!("{}{}", param_name, naming::param_len_suffix()),
                c_type: c_types::size_c_type().to_string(),
            },
        ]
    }

    pub fn slice_param(param_name: &str, inner_c_type: &str, mutable: bool) -> Vec<FfiParam> {
        let ptr_type = if mutable { format!("{}*", inner_c_type) } else { format!("const {}*", inner_c_type) };
        vec![
            FfiParam {
                name: format!("{}{}", param_name, naming::param_ptr_suffix()),
                c_type: ptr_type,
            },
            FfiParam {
                name: format!("{}{}", param_name, naming::param_len_suffix()),
                c_type: c_types::size_c_type().to_string(),
            },
        ]
    }

    pub fn vec_param(param_name: &str, inner_c_type: &str) -> Vec<FfiParam> {
        slice_param(param_name, inner_c_type, false)
    }

    pub fn vec_return_signatures(base_name: &str, inner_c_type: &str, input_params: &[FfiParam]) -> Vec<FfiSignature> {
        let len_name = format!("{}{}", base_name, naming::vec_len_suffix());
        let copy_name = format!("{}{}", base_name, naming::vec_copy_into_suffix());

        let mut copy_params: Vec<FfiParam> = input_params.to_vec();
        copy_params.push(FfiParam {
            name: "dst".to_string(),
            c_type: format!("{}*", inner_c_type),
        });
        copy_params.push(FfiParam {
            name: "dst_cap".to_string(),
            c_type: c_types::size_c_type().to_string(),
        });
        copy_params.push(FfiParam {
            name: "written".to_string(),
            c_type: format!("{}*", c_types::size_c_type()),
        });

        vec![
            FfiSignature {
                name: len_name,
                params: input_params.to_vec(),
                return_type: c_types::size_c_type().to_string(),
            },
            FfiSignature {
                name: copy_name,
                params: copy_params,
                return_type: c_types::status_c_type().to_string(),
            },
        ]
    }

    pub fn string_return_signature(base_name: &str, input_params: &[FfiParam]) -> FfiSignature {
        let mut params = input_params.to_vec();
        params.push(FfiParam {
            name: "out".to_string(),
            c_type: format!("{}*", c_types::string_c_type()),
        });

        FfiSignature {
            name: base_name.to_string(),
            params,
            return_type: c_types::status_c_type().to_string(),
        }
    }
}

pub mod callback {
    use super::naming::to_snake_case;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TypeId {
        Void,
        Bool,
        I8,
        U8,
        I16,
        U16,
        I32,
        U32,
        I64,
        U64,
        F32,
        F64,
        Isize,
        Usize,
        String,
        Bytes,
        Named(std::string::String),
    }

    impl TypeId {
        pub fn from_rust_type_str(s: &str) -> Self {
            match s {
                "bool" => Self::Bool,
                "i8" => Self::I8,
                "u8" => Self::U8,
                "i16" => Self::I16,
                "u16" => Self::U16,
                "i32" => Self::I32,
                "u32" => Self::U32,
                "i64" => Self::I64,
                "u64" => Self::U64,
                "f32" => Self::F32,
                "f64" => Self::F64,
                "isize" => Self::Isize,
                "usize" => Self::Usize,
                "String" | "&str" => Self::String,
                "()" => Self::Void,
                other => Self::Named(other.to_string()),
            }
        }

        pub fn as_signature_part(&self) -> String {
            match self {
                Self::Void => "Void".into(),
                Self::Bool => "Bool".into(),
                Self::I8 => "I8".into(),
                Self::U8 => "U8".into(),
                Self::I16 => "I16".into(),
                Self::U16 => "U16".into(),
                Self::I32 => "I32".into(),
                Self::U32 => "U32".into(),
                Self::I64 => "I64".into(),
                Self::U64 => "U64".into(),
                Self::F32 => "F32".into(),
                Self::F64 => "F64".into(),
                Self::Isize => "Isize".into(),
                Self::Usize => "Usize".into(),
                Self::String => "String".into(),
                Self::Bytes => "Bytes".into(),
                Self::Named(name) => name.clone(),
            }
        }
    }

    pub fn closure_signature_id(params: &[TypeId], returns: &TypeId) -> String {
        let params_part = params.iter().map(|p| p.as_signature_part()).collect::<Vec<_>>().join("_");

        let is_void_return = matches!(returns, TypeId::Void);
        let ret_part = returns.as_signature_part();

        if is_void_return {
            if params_part.is_empty() { "Void".to_string() } else { params_part }
        } else if params_part.is_empty() {
            format!("To{}", ret_part)
        } else {
            format!("{}To{}", params_part, ret_part)
        }
    }

    pub fn closure_callback_id(params: &[TypeId], returns: &TypeId) -> String {
        format!("__Closure_{}", closure_signature_id(params, returns))
    }

    pub fn closure_callback_id_snake(params: &[TypeId], returns: &TypeId) -> String {
        to_snake_case(&closure_callback_id(params, returns))
    }

    pub fn callback_wasm_import_call(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_call", callback_id_snake)
    }

    pub fn callback_wasm_import_free(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_free", callback_id_snake)
    }

    pub fn callback_wasm_import_clone(callback_id_snake: &str) -> String {
        format!("__boltffi_callback_{}_clone", callback_id_snake)
    }

    pub fn callback_create_handle_global() -> &'static str {
        "boltffi_create_callback_handle"
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn type_id_from_rust_primitives() {
            assert_eq!(TypeId::from_rust_type_str("bool"), TypeId::Bool);
            assert_eq!(TypeId::from_rust_type_str("i8"), TypeId::I8);
            assert_eq!(TypeId::from_rust_type_str("u8"), TypeId::U8);
            assert_eq!(TypeId::from_rust_type_str("i16"), TypeId::I16);
            assert_eq!(TypeId::from_rust_type_str("u16"), TypeId::U16);
            assert_eq!(TypeId::from_rust_type_str("i32"), TypeId::I32);
            assert_eq!(TypeId::from_rust_type_str("u32"), TypeId::U32);
            assert_eq!(TypeId::from_rust_type_str("i64"), TypeId::I64);
            assert_eq!(TypeId::from_rust_type_str("u64"), TypeId::U64);
            assert_eq!(TypeId::from_rust_type_str("f32"), TypeId::F32);
            assert_eq!(TypeId::from_rust_type_str("f64"), TypeId::F64);
            assert_eq!(TypeId::from_rust_type_str("isize"), TypeId::Isize);
            assert_eq!(TypeId::from_rust_type_str("usize"), TypeId::Usize);
        }

        #[test]
        fn type_id_from_rust_string_types() {
            assert_eq!(TypeId::from_rust_type_str("String"), TypeId::String);
            assert_eq!(TypeId::from_rust_type_str("&str"), TypeId::String);
        }

        #[test]
        fn type_id_from_rust_void() {
            assert_eq!(TypeId::from_rust_type_str("()"), TypeId::Void);
        }

        #[test]
        fn type_id_from_rust_custom() {
            assert_eq!(TypeId::from_rust_type_str("Point"), TypeId::Named("Point".into()));
            assert_eq!(TypeId::from_rust_type_str("MyCustomType"), TypeId::Named("MyCustomType".into()));
        }

        #[test]
        fn type_id_signature_parts() {
            assert_eq!(TypeId::Void.as_signature_part(), "Void");
            assert_eq!(TypeId::Bool.as_signature_part(), "Bool");
            assert_eq!(TypeId::I32.as_signature_part(), "I32");
            assert_eq!(TypeId::String.as_signature_part(), "String");
            assert_eq!(TypeId::Named("Point".into()).as_signature_part(), "Point");
        }

        #[test]
        fn closure_i32_to_i32() {
            let params = vec![TypeId::I32];
            let returns = TypeId::I32;
            assert_eq!(closure_signature_id(&params, &returns), "I32ToI32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_I32ToI32");
            assert_eq!(closure_callback_id_snake(&params, &returns), "___closure__i32_to_i32");
        }

        #[test]
        fn closure_point_to_point() {
            let params = vec![TypeId::Named("Point".into())];
            let returns = TypeId::Named("Point".into());
            assert_eq!(closure_signature_id(&params, &returns), "PointToPoint");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_PointToPoint");
        }

        #[test]
        fn closure_void_return() {
            let params = vec![TypeId::I32];
            let returns = TypeId::Void;
            assert_eq!(closure_signature_id(&params, &returns), "I32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_I32");
        }

        #[test]
        fn closure_no_params_with_return() {
            let params = vec![];
            let returns = TypeId::I32;
            assert_eq!(closure_signature_id(&params, &returns), "ToI32");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_ToI32");
        }

        #[test]
        fn closure_no_params_void_return() {
            let params = vec![];
            let returns = TypeId::Void;
            assert_eq!(closure_signature_id(&params, &returns), "Void");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_Void");
        }

        #[test]
        fn closure_multi_params() {
            let params = vec![TypeId::I32, TypeId::String];
            let returns = TypeId::Bool;
            assert_eq!(closure_signature_id(&params, &returns), "I32_StringToBool");
            assert_eq!(closure_callback_id(&params, &returns), "__Closure_I32_StringToBool");
        }

        #[test]
        fn closure_all_primitives_void() {
            let params = vec![
                TypeId::Bool,
                TypeId::I8,
                TypeId::U8,
                TypeId::I16,
                TypeId::U16,
                TypeId::I32,
                TypeId::U32,
                TypeId::I64,
                TypeId::U64,
                TypeId::F32,
                TypeId::F64,
            ];
            let returns = TypeId::Void;
            let sig = closure_signature_id(&params, &returns);
            assert_eq!(sig, "Bool_I8_U8_I16_U16_I32_U32_I64_U64_F32_F64");
        }

        #[test]
        fn wasm_import_names() {
            let id_snake = "___closure__i32_to_i32";
            assert_eq!(callback_wasm_import_call(id_snake), "__boltffi_callback____closure__i32_to_i32_call");
            assert_eq!(callback_wasm_import_free(id_snake), "__boltffi_callback____closure__i32_to_i32_free");
            assert_eq!(callback_wasm_import_clone(id_snake), "__boltffi_callback____closure__i32_to_i32_clone");
        }

        #[test]
        fn wasm_import_names_for_void_closure() {
            let params = vec![];
            let returns = TypeId::Void;
            let id_snake = closure_callback_id_snake(&params, &returns);
            assert_eq!(callback_wasm_import_call(&id_snake), "__boltffi_callback____closure__void_call");
            assert_eq!(callback_wasm_import_free(&id_snake), "__boltffi_callback____closure__void_free");
        }

        #[test]
        fn global_create_handle_name() {
            assert_eq!(callback_create_handle_global(), "boltffi_create_callback_handle");
        }

        #[test]
        fn inv09_naming_deterministic() {
            let params = vec![TypeId::I32, TypeId::String];
            let returns = TypeId::Bool;

            let id1 = closure_callback_id_snake(&params, &returns);
            let id2 = closure_callback_id_snake(&params, &returns);
            assert_eq!(id1, id2, "INV-09: naming must be deterministic");

            let call1 = callback_wasm_import_call(&id1);
            let call2 = callback_wasm_import_call(&id2);
            assert_eq!(call1, call2, "INV-09: import names must be deterministic");
        }
    }
}

pub mod transport {
    //! When returning a buffer (wire-encoded data) from Rust to the host, we need
    //! to tell the host where the data lives and how big it is. Different platforms
    //! have different optimal ways to do this:
    //!
    //! - WASM: pointers are 32-bit, so ptr+len fits in a single u64 register.
    //!   Returning a packed u64 avoids allocating a separate descriptor struct.
    //!
    //! - Native 64-bit: pointers are 64-bit, so we can't pack ptr+len into u64.
    //!   We return a FfiBuf struct containing { ptr, len, cap }.
    //!
    //! This module defines the transport strategies so both the macro (which
    //! generates Rust FFI exports) and the codegen (which generates host bindings)
    //! use the same rules. Adding a new platform means adding a variant here and
    //! handling it in both macro and codegen.

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BufferTransport {
        Packed,
        Descriptor,
    }

    impl BufferTransport {
        pub fn for_target(target: &str) -> Self {
            match target {
                "wasm32" | "wasm32-unknown-unknown" | "wasm32-wasi" => Self::Packed,
                _ => Self::Descriptor,
            }
        }

        pub fn is_packed(self) -> bool {
            matches!(self, Self::Packed)
        }

        pub fn is_descriptor(self) -> bool {
            matches!(self, Self::Descriptor)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum EncodedReturnStrategy {
        Utf8String,
        PrimitiveVec,
        OptionScalar,
        ResultScalar,
        WireEncoded,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn wasm_targets_use_packed() {
            assert_eq!(BufferTransport::for_target("wasm32"), BufferTransport::Packed);
            assert_eq!(BufferTransport::for_target("wasm32-unknown-unknown"), BufferTransport::Packed);
            assert_eq!(BufferTransport::for_target("wasm32-wasi"), BufferTransport::Packed);
        }

        #[test]
        fn native_targets_use_descriptor() {
            assert_eq!(BufferTransport::for_target("aarch64-apple-darwin"), BufferTransport::Descriptor);
            assert_eq!(BufferTransport::for_target("x86_64-unknown-linux-gnu"), BufferTransport::Descriptor);
            assert_eq!(BufferTransport::for_target("aarch64-linux-android"), BufferTransport::Descriptor);
        }
    }
}
