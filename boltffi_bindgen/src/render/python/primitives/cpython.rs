use crate::ir::types::PrimitiveType;
use crate::render::python::{PythonCallable, PythonParameter, PythonSequenceType, PythonType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CPythonCBinding {
    pub c_type_name: String,
    pub name: String,
}

pub(crate) trait CPythonPrimitiveTypeExt {
    fn c_type_name(self) -> &'static str;
    fn parser_name(self) -> &'static str;
    fn boxer_name(self) -> &'static str;
    fn vector_parser_name(self) -> &'static str;
    fn buffer_kind_constant(self) -> &'static str;
    fn allows_untyped_vector_buffer(self) -> bool;
    fn uses_bool_parser(self) -> bool;
    fn uses_signed_long_long_parser(self) -> bool;
    fn signed_min_macro(self) -> &'static str;
    fn signed_max_macro(self) -> &'static str;
    fn uses_unsigned_long_long_with_range_check(self) -> bool;
    fn unsigned_max_macro(self) -> &'static str;
    fn uses_u64_parser(self) -> bool;
    fn uses_isize_parser(self) -> bool;
    fn uses_usize_parser(self) -> bool;
    fn uses_f32_parser(self) -> bool;
    fn uses_f64_parser(self) -> bool;
    fn boxes_as_bool(self) -> bool;
    fn boxes_as_signed_long(self) -> bool;
    fn boxes_as_unsigned_long(self) -> bool;
    fn boxes_as_signed_long_long(self) -> bool;
    fn boxes_as_unsigned_long_long(self) -> bool;
    fn boxes_as_ssize(self) -> bool;
    fn boxes_as_size(self) -> bool;
    fn boxes_as_double(self) -> bool;
    fn rust_name(self) -> &'static str;
}

impl CPythonPrimitiveTypeExt for PrimitiveType {
    fn c_type_name(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "int8_t",
            PrimitiveType::U8 => "uint8_t",
            PrimitiveType::I16 => "int16_t",
            PrimitiveType::U16 => "uint16_t",
            PrimitiveType::I32 => "int32_t",
            PrimitiveType::U32 => "uint32_t",
            PrimitiveType::I64 => "int64_t",
            PrimitiveType::U64 => "uint64_t",
            PrimitiveType::ISize => "intptr_t",
            PrimitiveType::USize => "uintptr_t",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        }
    }

    fn parser_name(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "boltffi_python_parse_bool",
            PrimitiveType::I8 => "boltffi_python_parse_i8",
            PrimitiveType::U8 => "boltffi_python_parse_u8",
            PrimitiveType::I16 => "boltffi_python_parse_i16",
            PrimitiveType::U16 => "boltffi_python_parse_u16",
            PrimitiveType::I32 => "boltffi_python_parse_i32",
            PrimitiveType::U32 => "boltffi_python_parse_u32",
            PrimitiveType::I64 => "boltffi_python_parse_i64",
            PrimitiveType::U64 => "boltffi_python_parse_u64",
            PrimitiveType::ISize => "boltffi_python_parse_isize",
            PrimitiveType::USize => "boltffi_python_parse_usize",
            PrimitiveType::F32 => "boltffi_python_parse_f32",
            PrimitiveType::F64 => "boltffi_python_parse_f64",
        }
    }

    fn boxer_name(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "boltffi_python_box_bool",
            PrimitiveType::I8 => "boltffi_python_box_i8",
            PrimitiveType::U8 => "boltffi_python_box_u8",
            PrimitiveType::I16 => "boltffi_python_box_i16",
            PrimitiveType::U16 => "boltffi_python_box_u16",
            PrimitiveType::I32 => "boltffi_python_box_i32",
            PrimitiveType::U32 => "boltffi_python_box_u32",
            PrimitiveType::I64 => "boltffi_python_box_i64",
            PrimitiveType::U64 => "boltffi_python_box_u64",
            PrimitiveType::ISize => "boltffi_python_box_isize",
            PrimitiveType::USize => "boltffi_python_box_usize",
            PrimitiveType::F32 => "boltffi_python_box_f32",
            PrimitiveType::F64 => "boltffi_python_box_f64",
        }
    }

    fn vector_parser_name(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "boltffi_python_parse_vec_bool",
            PrimitiveType::I8 => "boltffi_python_parse_vec_i8",
            PrimitiveType::U8 => "boltffi_python_parse_vec_u8",
            PrimitiveType::I16 => "boltffi_python_parse_vec_i16",
            PrimitiveType::U16 => "boltffi_python_parse_vec_u16",
            PrimitiveType::I32 => "boltffi_python_parse_vec_i32",
            PrimitiveType::U32 => "boltffi_python_parse_vec_u32",
            PrimitiveType::I64 => "boltffi_python_parse_vec_i64",
            PrimitiveType::U64 => "boltffi_python_parse_vec_u64",
            PrimitiveType::ISize => "boltffi_python_parse_vec_isize",
            PrimitiveType::USize => "boltffi_python_parse_vec_usize",
            PrimitiveType::F32 => "boltffi_python_parse_vec_f32",
            PrimitiveType::F64 => "boltffi_python_parse_vec_f64",
        }
    }

    fn buffer_kind_constant(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "BOLTFFI_PY_BUFFER_BOOL",
            PrimitiveType::I8 => "BOLTFFI_PY_BUFFER_I8",
            PrimitiveType::U8 => "BOLTFFI_PY_BUFFER_U8",
            PrimitiveType::I16 => "BOLTFFI_PY_BUFFER_I16",
            PrimitiveType::U16 => "BOLTFFI_PY_BUFFER_U16",
            PrimitiveType::I32 => "BOLTFFI_PY_BUFFER_I32",
            PrimitiveType::U32 => "BOLTFFI_PY_BUFFER_U32",
            PrimitiveType::I64 => "BOLTFFI_PY_BUFFER_I64",
            PrimitiveType::U64 => "BOLTFFI_PY_BUFFER_U64",
            PrimitiveType::ISize => "BOLTFFI_PY_BUFFER_ISIZE",
            PrimitiveType::USize => "BOLTFFI_PY_BUFFER_USIZE",
            PrimitiveType::F32 => "BOLTFFI_PY_BUFFER_F32",
            PrimitiveType::F64 => "BOLTFFI_PY_BUFFER_F64",
        }
    }

    fn allows_untyped_vector_buffer(self) -> bool {
        matches!(self, PrimitiveType::U8)
    }

    fn uses_bool_parser(self) -> bool {
        matches!(self, PrimitiveType::Bool)
    }

    fn uses_signed_long_long_parser(self) -> bool {
        matches!(
            self,
            PrimitiveType::I8 | PrimitiveType::I16 | PrimitiveType::I32 | PrimitiveType::I64
        )
    }

    fn signed_min_macro(self) -> &'static str {
        match self {
            PrimitiveType::I8 => "INT8_MIN",
            PrimitiveType::I16 => "INT16_MIN",
            PrimitiveType::I32 => "INT32_MIN",
            PrimitiveType::I64 => "INT64_MIN",
            _ => unreachable!(),
        }
    }

    fn signed_max_macro(self) -> &'static str {
        match self {
            PrimitiveType::I8 => "INT8_MAX",
            PrimitiveType::I16 => "INT16_MAX",
            PrimitiveType::I32 => "INT32_MAX",
            PrimitiveType::I64 => "INT64_MAX",
            _ => unreachable!(),
        }
    }

    fn uses_unsigned_long_long_with_range_check(self) -> bool {
        matches!(
            self,
            PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32
        )
    }

    fn unsigned_max_macro(self) -> &'static str {
        match self {
            PrimitiveType::U8 => "UINT8_MAX",
            PrimitiveType::U16 => "UINT16_MAX",
            PrimitiveType::U32 => "UINT32_MAX",
            _ => unreachable!(),
        }
    }

    fn uses_u64_parser(self) -> bool {
        matches!(self, PrimitiveType::U64)
    }

    fn uses_isize_parser(self) -> bool {
        matches!(self, PrimitiveType::ISize)
    }

    fn uses_usize_parser(self) -> bool {
        matches!(self, PrimitiveType::USize)
    }

    fn uses_f32_parser(self) -> bool {
        matches!(self, PrimitiveType::F32)
    }

    fn uses_f64_parser(self) -> bool {
        matches!(self, PrimitiveType::F64)
    }

    fn boxes_as_bool(self) -> bool {
        matches!(self, PrimitiveType::Bool)
    }

    fn boxes_as_signed_long(self) -> bool {
        matches!(
            self,
            PrimitiveType::I8 | PrimitiveType::I16 | PrimitiveType::I32
        )
    }

    fn boxes_as_unsigned_long(self) -> bool {
        matches!(
            self,
            PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32
        )
    }

    fn boxes_as_signed_long_long(self) -> bool {
        matches!(self, PrimitiveType::I64)
    }

    fn boxes_as_unsigned_long_long(self) -> bool {
        matches!(self, PrimitiveType::U64)
    }

    fn boxes_as_ssize(self) -> bool {
        matches!(self, PrimitiveType::ISize)
    }

    fn boxes_as_size(self) -> bool {
        matches!(self, PrimitiveType::USize)
    }

    fn boxes_as_double(self) -> bool {
        matches!(self, PrimitiveType::F32 | PrimitiveType::F64)
    }

    fn rust_name(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "i8",
            PrimitiveType::U8 => "u8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::U16 => "u16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::U32 => "u32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::U64 => "u64",
            PrimitiveType::ISize => "isize",
            PrimitiveType::USize => "usize",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
        }
    }
}

