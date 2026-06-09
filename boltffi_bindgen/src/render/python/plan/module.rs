use crate::ir::types::PrimitiveType;

use super::{
    PythonCStyleEnum, PythonCallable, PythonFunction, PythonNativeCallable, PythonParameter,
    PythonRecord, PythonSequenceType, PythonType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonModule {
    pub module_name: String,
    pub package_name: String,
    pub package_version: Option<String>,
    pub library_name: String,
    pub free_buffer_symbol: String,
    pub records: Vec<PythonRecord>,
    pub enums: Vec<PythonCStyleEnum>,
    pub functions: Vec<PythonFunction>,
}

impl PythonModule {
    pub fn module_name_literal(&self) -> String {
        format!("{:?}", self.module_name)
    }

    pub fn package_name_literal(&self) -> String {
        format!("{:?}", self.package_name)
    }

    pub fn package_version_literal(&self) -> String {
        self.package_version
            .as_ref()
            .map(|version| format!("{version:?}"))
            .unwrap_or_else(|| "None".to_string())
    }

    pub fn exported_names(&self) -> Vec<&str> {
        self.enums
            .iter()
            .map(PythonCStyleEnum::class_name)
            .chain(self.records.iter().map(PythonRecord::class_name))
            .chain(
                self.functions
                    .iter()
                    .map(|function| function.python_name.as_str()),
            )
            .collect()
    }

    fn callables(&self) -> impl Iterator<Item = &PythonCallable> {
        self.functions
            .iter()
            .map(PythonFunction::callable)
            .chain(self.records.iter().flat_map(PythonRecord::callables))
            .chain(self.enums.iter().flat_map(PythonCStyleEnum::callables))
    }

    pub fn native_callables(&self) -> Vec<PythonNativeCallable<'_>> {
        self.functions
            .iter()
            .map(PythonFunction::native_callable)
            .chain(self.records.iter().flat_map(PythonRecord::native_callables))
            .chain(
                self.enums
                    .iter()
                    .flat_map(PythonCStyleEnum::native_callables),
            )
            .collect()
    }

    pub fn has_native_callables(&self) -> bool {
        !self.functions.is_empty()
            || self.records.iter().any(PythonRecord::has_native_callables)
            || self
                .enums
                .iter()
                .any(PythonCStyleEnum::has_native_callables)
    }

    pub fn uses_records(&self) -> bool {
        !self.records.is_empty()
    }

    pub fn uses_registered_types(&self) -> bool {
        self.uses_records() || self.uses_c_style_enums()
    }

    pub fn used_primitive_types(&self) -> Vec<PrimitiveType> {
        self.records
            .iter()
            .flat_map(|record| record.fields.iter().map(|field| field.primitive))
            .chain(
                self.enums
                    .iter()
                    .map(|enumeration| enumeration.type_ref.tag_type),
            )
            .chain(self.callables().flat_map(|callable| {
                callable
                    .parameters
                    .iter()
                    .filter_map(|parameter| parameter.type_ref.native_primitive())
                    .chain(callable.return_type.native_primitive())
            }))
            .fold(Vec::new(), |mut primitive_types, primitive| {
                if !primitive_types.contains(&primitive) {
                    primitive_types.push(primitive);
                }
                primitive_types
            })
    }

    pub fn uses_string_parameters(&self) -> bool {
        self.callables()
            .any(|callable| callable.parameters.iter().any(PythonParameter::is_string))
    }

    pub fn uses_buffer_input_parameters(&self) -> bool {
        self.callables().any(|callable| {
            callable
                .parameters
                .iter()
                .any(PythonParameter::uses_buffer_input)
        })
    }

    pub fn uses_c_style_enums(&self) -> bool {
        !self.enums.is_empty()
    }

    pub fn uses_string_returns(&self) -> bool {
        self.callables().any(PythonCallable::returns_string)
    }

    pub fn uses_record_returns(&self) -> bool {
        self.callables().any(PythonCallable::returns_record)
    }

    pub fn uses_c_style_enum_returns(&self) -> bool {
        self.callables().any(PythonCallable::returns_c_style_enum)
    }

    pub fn uses_bytes_returns(&self) -> bool {
        self.callables().any(PythonCallable::returns_bytes)
    }

    pub fn uses_primitive_vector_returns(&self) -> bool {
        self.callables()
            .any(PythonCallable::returns_primitive_vector)
    }

    pub fn uses_c_style_enum_vector_returns(&self) -> bool {
        self.callables()
            .any(PythonCallable::returns_c_style_enum_vector)
    }

    pub fn uses_owned_buffer_returns(&self) -> bool {
        self.callables()
            .any(|callable| callable.return_type.is_owned_buffer())
    }

    pub fn uses_sequence_parameter_annotations(&self) -> bool {
        self.callables().any(|callable| {
            callable.parameters.iter().any(|parameter| {
                parameter.type_ref.is_primitive_vector()
                    || parameter.type_ref.is_c_style_enum_vector()
            })
        })
    }

    pub fn used_primitive_vector_parameter_types(&self) -> Vec<PrimitiveType> {
        self.callables()
            .flat_map(|callable| {
                callable
                    .parameters
                    .iter()
                    .filter_map(|parameter| match &parameter.type_ref {
                        PythonType::Sequence(PythonSequenceType::PrimitiveVec(primitive)) => {
                            Some(*primitive)
                        }
                        _ => None,
                    })
            })
            .fold(Vec::new(), |mut primitive_types, primitive| {
                if !primitive_types.contains(&primitive) {
                    primitive_types.push(primitive);
                }
                primitive_types
            })
    }

    pub fn used_primitive_vector_return_types(&self) -> Vec<PrimitiveType> {
        self.callables()
            .filter_map(PythonCallable::return_vector_primitive)
            .filter(|primitive| *primitive != PrimitiveType::U8)
            .fold(Vec::new(), |mut primitive_types, primitive| {
                if !primitive_types.contains(&primitive) {
                    primitive_types.push(primitive);
                }
                primitive_types
            })
    }

    pub fn used_enum_vector_parameter_types(&self) -> Vec<&super::PythonEnumType> {
        self.callables()
            .flat_map(|callable| {
                callable
                    .parameters
                    .iter()
                    .filter_map(|parameter| parameter.type_ref.sequence_c_style_enum())
            })
            .fold(Vec::new(), |mut enum_types, enum_type| {
                if !enum_types.contains(&enum_type) {
                    enum_types.push(enum_type);
                }
                enum_types
            })
    }

    pub fn used_enum_vector_return_types(&self) -> Vec<&super::PythonEnumType> {
        self.callables()
            .fold(Vec::new(), |mut enum_types, callable| {
                if let Some(enum_type) = callable.return_vector_c_style_enum()
                    && !enum_types.contains(&enum_type)
                {
                    enum_types.push(enum_type);
                }
                enum_types
            })
    }
}
