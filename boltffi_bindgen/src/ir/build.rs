use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
use crate::ir::definitions::{
    CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef,
    CustomTypeDef, DataVariant, DefaultValue, DeprecationInfo, EnumDef, EnumRepr, FieldDef,
    FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, StreamDef,
    StreamMode, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, ConverterPath, CustomTypeId, EnumId, FieldName, FunctionId,
    MethodId, ParamName, QualifiedName, RecordId, StreamId, VariantName,
};
use crate::ir::types::{BuiltinDef, BuiltinKind, PrimitiveType, TypeExpr};
use crate::model::{self, Module};
use boltffi_ffi_rules::callable::ExecutionKind;

pub struct ContractBuilder<'m> {
    module: &'m Module,
}

impl<'m> ContractBuilder<'m> {
    pub fn new(module: &'m Module) -> Self {
        Self { module }
    }

    pub fn build(&self) -> FfiContract {
        let mut catalog = TypeCatalog::new();

        self.module
            .records
            .iter()
            .map(|r| self.convert_record(r))
            .for_each(|r| catalog.insert_record(r));

        self.module
            .enums
            .iter()
            .map(|e| self.convert_enum(e))
            .for_each(|e| catalog.insert_enum(e));

        self.module
            .classes
            .iter()
            .map(|c| self.convert_class(c))
            .for_each(|c| catalog.insert_class(c));

        self.module
            .callback_traits
            .iter()
            .map(|cb| self.convert_callback_trait(cb))
            .for_each(|cb| catalog.insert_callback(cb));

        self.module
            .custom_types
            .iter()
            .map(|ct| self.convert_custom_type(ct))
            .for_each(|ct| catalog.insert_custom(ct));

        let mut builtin_ids: Vec<_> = self.module.used_builtins.iter().collect();
        builtin_ids.sort_by_key(|id| id.type_id());
        builtin_ids
            .into_iter()
            .map(|id| convert_builtin_id(*id))
            .for_each(|b| catalog.insert_builtin(b));

        let mut closure_entries: Vec<_> = self.module.closures.iter().collect();
        closure_entries.sort_by_key(|(id, _)| *id);
        closure_entries
            .into_iter()
            .map(|(sig_id, sig)| self.convert_closure_to_callback(sig_id, sig))
            .for_each(|cb| catalog.insert_callback(cb));

        let functions = self
            .module
            .functions
            .iter()
            .map(|f| self.convert_function(f))
            .collect();

        FfiContract {
            package: PackageInfo {
                name: self.module.name.clone(),
                version: None,
            },
            catalog,
            functions,
        }
    }