pub(crate) trait CPythonTypeExt {
    fn c_type_name(&self) -> String;
}

impl CPythonTypeExt for PythonType {
    fn c_type_name(&self) -> String {
        match self {
            PythonType::Void => "void".to_string(),
            PythonType::Primitive(primitive) => primitive.c_type_name().to_string(),
            PythonType::Record(record_type) => record_type.c_type_name.clone(),
            PythonType::CStyleEnum(enum_type) => enum_type.tag_type.c_type_name().to_string(),
            PythonType::String | PythonType::Sequence(_) => "FfiBuf_u8".to_string(),
        }
    }
}

pub(crate) trait CPythonParameterExt {
    fn ffi_bindings(&self) -> Vec<CPythonCBinding>;
    fn local_bindings(&self) -> Vec<CPythonCBinding>;
    fn parser_name(&self) -> String;
    fn parser_output_arguments(&self) -> Vec<String>;
    fn ffi_argument_expressions(&self) -> Vec<String>;
    fn cleanup_statement(&self) -> Option<String>;
}

impl CPythonParameterExt for PythonParameter {
    fn ffi_bindings(&self) -> Vec<CPythonCBinding> {
        match &self.type_ref {
            PythonType::Void => unreachable!("python parameters cannot be void"),
            PythonType::Primitive(primitive) => vec![CPythonCBinding {
                c_type_name: primitive.c_type_name().to_string(),
                name: self.value_binding_name(),
            }],
            PythonType::Record(record_type) => vec![CPythonCBinding {
                c_type_name: record_type.c_type_name.clone(),
                name: self.value_binding_name(),
            }],
            PythonType::CStyleEnum(enum_type) => vec![CPythonCBinding {
                c_type_name: enum_type.tag_type.c_type_name().to_string(),
                name: self.value_binding_name(),
            }],
            PythonType::String => vec![
                CPythonCBinding {
                    c_type_name: "const uint8_t *".to_string(),
                    name: format!("{}.ptr", self.parser_state_name()),
                },
                CPythonCBinding {
                    c_type_name: "uintptr_t".to_string(),
                    name: format!("{}.len", self.parser_state_name()),
                },
            ],
            PythonType::Sequence(PythonSequenceType::PrimitiveVec(primitive)) => vec![
                CPythonCBinding {
                    c_type_name: format!("const {} *", primitive.c_type_name()),
                    name: format!("{}.ptr", self.parser_state_name()),
                },
                CPythonCBinding {
                    c_type_name: "uintptr_t".to_string(),
                    name: format!("{}.len", self.parser_state_name()),
                },
            ],
            PythonType::Sequence(PythonSequenceType::CStyleEnumVec(_)) => vec![
                CPythonCBinding {
                    c_type_name: "const uint8_t *".to_string(),
                    name: format!("{}.ptr", self.parser_state_name()),
                },
                CPythonCBinding {
                    c_type_name: "uintptr_t".to_string(),
                    name: format!("{}.len", self.parser_state_name()),
                },
            ],
        }
    }

