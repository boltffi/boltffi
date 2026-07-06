//! Scans Rust source and generates foreign-language bindings (Java, Kotlin,
//! Swift, C#, TypeScript, etc.) from an intermediate representation.

#![allow(dead_code)]

pub mod artifact;
pub mod generate;
pub mod ir;
pub mod metadata;
pub mod model;
pub mod render;
pub mod scan;
pub mod target;

pub use model::{
    Class, Constructor, ConstructorParam, Deprecation, Enumeration, Function, Method, Module,
    Parameter, Primitive, Receiver, Record, RecordField, StreamMethod, StreamMode, Type, Variant,
};

pub use boltffi_ffi_rules::naming::{LibraryName, ffi_prefix, library_name, load_library_name};
pub use render::c::CHeaderLowerer;
pub use render::kotlin::{FactoryStyle, KotlinApiStyle, KotlinOptions};
pub use render::{Renderer, TypeConversion, TypeMapping, TypeMappings};
pub use scan::{SourceScanner, scan_crate, scan_crate_with_pointer_width};

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_module() -> Module {
        let sensor_class = Class::new("Sensor")
            .with_doc("A hardware sensor interface")
            .with_constructor(Constructor::new())
            .with_method(
                Method::new("get_reading", Receiver::Ref)
                    .with_output(Type::Primitive(Primitive::F64)),
            )
            .with_method(
                Method::new("predict_next", Receiver::Ref)
                    .with_param(Parameter::new("samples", Type::Primitive(Primitive::U32)))
                    .with_output(Type::vec(Type::Primitive(Primitive::F64)))
                    .make_async(),
            )
            .with_stream(StreamMethod::new(
                "readings",
                Type::Primitive(Primitive::F64),
            ));

        let reading_record = Record::new("SensorReading")
            .with_field(RecordField::new(
                "timestamp",
                Type::Primitive(Primitive::U64),
            ))
            .with_field(RecordField::new("value", Type::Primitive(Primitive::F64)))
            .with_field(RecordField::new("unit", Type::String));

        let status_enum = Enumeration::new("SensorStatus")
            .with_variant(Variant::new("idle").with_discriminant(0))
            .with_variant(Variant::new("active").with_discriminant(1))
            .with_variant(Variant::new("error").with_discriminant(2));

        Module::new("sensors")
            .with_class(sensor_class)
            .with_record(reading_record)
            .with_enum(status_enum)
    }

    fn create_test_module_with_custom_type() -> Module {
        let instant = Type::Custom {
            name: "UtcDateTime".to_string(),
            repr: Box::new(Type::Primitive(Primitive::I64)),
        };

        let event = Record::new("Event").with_field(RecordField::new("at", instant.clone()));

        Module::new("test")
            .with_custom_type(model::CustomType::new(
                "UtcDateTime",
                Type::Primitive(Primitive::I64),
            ))
            .with_record(event)
            .with_function(
                Function::new("echo_instant")
                    .with_param(Parameter::new("value", instant.clone()))
                    .with_output(instant),
            )
    }

    #[test]
    fn test_ffi_prefix() {
        use boltffi_ffi_rules::naming;
        assert_eq!(naming::ffi_prefix(), "boltffi");
    }

    #[test]
    fn test_class_ffi_names() {
        use boltffi_ffi_rules::naming;
        let module = create_test_module();
        let class = module.find_class("Sensor").unwrap();

        assert_eq!(
            naming::class_ffi_new(&class.name).as_str(),
            "boltffi_sensor_new"
        );
        assert_eq!(
            naming::class_ffi_free(&class.name).as_str(),
            "boltffi_sensor_free"
        );
    }

    #[test]
    fn test_method_ffi_names() {
        use boltffi_ffi_rules::naming;
        let module = create_test_module();
        let class = module.find_class("Sensor").unwrap();
        let method = class
            .methods
            .iter()
            .find(|m| m.name == "predict_next")
            .unwrap();

        assert_eq!(
            naming::method_ffi_name(&class.name, &method.name).as_str(),
            "boltffi_sensor_predict_next"
        );
        assert_eq!(
            naming::method_ffi_poll(&class.name, &method.name).as_str(),
            "boltffi_sensor_predict_next_poll"
        );
    }

    #[test]
    fn test_enum_detection() {
        let c_style = Enumeration::new("Status")
            .with_variant(Variant::new("ok"))
            .with_variant(Variant::new("error"));

        let data_enum = Enumeration::new("Result")
            .with_variant(
                Variant::new("success")
                    .with_field(RecordField::new("value", Type::Primitive(Primitive::I32))),
            )
            .with_variant(Variant::new("failure"));

        assert!(c_style.is_c_style());
        assert!(!data_enum.is_c_style());
        assert!(data_enum.is_data_enum());
    }
}