    fn convert_record(&self, record: &model::Record) -> RecordDef {
        let constructors = record
            .constructors
            .iter()
            .map(|ctor| self.convert_constructor(ctor, true))
            .collect();

        RecordDef {
            id: RecordId::new(&record.name),
            is_repr_c: record.is_repr_c,
            is_error: record.is_error,
            fields: record
                .fields
                .iter()
                .map(|f| {
                    let type_expr = Self::convert_type(&f.field_type);
                    let default =
                        f.default_value
                            .as_deref()
                            .map(parse_default_value)
                            .or_else(|| {
                                matches!(type_expr, TypeExpr::Option(_))
                                    .then_some(DefaultValue::Null)
                            });
                    FieldDef {
                        name: FieldName::new(&f.name),
                        type_expr,
                        doc: f.doc.clone(),
                        default,
                    }
                })
                .collect(),
            constructors,
            methods: record
                .methods
                .iter()
                .map(|m| self.convert_method(m))
                .collect(),
            doc: record.doc.clone(),
            deprecated: record.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_enum(&self, enumeration: &model::Enumeration) -> EnumDef {
        let tag_type = enumeration.repr_type.unwrap_or(PrimitiveType::I32);
        let repr = if enumeration.is_c_style() {
            EnumRepr::CStyle {
                tag_type,
                variants: enumeration
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(idx, v)| CStyleVariant {
                        name: VariantName::new(&v.name),
                        discriminant: v.discriminant.unwrap_or(idx as i128),
                        doc: v.doc.clone(),
                    })
                    .collect(),
            }
        } else {
            EnumRepr::Data {
                tag_type,
                variants: enumeration
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(idx, v)| DataVariant {
                        name: VariantName::new(&v.name),
                        discriminant: v.discriminant.unwrap_or(idx as i128),
                        payload: self.convert_variant_payload(&v.fields),
                        doc: v.doc.clone(),
                    })
                    .collect(),
            }
        };

        let enum_constructors = enumeration
            .constructors
            .iter()
            .map(|ctor| self.convert_constructor(ctor, true))
            .collect();

        EnumDef {
            id: EnumId::new(&enumeration.name),
            repr,
            is_error: enumeration.is_error,
            constructors: enum_constructors,
            methods: enumeration
                .methods
                .iter()
                .map(|m| self.convert_method(m))
                .collect(),
            doc: enumeration.doc.clone(),
            deprecated: enumeration.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_variant_payload(&self, fields: &[model::RecordField]) -> VariantPayload {
        if fields.is_empty() {
            VariantPayload::Unit
        } else if fields
            .iter()
            .enumerate()
            .all(|(i, f)| f.name == format!("value_{i}"))
        {
            VariantPayload::Tuple(
                fields
                    .iter()
                    .map(|f| Self::convert_type(&f.field_type))
                    .collect(),
            )
        } else {
            VariantPayload::Struct(
                fields
                    .iter()
                    .map(|f| FieldDef {
                        name: FieldName::new(&f.name),
                        type_expr: Self::convert_type(&f.field_type),
                        doc: f.doc.clone(),
                        default: None,
                    })
                    .collect(),
            )
        }
    }

    fn convert_function(&self, func: &model::Function) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(&func.name),
            params: func
                .inputs
                .iter()
                .map(|p| self.convert_param(&p.name, &p.param_type))
                .collect(),
            returns: self.convert_return_type(&func.returns),
            execution_kind: func.execution_kind,
            doc: func.doc.clone(),
            deprecated: func.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_class(&self, class: &model::Class) -> ClassDef {
        ClassDef {
            id: ClassId::new(&class.name),
            constructors: {
                let has_default_init = class.constructors.iter().any(|c| c.name == "new");
                let mut promoted_no_param = has_default_init;
                class
                    .constructors
                    .iter()
                    .map(|ctor| {
                        let result = self.convert_constructor(ctor, promoted_no_param);
                        if ctor.name != "new" && ctor.inputs.is_empty() && !promoted_no_param {
                            promoted_no_param = true;
                        }
                        result
                    })
                    .collect()
            },
            methods: class
                .methods
                .iter()
                .map(|m| self.convert_method(m))
                .collect(),
            streams: class
                .streams
                .iter()
                .map(|s| self.convert_stream(s))
                .collect(),
            doc: class.doc.clone(),
            deprecated: class.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_stream(&self, stream: &model::StreamMethod) -> StreamDef {
        StreamDef {
            id: StreamId::new(&stream.name),
            item_type: Self::convert_type(&stream.item_type),
            mode: match stream.mode {
                model::StreamMode::Async => StreamMode::Async,
                model::StreamMode::Batch => StreamMode::Batch,
                model::StreamMode::Callback => StreamMode::Callback,
            },
            doc: stream.doc.clone(),
            deprecated: stream.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_constructor(
        &self,
        ctor: &model::Constructor,
        has_default_init: bool,
    ) -> ConstructorDef {
        let params: Vec<_> = ctor
            .inputs
            .iter()
            .map(|p| self.convert_param(&p.name, &p.param_type))
            .collect();

        // When there's no `new()`, a no-param named ctor like `with_defaults()`
        // would normally become a static factory (`static func withDefaults()`).
        // But if nothing else claims the default init slot, there's no reason
        // not to give it `public init()` — it's cleaner for the caller.
        // We only promote the first one to avoid duplicate init() signatures.
        if ctor.name == "new" || (!has_default_init && params.is_empty()) {
            ConstructorDef::Default {
                params,
                is_fallible: ctor.is_fallible,
                is_optional: ctor.is_optional,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        } else if params.is_empty() {
            ConstructorDef::NamedFactory {
                name: MethodId::new(&ctor.name),
                is_fallible: ctor.is_fallible,
                is_optional: ctor.is_optional,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        } else {
            let mut params_iter = params.into_iter();
            let first_param = params_iter.next().expect("params is non-empty");
            ConstructorDef::NamedInit {
                name: MethodId::new(&ctor.name),
                first_param,
                rest_params: params_iter.collect(),
                is_fallible: ctor.is_fallible,
                is_optional: ctor.is_optional,
                doc: ctor.doc.clone(),
                deprecated: None,
            }
        }
    }

    fn convert_method(&self, method: &model::Method) -> MethodDef {
        MethodDef {
            id: MethodId::new(&method.name),
            receiver: convert_receiver(method.receiver),
            params: method
                .inputs
                .iter()
                .map(|p| self.convert_param(&p.name, &p.param_type))
                .collect(),
            returns: self.convert_return_type(&method.returns),
            execution_kind: method.execution_kind,
            doc: method.doc.clone(),
            deprecated: method.deprecated.as_ref().map(convert_deprecation),
        }
    }

    fn convert_callback_trait(&self, cb: &model::CallbackTrait) -> CallbackTraitDef {
        CallbackTraitDef {
            id: CallbackId::new(&cb.name),
            methods: cb
                .methods
                .iter()
                .map(|m| CallbackMethodDef {
                    id: MethodId::new(&m.name),
                    params: m
                        .inputs
                        .iter()
                        .map(|p| self.convert_param(&p.name, &p.param_type))
                        .collect(),
                    returns: self.convert_return_type(&m.returns),
                    execution_kind: m.execution_kind,
                    doc: m.doc.clone(),
                })
                .collect(),
            kind: CallbackKind::Trait,
            doc: cb.doc.clone(),
        }
    }

    fn convert_custom_type(&self, ct: &model::CustomType) -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new(&ct.name),
            rust_type: QualifiedName::new(&ct.name),
            repr: Self::convert_type(&ct.repr),
            converters: ConverterPath {
                into_ffi: QualifiedName::new(format!("{}::into_ffi", ct.name)),
                try_from_ffi: QualifiedName::new(format!("{}::try_from_ffi", ct.name)),
            },
            doc: None,
        }
    }

    fn convert_closure_to_callback(
        &self,
        sig_id: &str,
        sig: &model::ClosureSignature,
    ) -> CallbackTraitDef {
        let params = sig
            .params
            .iter()
            .enumerate()
            .map(|(idx, ty)| {
                let (type_expr, passing) = self.convert_type_with_passing(ty);
                ParamDef {
                    name: ParamName::new(format!("arg{}", idx)),
                    type_expr,
                    passing,
                    doc: None,
                }
            })
            .collect();

        let returns = self.convert_return_type(&model::ReturnType::from_output(
            sig.returns.as_ref().clone(),
        ));

        CallbackTraitDef {
            id: CallbackId::new(sig_id),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params,
                returns,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        }
    }

    fn convert_param(&self, name: &str, ty: &model::Type) -> ParamDef {
        let (type_expr, passing) = self.convert_type_with_passing(ty);
        ParamDef {
            name: ParamName::new(name),
            type_expr,
            passing,
            doc: None,
        }
    }

    fn convert_type_with_passing(&self, ty: &model::Type) -> (TypeExpr, ParamPassing) {
        match ty {
            model::Type::Slice(inner) => (
                TypeExpr::Vec(Box::new(Self::convert_type(inner))),
                ParamPassing::Ref,
            ),
            model::Type::MutSlice(inner) => (
                TypeExpr::Vec(Box::new(Self::convert_type(inner))),
                ParamPassing::RefMut,
            ),
            model::Type::BoxedTrait(name) => (
                TypeExpr::Callback(CallbackId::new(name)),
                ParamPassing::BoxedDyn,
            ),
            model::Type::Closure(sig) => {
                let sig_id = format!("__Closure_{}", sig.signature_id());
                (
                    TypeExpr::Callback(CallbackId::new(&sig_id)),
                    ParamPassing::ImplTrait,
                )
            }
            _ => (Self::convert_type(ty), ParamPassing::Value),
        }
    }

    fn convert_type(ty: &model::Type) -> TypeExpr {
        match ty {
            model::Type::Primitive(p) => TypeExpr::Primitive(*p),
            model::Type::String => TypeExpr::String,
            model::Type::Builtin(id) => TypeExpr::Builtin(BuiltinId::new(id.type_id())),
            model::Type::Vec(inner) => TypeExpr::Vec(Box::new(Self::convert_type(inner))),
            model::Type::Option(inner) => TypeExpr::Option(Box::new(Self::convert_type(inner))),
            model::Type::Result { ok, err } => TypeExpr::Result {
                ok: Box::new(Self::convert_type(ok)),
                err: Box::new(Self::convert_type(err)),
            },
            model::Type::Record(name) => TypeExpr::Record(RecordId::new(name)),
            model::Type::Enum(name) => TypeExpr::Enum(EnumId::new(name)),
            model::Type::Object(name) => TypeExpr::Handle(ClassId::new(name)),
            model::Type::Custom { name, .. } => TypeExpr::Custom(CustomTypeId::new(name)),
            model::Type::BoxedTrait(name) => TypeExpr::Callback(CallbackId::new(name)),
            model::Type::Closure(sig) => {
                let sig_id = format!("__Closure_{}", sig.signature_id());
                TypeExpr::Callback(CallbackId::new(&sig_id))
            }
            model::Type::Slice(inner) | model::Type::MutSlice(inner) => {
                TypeExpr::Vec(Box::new(Self::convert_type(inner)))
            }
            model::Type::Void => TypeExpr::Void,
        }
    }

    fn convert_return_type(&self, ret: &model::ReturnType) -> ReturnDef {
        match ret {
            model::ReturnType::Void => ReturnDef::Void,
            model::ReturnType::Value(ty) => {
                if ty.is_void() {
                    ReturnDef::Void
                } else {
                    ReturnDef::Value(Self::convert_type(ty))
                }
            }
            model::ReturnType::Fallible { ok, err } => ReturnDef::Result {
                ok: Self::convert_type(ok),
                err: Self::convert_type(err),
            },
        }
    }
}

fn parse_default_value(raw: &str) -> DefaultValue {
    match raw {
        "true" => DefaultValue::Bool(true),
        "false" => DefaultValue::Bool(false),
        "None" => DefaultValue::Null,
        s if s.starts_with('"') && s.ends_with('"') => {
            DefaultValue::String(s[1..s.len() - 1].to_string())
        }
        s if s.contains("::") => {
            let (enum_name, variant_name) = s.rsplit_once("::").expect("contains ::");
            DefaultValue::EnumVariant {
                enum_name: enum_name.to_string(),
                variant_name: variant_name.to_string(),
            }
        }
        s if s.contains('.') => DefaultValue::Float(s.parse().unwrap_or(0.0)),
        s => DefaultValue::Integer(s.parse().unwrap_or(0)),
    }
}

fn convert_receiver(r: model::Receiver) -> Receiver {
    match r {
        model::Receiver::None => Receiver::Static,
        model::Receiver::Ref => Receiver::RefSelf,
        model::Receiver::RefMut => Receiver::RefMutSelf,
    }
}

fn convert_deprecation(d: &model::Deprecation) -> DeprecationInfo {
    DeprecationInfo {
        message: d.message.clone(),
        since: d.since.clone(),
    }
}

fn convert_builtin_id(id: model::BuiltinId) -> BuiltinDef {
    let (kind, rust_type) = match id {
        model::BuiltinId::Duration => (BuiltinKind::Duration, "std::time::Duration"),
        model::BuiltinId::SystemTime => (BuiltinKind::SystemTime, "std::time::SystemTime"),
        model::BuiltinId::Uuid => (BuiltinKind::Uuid, "uuid::Uuid"),
        model::BuiltinId::Url => (BuiltinKind::Url, "url::Url"),
    };
    BuiltinDef {
        id: BuiltinId::new(id.type_id()),
        kind,
        rust_type: QualifiedName::new(rust_type),
    }
}

pub fn build_contract(module: &mut Module) -> FfiContract {
    module.collect_derived_types();
    ContractBuilder::new(module).build()
}

#[cfg(test)]
mod tests {
    use crate::ir::definitions::{
        CallbackKind, ConstructorDef, DefaultValue, EnumRepr, ParamPassing, Receiver as IrReceiver,
        ReturnDef, VariantPayload,
    };

    use super::parse_default_value;
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use crate::model::{
        self, CallbackTrait, Enumeration, Module, Parameter, Primitive, Receiver, Record,
        RecordField, ReturnType, TraitMethod, TraitMethodParam, Type, Variant,
    };

    use super::ContractBuilder;

    fn empty_module() -> Module {
        Module {
            name: "test".to_string(),
            records: vec![],
            enums: vec![],
            classes: vec![],
            callback_traits: vec![],
            functions: vec![],
            custom_types: vec![],
            closures: Default::default(),
            used_builtins: Default::default(),
        }
    }

    fn builder(module: &Module) -> ContractBuilder<'_> {
        ContractBuilder::new(module)
    }

    #[test]
    fn record_fields_and_docs_propagate() {
        let mut module = empty_module();
        module.records.push(
            Record::new("Location")
                .with_doc("A geographic point.")
                .with_field(
                    RecordField::new("lat", Type::Primitive(Primitive::F64))
                        .with_doc("Latitude in degrees."),
                )
                .with_field(RecordField::new("lng", Type::Primitive(Primitive::F64))),
        );

        let def = builder(&module).convert_record(&module.records[0]);

        assert_eq!(def.id.as_str(), "Location");
        assert_eq!(def.doc.as_deref(), Some("A geographic point."));
        assert_eq!(def.fields.len(), 2);
        assert_eq!(def.fields[0].name.as_str(), "lat");
        assert_eq!(def.fields[0].doc.as_deref(), Some("Latitude in degrees."));
        assert!(matches!(
            def.fields[0].type_expr,
            TypeExpr::Primitive(PrimitiveType::F64)
        ));
        assert_eq!(def.fields[1].name.as_str(), "lng");
        assert!(def.fields[1].doc.is_none());
    }

    #[test]
    fn record_without_doc_has_none() {
        let mut module = empty_module();
        module.records.push(Record::new("Bare"));

        let def = builder(&module).convert_record(&module.records[0]);

        assert!(def.doc.is_none());
        assert!(def.fields.is_empty());
    }

    #[test]
    fn c_style_enum_variants_and_docs() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("Direction")
                .with_doc("Cardinal direction.")
                .with_variant(Variant::new("North").with_doc("Toward the north pole."))
                .with_variant(Variant::new("South")),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        assert_eq!(def.id.as_str(), "Direction");
        assert_eq!(def.doc.as_deref(), Some("Cardinal direction."));
        assert!(!def.is_error);
        let variants = match &def.repr {
            EnumRepr::CStyle { variants, .. } => variants,
            _ => panic!("expected c-style"),
        };
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name.as_str(), "North");
        assert_eq!(variants[0].doc.as_deref(), Some("Toward the north pole."));
        assert_eq!(variants[0].discriminant, 0);
        assert_eq!(variants[1].name.as_str(), "South");
        assert!(variants[1].doc.is_none());
        assert_eq!(variants[1].discriminant, 1);
    }

    #[test]
    fn data_enum_with_variant_fields() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("ApiResult")
                .with_variant(Variant::new("Ok"))
                .with_variant(
                    Variant::new("Error")
                        .with_field(RecordField::new("code", Type::Primitive(Primitive::I32)))
                        .with_doc("Something went wrong."),
                ),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        let variants = match &def.repr {
            EnumRepr::Data { variants, .. } => variants,
            _ => panic!("expected data enum"),
        };
        assert_eq!(variants.len(), 2);
        assert!(matches!(variants[0].payload, VariantPayload::Unit));
        match &variants[1].payload {
            VariantPayload::Struct(fields) => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name.as_str(), "code");
            }
            _ => panic!("expected struct payload"),
        }
        assert_eq!(variants[1].doc.as_deref(), Some("Something went wrong."));
    }