    fn local_bindings(&self) -> Vec<CPythonCBinding> {
        match &self.type_ref {
            PythonType::Void => unreachable!("python parameters cannot be void"),
            PythonType::Primitive(primitive) => vec![CPythonCBinding {
                c_type_name: primitive.c_type_name().to_string(),
                name: self.value_binding_name(),
            }],
            PythonType::Record(record_type) => vec![CPythonCBinding {
                c_type_name: record_type.c_type_name.clone(),
                name: self.value_binding_name(),
            }],
            PythonType::CStyleEnum(enum_type) => vec![CPythonCBinding {
                c_type_name: enum_type.tag_type.c_type_name().to_string(),
                name: self.value_binding_name(),
            }],
            PythonType::String => vec![CPythonCBinding {
                c_type_name: "boltffi_python_utf8_input".to_string(),
                name: self.parser_state_name(),
            }],
            PythonType::Sequence(PythonSequenceType::PrimitiveVec(_))
            | PythonType::Sequence(PythonSequenceType::CStyleEnumVec(_)) => {
                vec![CPythonCBinding {
                    c_type_name: "boltffi_python_buffer_input".to_string(),
                    name: self.parser_state_name(),
                }]
            }
        }
    }

    fn parser_name(&self) -> String {
        match &self.type_ref {
            PythonType::Void => unreachable!("python parameters cannot be void"),
            PythonType::Primitive(primitive) => primitive.parser_name().to_string(),
            PythonType::Record(record_type) => record_type.parser_name(),
            PythonType::CStyleEnum(enum_type) => enum_type.parser_name(),
            PythonType::String => "boltffi_python_parse_string".to_string(),
            PythonType::Sequence(PythonSequenceType::PrimitiveVec(primitive)) => {
                primitive.vector_parser_name().to_string()
            }
            PythonType::Sequence(PythonSequenceType::CStyleEnumVec(enum_type)) => {
                enum_type.vector_parser_name()
            }
        }
    }

