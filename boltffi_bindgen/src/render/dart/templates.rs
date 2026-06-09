use askama::Template;

#[derive(Template)]
#[template(path = "render_dart/prelude.txt", escape = "none")]
pub struct PreludeTemplate {}

#[derive(Template)]
#[template(path = "render_dart/custom_types.txt", escape = "none")]
pub struct CustomTypesTemplate<'a> {
    pub custom_types: &'a [super::DartCustomType],
}

#[derive(Template)]
#[template(path = "render_dart/native_functions.txt", escape = "none")]
pub struct NativeFunctionsTemplate<'a> {
    pub cfuncs: &'a [super::DartNativeFunction],
}

#[derive(Template)]
#[template(path = "render_dart/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a super::DartRecord,
}

#[derive(Template)]
#[template(path = "render_dart/hook.build.dart.txt", escape = "none")]
pub struct BuildHookTemplate<'a> {
    pub artifact_name: &'a str,
}

#[derive(Template)]
#[template(path = "render_dart/pubspec.yaml.txt", escape = "none")]
pub struct PubspecTemplate<'a> {
    pub artifact_name: &'a str,
    pub description: Option<&'a str>,
    pub version: Option<&'a str>,
    pub repository: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "render_dart/enum.txt", escape = "none")]
pub struct EnhancedEnumTemplate<'a> {
    pub dart_enum: &'a super::DartEnum,
}

#[derive(Template)]
#[template(path = "render_dart/sealed_class_enum.txt", escape = "none")]
pub struct SealedClassEnumTemplate<'a> {
    pub dart_enum: &'a super::DartEnum,
}

#[derive(Template)]
#[template(path = "render_dart/callback.txt", escape = "none")]
pub struct CallbackTemplate<'a> {
    pub cb: &'a super::DartCallback,
}

#[derive(Template)]
#[template(path = "render_dart/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class: &'a super::DartClass,
}

#[cfg(test)]
mod tests {
    use boltffi_ffi_rules::callable::ExecutionKind;

    use crate::{
        ir::{
            self, CallbackId, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ClassId,
            ConstructorDef, FfiContract, MethodDef, MethodId, PackageInfo, ParamDef, ParamName,
            ParamPassing, PrimitiveType, Receiver, ReturnDef, StreamDef, StreamId, StreamMode,
            TypeExpr,
        },
        render::dart::{DartLibrary, DartLowerer},
    };

    use super::*;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn lower(ffi: &FfiContract) -> DartLibrary {
        let abi = ir::Lowerer::new(ffi).to_abi_contract();

        DartLowerer::new(ffi, &abi, "test").library()
    }

    fn generic_callback_def(kind: ExecutionKind) -> CallbackTraitDef {
        CallbackTraitDef {
            id: CallbackId::new("ICallback"),
            methods: vec![
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_u32"),
                    params: vec![ParamDef {
                        name: ParamName::new("n"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_string_ref"),
                    params: vec![ParamDef {
                        name: ParamName::new("s"),
                        type_expr: TypeExpr::String,
                        passing: ParamPassing::Ref,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::String),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("map_string"),
                    params: vec![ParamDef {
                        name: ParamName::new("s"),
                        type_expr: TypeExpr::String,
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Value(TypeExpr::String),
                    doc: None,
                },
                CallbackMethodDef {
                    execution_kind: kind,
                    id: MethodId::new("process_bytes"),
                    params: vec![ParamDef {
                        name: ParamName::new("bytes"),
                        type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
                        passing: ParamPassing::Value,
                        doc: None,
                    }],
                    returns: ReturnDef::Void,
                    doc: None,
                },
            ],
            kind: CallbackKind::Trait,
            doc: None,
        }
    }

    #[test]
    pub fn snapshot_sync_callback() {
        let mut ffi = empty_contract();
        ffi.catalog
            .insert_callback(generic_callback_def(ExecutionKind::Sync));
        let library = lower(&ffi);

        let template = CallbackTemplate {
            cb: &library.callbacks[0],
        };

        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    pub fn snapshot_async_callback() {
        let mut ffi = empty_contract();
        ffi.catalog
            .insert_callback(generic_callback_def(ExecutionKind::Async));
        let library = lower(&ffi);

        let template = CallbackTemplate {
            cb: &library.callbacks[0],
        };

        insta::assert_snapshot!(template.render().unwrap());
    }

    #[test]
    pub fn snapshot_class() {
        let mut ffi = empty_contract();

        let temperature_stream = |mode: StreamMode| StreamDef {
            id: StreamId::new(format!(
                "temperature_event_{}",
                match mode {
                    StreamMode::Async => "async",
                    StreamMode::Batch => "batch",
                    StreamMode::Callback => "callback",
                }
            )),
            item_type: TypeExpr::Primitive(PrimitiveType::F32),
            mode,
            doc: None,
            deprecated: None,
        };

        let names_stream = |mode: StreamMode| StreamDef {
            id: StreamId::new(format!(
                "names_event_{}",
                match mode {
                    StreamMode::Async => "async",
                    StreamMode::Batch => "batch",
                    StreamMode::Callback => "callback",
                }
            )),
            item_type: TypeExpr::String,
            mode,
            doc: None,
            deprecated: None,
        };

        let mut streams = vec![];
        streams.extend(
            [StreamMode::Async, StreamMode::Batch, StreamMode::Callback]
                .into_iter()
                .map(temperature_stream),
        );
        streams.extend(
            [StreamMode::Async, StreamMode::Batch, StreamMode::Callback]
                .into_iter()
                .map(names_stream),
        );

        ffi.catalog.insert_class(ClassDef {
            id: ClassId::new("Person"),
            constructors: vec![
                ConstructorDef::Default {
                    params: vec![
                        ParamDef {
                            name: ParamName::new("name"),
                            type_expr: TypeExpr::String,
                            passing: ParamPassing::Value,
                            doc: None,
                        },
                        ParamDef {
                            name: ParamName::new("age"),
                            type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                            passing: ParamPassing::Value,
                            doc: None,
                        },
                    ],
                    is_fallible: false,
                    is_optional: false,
                    doc: None,
                    deprecated: None,
                },
                ConstructorDef::NamedInit {
                    name: MethodId::new("new_with_default_age"),
                    is_fallible: false,
                    is_optional: false,
                    doc: None,
                    deprecated: None,
                    first_param: ParamDef {
                        name: ParamName::new("name"),
                        type_expr: TypeExpr::String,
                        passing: ParamPassing::Value,
                        doc: None,
                    },
                    rest_params: vec![],
                },
            ],
            methods: vec![MethodDef {
                id: MethodId::new("get_name"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            }],
            streams,
            doc: None,
            deprecated: None,
        });
        let library = lower(&ffi);

        let template = ClassTemplate {
            class: &library.classes[0],
        };

        insta::assert_snapshot!(template.render().unwrap());
    }
}