    #[test]
    fn tuple_variant_fields_produce_tuple_payload() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("LocationBias")
                .with_variant(
                    Variant::new("Left")
                        .with_field(RecordField::new("value_0", Type::Primitive(Primitive::F64))),
                )
                .with_variant(
                    Variant::new("Right")
                        .with_field(RecordField::new("value_0", Type::Primitive(Primitive::F64))),
                ),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        let variants = match &def.repr {
            EnumRepr::Data { variants, .. } => variants,
            _ => panic!("expected data enum"),
        };
        assert_eq!(variants.len(), 2);
        match &variants[0].payload {
            VariantPayload::Tuple(types) => {
                assert_eq!(types.len(), 1);
                assert!(matches!(types[0], TypeExpr::Primitive(PrimitiveType::F64)));
            }
            other => panic!("expected tuple payload, got {:?}", other),
        }
        match &variants[1].payload {
            VariantPayload::Tuple(types) => {
                assert_eq!(types.len(), 1);
                assert!(matches!(types[0], TypeExpr::Primitive(PrimitiveType::F64)));
            }
            other => panic!("expected tuple payload, got {:?}", other),
        }
    }

    #[test]
    fn multi_field_tuple_variant_produces_tuple_payload() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("Value").with_variant(
                Variant::new("Pair")
                    .with_field(RecordField::new("value_0", Type::Primitive(Primitive::I32)))
                    .with_field(RecordField::new("value_1", Type::String)),
            ),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        let variants = match &def.repr {
            EnumRepr::Data { variants, .. } => variants,
            _ => panic!("expected data enum"),
        };
        match &variants[0].payload {
            VariantPayload::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert!(matches!(types[0], TypeExpr::Primitive(PrimitiveType::I32)));
                assert!(matches!(types[1], TypeExpr::String));
            }
            other => panic!("expected tuple payload, got {:?}", other),
        }
    }

    #[test]
    fn named_fields_produce_struct_payload() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("Event").with_variant(
                Variant::new("Click")
                    .with_field(RecordField::new("x", Type::Primitive(Primitive::I32)))
                    .with_field(RecordField::new("y", Type::Primitive(Primitive::I32))),
            ),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        let variants = match &def.repr {
            EnumRepr::Data { variants, .. } => variants,
            _ => panic!("expected data enum"),
        };
        match &variants[0].payload {
            VariantPayload::Struct(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name.as_str(), "x");
                assert_eq!(fields[1].name.as_str(), "y");
            }
            other => panic!("expected struct payload, got {:?}", other),
        }
    }

    #[test]
    fn error_enum_flag_propagates() {
        let mut module = empty_module();
        module.enums.push(
            Enumeration::new("ParseError")
                .as_error()
                .with_variant(Variant::new("InvalidSyntax")),
        );

        let def = builder(&module).convert_enum(&module.enums[0]);

        assert!(def.is_error);
    }

    #[test]
    fn function_params_returns_and_doc() {
        let mut module = empty_module();
        module.functions.push(
            model::Function::new("add")
                .with_doc("Adds two numbers.")
                .with_param(Parameter::new("a", Type::Primitive(Primitive::I32)))
                .with_param(Parameter::new("b", Type::Primitive(Primitive::I32)))
                .with_return(ReturnType::value(Type::Primitive(Primitive::I64))),
        );

        let def = builder(&module).convert_function(&module.functions[0]);

        assert_eq!(def.id.as_str(), "add");
        assert_eq!(def.doc.as_deref(), Some("Adds two numbers."));
        assert!(!def.is_async());
        assert_eq!(def.params.len(), 2);
        assert_eq!(def.params[0].name.as_str(), "a");
        assert!(matches!(
            def.params[0].type_expr,
            TypeExpr::Primitive(PrimitiveType::I32)
        ));
        assert!(matches!(def.params[0].passing, ParamPassing::Value));
        assert!(matches!(
            def.returns,
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64))
        ));
    }

    #[test]
    fn async_function_preserves_flag() {
        let mut module = empty_module();
        module
            .functions
            .push(model::Function::new("fetch").make_async());

        let def = builder(&module).convert_function(&module.functions[0]);

        assert!(def.is_async());
    }

    #[test]
    fn void_function_returns_void() {
        let mut module = empty_module();
        module.functions.push(model::Function::new("noop"));

        let def = builder(&module).convert_function(&module.functions[0]);

        assert!(matches!(def.returns, ReturnDef::Void));
    }

    #[test]
    fn class_method_with_receiver_and_doc() {
        let mut module = empty_module();
        module.classes.push(
            model::Class::new("Counter")
                .with_constructor(model::Constructor::new())
                .with_method(
                    model::Method::new("increment", Receiver::RefMut)
                        .with_doc("Bumps the counter by one.")
                        .with_param(Parameter::new("amount", Type::Primitive(Primitive::I32)))
                        .with_return(ReturnType::value(Type::Primitive(Primitive::I64))),
                ),
        );

        let b = builder(&module);
        let def = b.convert_class(&module.classes[0]);

        assert_eq!(def.methods.len(), 1);
        let method = &def.methods[0];
        assert_eq!(method.id.as_str(), "increment");
        assert_eq!(method.doc.as_deref(), Some("Bumps the counter by one."));
        assert!(matches!(method.receiver, IrReceiver::RefMutSelf));
        assert_eq!(method.params.len(), 1);
        assert!(matches!(
            method.returns,
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64))
        ));
    }

    #[test]
    fn class_doc_propagates() {
        let mut module = empty_module();
        module
            .classes
            .push(model::Class::new("Store").with_doc("A persistent store."));

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(def.doc.as_deref(), Some("A persistent store."));
    }

    #[test]
    fn named_ctor_with_params_becomes_named_init() {
        let mut module = empty_module();
        module.classes.push(
            model::Class::new("Buffer").with_constructor(
                model::Constructor::new()
                    .with_name("with_capacity")
                    .with_param(model::ConstructorParam {
                        name: "size".to_string(),
                        param_type: Type::Primitive(Primitive::U32),
                    }),
            ),
        );

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(def.constructors.len(), 1);
        assert!(
            matches!(&def.constructors[0], ConstructorDef::NamedInit { name, .. } if name.as_str() == "with_capacity")
        );
    }

    #[test]
    fn constructor_doc_propagates() {
        let mut module = empty_module();
        module
            .classes
            .push(model::Class::new("Db").with_constructor(
                model::Constructor::new().with_doc("Opens a new database connection."),
            ));

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(
            def.constructors[0].doc(),
            Some("Opens a new database connection.")
        );
    }

    #[test]
    fn named_no_param_ctor_promoted_to_default_when_no_new() {
        let mut module = empty_module();
        module.classes.push(
            model::Class::new("Store")
                .with_constructor(model::Constructor::new().with_name("with_defaults")),
        );

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(def.constructors.len(), 1);
        assert!(
            matches!(&def.constructors[0], ConstructorDef::Default { params, .. } if params.is_empty())
        );
    }

    #[test]
    fn named_no_param_ctor_stays_factory_when_new_exists() {
        let mut module = empty_module();
        module.classes.push(
            model::Class::new("Store")
                .with_constructor(model::Constructor::new())
                .with_constructor(model::Constructor::new().with_name("with_defaults")),
        );

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(def.constructors.len(), 2);
        assert!(matches!(
            &def.constructors[0],
            ConstructorDef::Default { .. }
        ));
        assert!(matches!(
            &def.constructors[1],
            ConstructorDef::NamedFactory { .. }
        ));
    }

    #[test]
    fn only_first_no_param_ctor_promoted_when_no_new() {
        let mut module = empty_module();
        module.classes.push(
            model::Class::new("Store")
                .with_constructor(model::Constructor::new().with_name("with_defaults"))
                .with_constructor(model::Constructor::new().with_name("empty")),
        );

        let def = builder(&module).convert_class(&module.classes[0]);

        assert_eq!(def.constructors.len(), 2);
        assert!(matches!(
            &def.constructors[0],
            ConstructorDef::Default { .. }
        ));
        assert!(matches!(
            &def.constructors[1],
            ConstructorDef::NamedFactory { .. }
        ));
    }

    #[test]
    fn callback_trait_methods_and_docs() {
        let mut module = empty_module();
        module.callback_traits.push(
            CallbackTrait::new("DataProvider")
                .with_doc("Supplies data to the engine.")
                .with_method(
                    TraitMethod::new("fetch")
                        .with_doc("Fetches the next batch.")
                        .with_param(TraitMethodParam::new(
                            "count",
                            Type::Primitive(Primitive::I32),
                        ))
                        .with_return(ReturnType::value(Type::Primitive(Primitive::Bool))),
                )
                .with_method(TraitMethod::new("reset").make_async()),
        );

        let def = builder(&module).convert_callback_trait(&module.callback_traits[0]);

        assert_eq!(def.id.as_str(), "DataProvider");
        assert_eq!(def.doc.as_deref(), Some("Supplies data to the engine."));
        assert!(matches!(def.kind, CallbackKind::Trait));
        assert_eq!(def.methods.len(), 2);

        let fetch = &def.methods[0];
        assert_eq!(fetch.id.as_str(), "fetch");
        assert_eq!(fetch.doc.as_deref(), Some("Fetches the next batch."));
        assert!(!fetch.is_async());
        assert_eq!(fetch.params.len(), 1);
        assert!(matches!(
            fetch.returns,
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool))
        ));

        let reset = &def.methods[1];
        assert_eq!(reset.id.as_str(), "reset");
        assert!(reset.is_async());
        assert!(matches!(reset.returns, ReturnDef::Void));
    }

    #[test]
    fn closure_result_return_becomes_fallible_callback_return() {
        let mut module = empty_module();
        module.enums.push(Enumeration::new("MathError").as_error());
        module.functions.push(
            model::Function::new("apply_result_closure")
                .with_param(Parameter::new(
                    "f",
                    Type::Closure(model::ClosureSignature::new(
                        vec![Type::Primitive(Primitive::I32)],
                        Type::result(
                            Type::Primitive(Primitive::I32),
                            Type::Enum("MathError".into()),
                        ),
                    )),
                ))
                .with_param(Parameter::new("value", Type::Primitive(Primitive::I32)))
                .with_return(ReturnType::fallible(
                    Type::Primitive(Primitive::I32),
                    Type::Enum("MathError".into()),
                )),
        );

        let contract = super::build_contract(&mut module);
        let closure_callback = contract
            .catalog
            .all_callbacks()
            .find(|callback| matches!(callback.kind, CallbackKind::Closure))
            .expect("closure callback");

        assert!(matches!(
            closure_callback.methods[0].returns,
            ReturnDef::Result {
                ok: TypeExpr::Primitive(PrimitiveType::I32),
                err: TypeExpr::Enum(_),
            }
        ));
    }

    #[test]
    fn full_contract_build_integrates_all_types() {
        let mut module = empty_module();
        module.records.push(
            Record::new("Point")
                .with_field(RecordField::new("x", Type::Primitive(Primitive::F64)))
                .with_field(RecordField::new("y", Type::Primitive(Primitive::F64))),
        );
        module.enums.push(
            Enumeration::new("Color")
                .with_variant(Variant::new("Red"))
                .with_variant(Variant::new("Green"))
                .with_variant(Variant::new("Blue")),
        );
        module.classes.push(
            model::Class::new("Canvas")
                .with_constructor(model::Constructor::new())
                .with_method(model::Method::new("draw", Receiver::RefMut)),
        );
        module.functions.push(
            model::Function::new("distance")
                .with_param(Parameter::new("a", Type::Record("Point".into())))
                .with_param(Parameter::new("b", Type::Record("Point".into())))
                .with_return(ReturnType::value(Type::Primitive(Primitive::F64))),
        );
        module
            .callback_traits
            .push(CallbackTrait::new("Renderer").with_method(TraitMethod::new("render")));

        let contract = builder(&module).build();

        assert!(contract.catalog.resolve_record(&"Point".into()).is_some());
        assert!(contract.catalog.resolve_enum(&"Color".into()).is_some());
        assert!(contract.catalog.resolve_class(&"Canvas".into()).is_some());
        assert_eq!(contract.functions.len(), 1);
        assert_eq!(contract.catalog.all_callbacks().count(), 1);
    }

    #[test]
    fn parse_default_bool_true() {
        assert!(matches!(
            parse_default_value("true"),
            DefaultValue::Bool(true)
        ));
    }

    #[test]
    fn parse_default_bool_false() {
        assert!(matches!(
            parse_default_value("false"),
            DefaultValue::Bool(false)
        ));
    }

    #[test]
    fn parse_default_integer() {
        match parse_default_value("42") {
            DefaultValue::Integer(v) => assert_eq!(v, 42),
            other => panic!("expected Integer, got {:?}", other),
        }
    }

    #[test]
    fn parse_default_negative_integer() {
        match parse_default_value("-7") {
            DefaultValue::Integer(v) => assert_eq!(v, -7),
            other => panic!("expected Integer, got {:?}", other),
        }
    }

    #[test]
    fn parse_default_float() {
        match parse_default_value("2.5") {
            DefaultValue::Float(v) => assert!((v - 2.5).abs() < f64::EPSILON),
            other => panic!("expected Float, got {:?}", other),
        }
    }

    #[test]
    fn parse_default_string() {
        match parse_default_value("\"hello\"") {
            DefaultValue::String(v) => assert_eq!(v, "hello"),
            other => panic!("expected String, got {:?}", other),
        }
    }

    #[test]
    fn parse_default_enum_variant() {
        match parse_default_value("Direction::North") {
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "Direction");
                assert_eq!(variant_name, "North");
            }
            other => panic!("expected EnumVariant, got {:?}", other),
        }
    }

    #[test]
    fn parse_default_none_becomes_null() {
        assert!(matches!(parse_default_value("None"), DefaultValue::Null));
    }

    #[test]
    fn option_field_auto_defaults_to_null() {
        let mut module = empty_module();
        module.records.push(
            Record::new("Config")
                .with_field(RecordField::new("name", Type::Primitive(Primitive::I32)))
                .with_field(RecordField::new(
                    "label",
                    Type::Option(Box::new(Type::String)),
                )),
        );

        let def = builder(&module).convert_record(&module.records[0]);

        assert!(def.fields[0].default.is_none());
        assert!(matches!(def.fields[1].default, Some(DefaultValue::Null)));
    }

    #[test]
    fn explicit_default_overrides_option_auto() {
        let mut module = empty_module();
        module.records.push(
            Record::new("Config").with_field(
                RecordField::new("label", Type::Option(Box::new(Type::String)))
                    .with_default("\"unnamed\""),
            ),
        );

        let def = builder(&module).convert_record(&module.records[0]);

        match &def.fields[0].default {
            Some(DefaultValue::String(v)) => assert_eq!(v, "unnamed"),
            other => panic!("expected String default, got {:?}", other),
        }
    }

    #[test]
    fn required_field_has_no_default() {
        let mut module = empty_module();
        module.records.push(
            Record::new("Point")
                .with_field(RecordField::new("x", Type::Primitive(Primitive::F64)))
                .with_field(RecordField::new("y", Type::Primitive(Primitive::F64))),
        );

        let def = builder(&module).convert_record(&module.records[0]);

        assert!(def.fields[0].default.is_none());
        assert!(def.fields[1].default.is_none());
    }
}
