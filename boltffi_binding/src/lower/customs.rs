//! Custom type declaration lowering.
//!
//! Walks every [`CustomTypeDef`] the source contract exposes and
//! produces a [`CustomTypeDecl`] that names the custom type and records
//! the binding [`TypeRef`] foreign code observes when the custom type
//! appears as a field, parameter, or return.
//!
//! The source carries converter functions between the remote Rust type
//! and the chosen FFI representation. Lowering keeps that conversion
//! contract on the custom declaration so every consumer reads the same
//! boundary rule.
//!
//! Custom-type references already lower transparently through
//! [`super::types::lower`] for `TypeExpr::Custom(id)`; this pass adds
//! the declaration itself so the contract advertises the named type
//! alongside its representation.
//!
//! [`CustomTypeDef`]: boltffi_ast::CustomTypeDef
//! [`CustomTypeDecl`]: crate::CustomTypeDecl
//! [`TypeRef`]: crate::TypeRef

use boltffi_ast::{
    CustomTypeConverter as SourceConverter, CustomTypeConverters as SourceConverters,
    CustomTypeDef as SourceCustom, Path as SourcePath, PathRoot as SourcePathRoot,
};

use crate::{
    CanonicalName, CustomConverterExpression, CustomConverterPath, CustomConverterPathRoot,
    CustomTypeConverter, CustomTypeConverters, CustomTypeDecl, NamePart,
};

use super::{
    LowerError, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata, types,
};

pub fn lower(index: &Index, ids: &DeclarationIds) -> Result<Vec<CustomTypeDecl>, LowerError> {
    index
        .customs()
        .iter()
        .map(|custom| lower_one(ids, custom))
        .collect()
}

fn lower_one(ids: &DeclarationIds, custom: &SourceCustom) -> Result<CustomTypeDecl, LowerError> {
    let custom_id = ids.custom(&custom.id)?;
    let representation = types::lower(ids, &custom.repr)?;
    if representation.contains_interned_string() {
        return Err(LowerError::unsupported_type(
            UnsupportedType::InternedStringCustomRepresentation,
        ));
    }
    Ok(CustomTypeDecl::new(
        custom_id,
        CanonicalName::from(&custom.name),
        metadata::decl_meta(custom.doc.as_ref(), custom.deprecated.as_ref()),
        representation,
        lower_converters(&custom.converters)?,
    ))
}

fn lower_converters(converters: &SourceConverters) -> Result<CustomTypeConverters, LowerError> {
    Ok(CustomTypeConverters::new(
        lower_converter(&converters.into_ffi)?,
        lower_converter(&converters.try_from_ffi)?,
    ))
}

fn lower_converter(converter: &SourceConverter) -> Result<CustomTypeConverter, LowerError> {
    match converter {
        SourceConverter::Path(path) => lower_path(path).map(CustomTypeConverter::path),
        SourceConverter::TraitMethod(converter) => Ok(CustomTypeConverter::trait_method(
            lower_path(&converter.receiver)?,
            NamePart::new(converter.method.as_str()),
        )),
        SourceConverter::Expr(expression) => Ok(CustomTypeConverter::expression(
            CustomConverterExpression::new(expression.source.clone()),
        )),
    }
}