    fn parser_output_arguments(&self) -> Vec<String> {
        match &self.type_ref {
            PythonType::Void => unreachable!("python parameters cannot be void"),
            PythonType::Primitive(_) | PythonType::Record(_) | PythonType::CStyleEnum(_) => {
                vec![format!("&{}", self.value_binding_name())]
            }
            PythonType::String | PythonType::Sequence(_) => {
                vec![format!("&{}", self.parser_state_name())]
            }
        }
    }

    fn ffi_argument_expressions(&self) -> Vec<String> {
        match &self.type_ref {
            PythonType::Void => unreachable!("python parameters cannot be void"),
            PythonType::Primitive(_) | PythonType::Record(_) | PythonType::CStyleEnum(_) => {
                vec![self.value_binding_name()]
            }
            PythonType::String => vec![
                format!("{}.ptr", self.parser_state_name()),
                format!("{}.len", self.parser_state_name()),
            ],
            PythonType::Sequence(PythonSequenceType::PrimitiveVec(primitive)) => vec![
                format!(
                    "(const {} *){}.ptr",
                    primitive.c_type_name(),
                    self.parser_state_name()
                ),
                format!("{}.len", self.parser_state_name()),
            ],
            PythonType::Sequence(PythonSequenceType::CStyleEnumVec(_)) => vec![
                format!("(const uint8_t *){}.ptr", self.parser_state_name()),
                format!("{}.len", self.parser_state_name()),
            ],
        }
    }

    fn cleanup_statement(&self) -> Option<String> {
        match &self.type_ref {
            PythonType::Sequence(PythonSequenceType::PrimitiveVec(_)) => Some(format!(
                "boltffi_python_release_buffer_input(&{});",
                self.parser_state_name()
            )),
            PythonType::Sequence(PythonSequenceType::CStyleEnumVec(_)) => Some(format!(
                "boltffi_python_release_buffer_input(&{});",
                self.parser_state_name()
            )),
            _ => None,
        }
    }
}

pub(crate) trait CPythonCallableExt {
    fn binding_stem(&self) -> &str;
    fn wrapper_name(&self) -> String;
    fn function_pointer_typedef_name(&self) -> String;
    fn function_pointer_name(&self) -> String;
    fn ffi_arguments(&self) -> Vec<CPythonCBinding>;
    fn call_argument_expressions(&self) -> Vec<String>;
    fn cleanup_statements(&self) -> Vec<String>;
}

impl CPythonCallableExt for PythonCallable {
    fn binding_stem(&self) -> &str {
        self.native_name.trim_start_matches('_')
    }

    fn wrapper_name(&self) -> String {
        format!("boltffi_python_callable_wrapper_{}", self.binding_stem())
    }

    fn function_pointer_typedef_name(&self) -> String {
        format!("boltffi_python_symbol_{}_fn", self.binding_stem())
    }

    fn function_pointer_name(&self) -> String {
        format!("boltffi_python_symbol_{}", self.binding_stem())
    }

    fn ffi_arguments(&self) -> Vec<CPythonCBinding> {
        self.parameters
            .iter()
            .flat_map(<PythonParameter as CPythonParameterExt>::ffi_bindings)
            .collect()
    }

    fn call_argument_expressions(&self) -> Vec<String> {
        self.parameters
            .iter()
            .flat_map(<PythonParameter as CPythonParameterExt>::ffi_argument_expressions)
            .collect()
    }

    fn cleanup_statements(&self) -> Vec<String> {
        self.parameters
            .iter()
            .filter_map(<PythonParameter as CPythonParameterExt>::cleanup_statement)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::types::PrimitiveType;
    use crate::render::python::{PythonCallable, PythonType};

    use super::CPythonCallableExt;

    #[test]
    fn callable_wrappers_use_callable_only_namespace() {
        let callable = PythonCallable {
            native_name: "register_status".to_string(),
            ffi_symbol: "boltffi_register_status".to_string(),
            parameters: vec![],
            return_type: PythonType::Primitive(PrimitiveType::Bool),
        };

        assert_eq!(
            callable.wrapper_name(),
            "boltffi_python_callable_wrapper_register_status"
        );
    }
}