fn lower_path(path: &SourcePath) -> Result<CustomConverterPath, LowerError> {
    let root = match path.root {
        SourcePathRoot::Relative => CustomConverterPathRoot::Relative,
        SourcePathRoot::Crate => CustomConverterPathRoot::Crate,
        SourcePathRoot::Self_ => CustomConverterPathRoot::Self_,
        SourcePathRoot::Super(levels) => CustomConverterPathRoot::Super(levels),
        SourcePathRoot::Absolute => CustomConverterPathRoot::Absolute,
    };
    path.segments
        .iter()
        .map(|segment| match segment.arguments.is_empty() {
            true => Ok(NamePart::new(segment.name.as_str())),
            false => Err(LowerError::unsupported_type(
                UnsupportedType::CustomConverter,
            )),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| CustomConverterPath::new(root, segments))
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, CustomRemoteType, CustomTypeConverter, CustomTypeConverters,
        CustomTypeDef, CustomTypeId as SourceCustomTypeId,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, FieldDef,
        PackageInfo as SourcePackage, ParameterDef, Path as SourcePath, Primitive, RecordDef,
        ReturnDef, SourceContract, TypeExpr,
    };

    use crate::lower::{LowerError, LowerErrorKind, UnsupportedType, lower};
    use crate::{
        Bindings, CanonicalName, CodecNode, CustomTypeDecl, CustomTypeId, Decl, Native, ParamPlan,
        Primitive as BindingPrimitive, Receive, RecordId, ReturnPlan, SurfaceLower, TypeRef,
        Wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn converters() -> CustomTypeConverters {
        CustomTypeConverters::new(
            CustomTypeConverter::path(SourcePath::single("into_ffi")),
            CustomTypeConverter::path(SourcePath::single("try_from_ffi")),
        )
    }

    fn custom_type(id: &str, type_name: &str, repr: TypeExpr) -> CustomTypeDef {
        CustomTypeDef::new(
            SourceCustomTypeId::new(id),
            name(type_name),
            CustomRemoteType::single_path("Remote"),
            repr,
            None,
            converters(),
        )
    }

    fn record_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(id.into(), SourcePath::single(path))
    }

    fn custom_ref(id: &str, path: &str) -> TypeExpr {
        TypeExpr::custom(id.into(), SourcePath::single(path))
    }

    fn lower_customs<S: SurfaceLower>(
        customs: Vec<CustomTypeDef>,
    ) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.customs = customs;
        lower::<S>(&contract)
    }

    fn lower_customs_ok<S: SurfaceLower>(customs: Vec<CustomTypeDef>) -> Bindings<S> {
        lower_customs::<S>(customs).expect("customs should lower")
    }

    fn custom_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&CustomTypeDecl> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::CustomType(custom) => Some(custom.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn only_custom<S: SurfaceLower>(bindings: &Bindings<S>) -> &CustomTypeDecl {
        let decls = custom_decls(bindings);
        assert_eq!(
            decls.len(),
            1,
            "expected exactly one custom type declaration"
        );
        decls[0]
    }

    #[test]
    fn primitive_repr_lowers_to_primitive_representation() {
        let bindings = lower_customs_ok::<Native>(vec![custom_type(
            "demo::Handle",
            "Handle",
            TypeExpr::Primitive(Primitive::U64),
        )]);
        let decl = only_custom(&bindings);

        assert_eq!(decl.id(), CustomTypeId::from_raw(0));
        assert_eq!(decl.name(), &CanonicalName::single("Handle"));
        assert_eq!(
            decl.representation(),
            &TypeRef::Primitive(BindingPrimitive::U64)
        );
    }

    #[test]
    fn string_repr_lowers_to_string_representation() {
        let bindings = lower_customs_ok::<Native>(vec![custom_type(
            "demo::DisplayName",
            "DisplayName",
            TypeExpr::String,
        )]);
        let decl = only_custom(&bindings);

        assert_eq!(decl.representation(), &TypeRef::String);
    }

    #[test]
    fn interned_string_repr_is_rejected_before_wrapper_rendering() {
        let error = lower_customs::<Native>(vec![custom_type(
            "demo::Browser",
            "Browser",
            TypeExpr::interned_string(
                SourcePath::single("InternedString"),
                "demo::BrowserName",
                SourcePath::single("BrowserName"),
                vec!["Chrome".to_owned()],
            ),
        )])
        .expect_err("interned custom representation must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::InternedStringCustomRepresentation)
        ));
    }

    #[test]
    fn record_repr_resolves_to_record_id_in_the_same_contract() {
        let mut contract = package();
        let mut point = RecordDef::new("demo::Point".into(), name("Point"));
        point.fields = vec![FieldDef::new(
            name("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        contract.records.push(point);
        contract.customs.push(custom_type(
            "demo::PointAlias",
            "PointAlias",
            record_type("demo::Point", "Point"),
        ));

        let bindings = lower::<Native>(&contract).expect("contract should lower");
        let decl = custom_decls(&bindings)[0];

        assert_eq!(
            decl.representation(),
            &TypeRef::Record(RecordId::from_raw(0))
        );
    }

    #[test]
    fn customs_lower_on_wasm32_with_same_representation() {
        let bindings = lower_customs_ok::<Wasm32>(vec![custom_type(
            "demo::Handle",
            "Handle",
            TypeExpr::Primitive(Primitive::U32),
        )]);

        assert_eq!(
            only_custom(&bindings).representation(),
            &TypeRef::Primitive(BindingPrimitive::U32)
        );
    }

    #[test]
    fn duplicate_custom_source_ids_are_rejected() {
        let error = lower_customs::<Native>(vec![
            custom_type("demo::Dup", "Dup", TypeExpr::Primitive(Primitive::U32)),
            custom_type("demo::Dup", "DupAgain", TypeExpr::Primitive(Primitive::U32)),
        ])
        .expect_err("duplicate custom id must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::DuplicateSourceId { .. }
        ));
    }

    #[test]
    fn multiple_customs_get_sequential_ids_in_source_order() {
        let bindings = lower_customs_ok::<Native>(vec![
            custom_type("demo::One", "One", TypeExpr::Primitive(Primitive::U32)),
            custom_type("demo::Two", "Two", TypeExpr::Primitive(Primitive::U64)),
            custom_type("demo::Three", "Three", TypeExpr::String),
        ]);
        let ids: Vec<u32> = custom_decls(&bindings)
            .into_iter()
            .map(|decl| decl.id().raw())
            .collect();

        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn custom_doc_and_deprecation_propagate_to_decl_meta() {
        let mut handle = custom_type(
            "demo::Handle",
            "Handle",
            TypeExpr::Primitive(Primitive::U64),
        );
        handle.doc = Some(SourceDocComment::new("opaque handle"));
        handle.deprecated = Some(SourceDeprecationInfo {
            note: Some("use Handle2 instead".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = lower_customs_ok::<Native>(vec![handle]);
        let meta = only_custom(&bindings).meta();

        assert_eq!(meta.doc().map(|doc| doc.as_str()), Some("opaque handle"));
        assert_eq!(
            meta.deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use Handle2 instead")
        );
    }

    #[test]
    fn function_referencing_custom_type_lowers_through_type_ref_custom() {
        let mut contract = package();
        contract.customs.push(custom_type(
            "demo::Handle",
            "Handle",
            TypeExpr::Primitive(Primitive::U64),
        ));
        let mut function =
            boltffi_ast::FunctionDef::new(boltffi_ast::FunctionId::new("demo::open"), name("open"));
        function.parameters = vec![ParameterDef::value(
            name("handle"),
            custom_ref("demo::Handle", "Handle"),
        )];
        function.returns = ReturnDef::value(custom_ref("demo::Handle", "Handle"));
        contract.functions.push(function);

        let bindings = lower::<Native>(&contract).expect("contract should lower");
        let function = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Function(function) => Some(function.as_ref()),
                _ => None,
            })
            .expect("expected function declaration");

        match function.callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                receive: Receive::ByValue,
                codec,
                ..
            } => {
                assert_eq!(ty, &TypeRef::Custom(CustomTypeId::from_raw(0)));
                assert_eq!(
                    codec.root(),
                    &CodecNode::Custom {
                        id: CustomTypeId::from_raw(0),
                        representation: Box::new(CodecNode::Primitive(BindingPrimitive::U64)),
                    }
                );
            }
            other => panic!("expected encoded custom param, got {other:?}"),
        }
        match function.callable().returns().plan() {
            ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } => {
                assert_eq!(ty, &TypeRef::Custom(CustomTypeId::from_raw(0)));
                assert_eq!(
                    codec.root(),
                    &CodecNode::Custom {
                        id: CustomTypeId::from_raw(0),
                        representation: Box::new(CodecNode::Primitive(BindingPrimitive::U64)),
                    }
                );
            }
            other => panic!("expected encoded custom return, got {other:?}"),
        }
    }

    #[test]
    fn custom_type_does_not_register_any_native_symbols() {
        let bindings = lower_customs_ok::<Native>(vec![custom_type(
            "demo::Handle",
            "Handle",
            TypeExpr::Primitive(Primitive::U64),
        )]);

        assert_eq!(bindings.symbols().symbols().len(), 0);
    }
}
