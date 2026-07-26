use boltffi_ast::{
    EnumDef as SourceEnum, TypeExpr, VariantDef as SourceVariant, VariantPayload as SourcePayload,
};

use crate::{
    CStyleEnumDecl, CStyleVariantDecl, CanonicalName, DataEnumDecl, DataVariantDecl,
    DataVariantPayload, EncodedFieldDecl, EnumDecl, ExportedMethodDecl, FieldKey, InitializerDecl,
    IntegerRepr, IntegerValue, NativeSymbol, ValueRef, VariantTag,
};

use super::{
    LowerError, codecs, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata,
    methods, primitive, surface::SurfaceLower, symbol::SymbolAllocator,
};

/// Lowers every enum in the source contract.
///
/// `allocator` is shared across the whole pass so each enum method's
/// [`NativeSymbol`] gets a unique [`SymbolId`] inside the
/// [`Bindings<S>`] under construction.
///
/// [`SymbolId`]: crate::SymbolId
/// [`NativeSymbol`]: crate::NativeSymbol
/// [`Bindings<S>`]: crate::Bindings
pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<EnumDecl<S>>, LowerError> {
    index
        .enums()
        .iter()
        .map(|enumeration| lower_one(index, ids, allocator, enumeration))
        .collect()
}

/// Reports whether a source enum codes as a C-style integer
/// discriminant.
///
/// Exposed to the codec lane so a nested `TypeExpr::Enum(id)` agrees
/// with the enum's own declaration on `CStyleEnum` vs `DataEnum`.
pub fn is_c_style(enumeration: &SourceEnum) -> bool {
    enumeration
        .variants
        .iter()
        .all(|variant| matches!(variant.payload, SourcePayload::Unit))
        && effective_integer_repr(enumeration).is_some()
}

pub fn c_style_repr(enumeration: &SourceEnum) -> Option<IntegerRepr> {
    is_c_style(enumeration)
        .then(|| effective_integer_repr(enumeration))
        .flatten()
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    enumeration: &SourceEnum,
) -> Result<EnumDecl<S>, LowerError> {
    let initializers = methods::lower_enum_initializers::<S>(index, ids, allocator, enumeration)?;
    let enum_methods = methods::lower_enum_methods::<S>(index, ids, allocator, enumeration)?;
    if is_c_style(enumeration) {
        lower_c_style(ids, enumeration, initializers, enum_methods).map(EnumDecl::c_style)
    } else {
        lower_data(index, ids, enumeration, initializers, enum_methods).map(EnumDecl::data)
    }
}

fn lower_c_style<S: SurfaceLower>(
    ids: &DeclarationIds,
    enumeration: &SourceEnum,
    initializers: Vec<InitializerDecl<S>>,
    enum_methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
) -> Result<CStyleEnumDecl<S>, LowerError> {
    Ok(CStyleEnumDecl::new(
        ids.enumeration(&enumeration.id)?,
        CanonicalName::from(&enumeration.name),
        metadata::decl_meta(enumeration.doc.as_ref(), enumeration.deprecated.as_ref()),
        effective_integer_repr(enumeration)
            .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::EnumRepr))?,
        discriminants(&enumeration.variants)?
            .into_iter()
            .map(|(variant, discriminant)| {
                Ok(CStyleVariantDecl::new(
                    CanonicalName::from(&variant.name),
                    IntegerValue::new(discriminant),
                    metadata::element_meta(variant.doc.as_ref(), None, None)?,
                ))
            })
            .collect::<Result<Vec<_>, LowerError>>()?,
        initializers,
        enum_methods,
    ))
}

fn lower_data<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    enumeration: &SourceEnum,
    initializers: Vec<InitializerDecl<S>>,
    enum_methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
) -> Result<DataEnumDecl<S>, LowerError> {
    Ok(DataEnumDecl::new(
        ids.enumeration(&enumeration.id)?,
        CanonicalName::from(&enumeration.name),
        metadata::decl_meta(enumeration.doc.as_ref(), enumeration.deprecated.as_ref()),
        enumeration
            .variants
            .iter()
            .enumerate()
            .map(|(variant_index, variant)| lower_variant(index, ids, variant_index, variant))
            .collect::<Result<Vec<_>, LowerError>>()?,
        initializers,
        enum_methods,
        codecs::plan(
            index,
            ids,
            &TypeExpr::enumeration(
                enumeration.id.clone(),
                boltffi_ast::Path::single(enumeration.name.spelling()),
            ),
            ValueRef::self_value(),
        )?,
    ))
}

fn lower_variant(
    index: &Index,
    ids: &DeclarationIds,
    variant_index: usize,
    variant: &SourceVariant,
) -> Result<DataVariantDecl, LowerError> {
    Ok(DataVariantDecl::new(
        CanonicalName::from(&variant.name),
        VariantTag::from_index(variant_index).ok_or_else(LowerError::variant_tag_overflow)?,
        lower_payload(index, ids, &variant.payload)?,
        metadata::element_meta(variant.doc.as_ref(), None, None)?,
    ))
}

fn lower_payload(
    index: &Index,
    ids: &DeclarationIds,
    payload: &SourcePayload,
) -> Result<DataVariantPayload, LowerError> {
    match payload {
        SourcePayload::Unit => Ok(DataVariantPayload::Unit),
        SourcePayload::Tuple(types) => types
            .iter()
            .enumerate()
            .map(|(field_index, type_expr)| {
                let key = FieldKey::position(field_index)
                    .ok_or_else(LowerError::field_position_overflow)?;
                let value = ValueRef::self_value().field(key.clone());
                let ty = super::types::lower(ids, type_expr)?;
                let codec = codecs::plan(index, ids, type_expr, value)?;
                Ok(EncodedFieldDecl::new(key, ty, codec, Default::default()))
            })
            .collect::<Result<Vec<_>, LowerError>>()
            .map(DataVariantPayload::Tuple),
        SourcePayload::Struct(fields) => fields
            .iter()
            .map(|field| {
                let key = FieldKey::from(field);
                let value = ValueRef::self_value().field(key.clone());
                let ty = super::types::lower(ids, &field.type_expr)?;
                let codec = codecs::plan(index, ids, &field.type_expr, value)?;
                Ok(EncodedFieldDecl::new(
                    key,
                    ty,
                    codec,
                    metadata::element_meta(field.doc.as_ref(), None, field.default.as_ref())?,
                ))
            })
            .collect::<Result<Vec<_>, LowerError>>()
            .map(DataVariantPayload::Struct),
    }
}

fn effective_integer_repr(enumeration: &SourceEnum) -> Option<IntegerRepr> {
    primitive::integer_repr(&enumeration.repr).or_else(|| {
        (enumeration
            .variants
            .iter()
            .all(|variant| matches!(variant.payload, SourcePayload::Unit))
            && enumeration.repr.items.is_empty())
        .then_some(IntegerRepr::I32)
    })
}

fn discriminants(variants: &[SourceVariant]) -> Result<Vec<(&SourceVariant, i128)>, LowerError> {
    variants
        .iter()
        .try_fold((0_i128, Vec::new()), |(next, mut variants), variant| {
            let discriminant = variant.discriminant.unwrap_or(next);
            variants.push((variant, discriminant));
            let next = discriminant
                .checked_add(1)
                .ok_or_else(LowerError::discriminant_overflow)?;
            Ok((next, variants))
        })
        .map(|(_, variants)| variants)
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, DefaultValue as SourceDefaultValue,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, EnumDef,
        ExecutionKind, FieldDef, FnSig, FnTrait, FnTraitKind, IntegerLiteral, MethodDef,
        MethodId as SourceMethodId, PackageInfo as SourcePackage, ParameterDef, ParameterPassing,
        Path as SourcePath, Primitive, Receiver, RecordDef, ReprAttr, ReprItem, ReturnDef, Source,
        SourceContract, TypeExpr, VariantDef, VariantPayload,
    };

    use crate::lower::lower;
    use crate::{
        BindingErrorKind, Bindings, CStyleEnumDecl, CanonicalName, CodecNode, DataEnumDecl,
        DataVariantPayload, Decl, DefaultValue, DirectValueType, EncodedFieldDecl, EnumDecl,
        EnumId, ErrorDecl, ExecutionDecl, ExportedMethodDecl, FieldKey, InitializerDecl,
        IntegerRepr, IntegerValue, LowerError, LowerErrorKind, Native, NativeSymbol, OutOfRust,
        ParamPlan, Primitive as BindingPrimitive, ReadPlan, Receive, RecordId, ReturnPlan,
        SurfaceLower, TypeRef, ValueRef, Wasm32, native, wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn binding_name(part: &str) -> CanonicalName {
        CanonicalName::single(part)
    }

    fn unit_variant(variant_name: &str) -> VariantDef {
        VariantDef::unit(name(variant_name))
    }

    fn unit_variant_with_discriminant(variant_name: &str, discriminant: i128) -> VariantDef {
        let mut variant = unit_variant(variant_name);
        variant.discriminant = Some(discriminant);
        variant
    }

    fn variant(variant_name: &str, payload: VariantPayload) -> VariantDef {
        VariantDef {
            name: name(variant_name).into(),
            discriminant: None,
            payload,
            doc: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }

    fn tuple_variant(variant_name: &str, fields: Vec<TypeExpr>) -> VariantDef {
        variant(variant_name, VariantPayload::Tuple(fields))
    }

    fn struct_variant(variant_name: &str, fields: Vec<FieldDef>) -> VariantDef {
        variant(variant_name, VariantPayload::Struct(fields))
    }

    fn enumeration(id: &str, enum_name: &str, variants: Vec<VariantDef>) -> EnumDef {
        let mut enumeration = EnumDef::new(id.into(), name(enum_name));
        enumeration.variants = variants;
        enumeration
    }

    fn direction_enum() -> EnumDef {
        enumeration(
            "demo::Direction",
            "Direction",
            vec![unit_variant("north"), unit_variant("south")],
        )
    }

    fn event_enum() -> EnumDef {
        enumeration(
            "demo::Event",
            "Event",
            vec![
                unit_variant("none"),
                tuple_variant("message", vec![TypeExpr::String]),
            ],
        )
    }

    fn field(field_name: &str, type_expr: TypeExpr) -> FieldDef {
        FieldDef::new(name(field_name), type_expr)
    }

    fn record(id: &str, record_name: &str, fields: Vec<FieldDef>) -> RecordDef {
        let mut record = RecordDef::new(id.into(), name(record_name));
        record.fields = fields;
        record
    }

    fn point_record() -> RecordDef {
        let mut record = record(
            "demo::Point",
            "Point",
            vec![
                field("x", TypeExpr::Primitive(Primitive::F64)),
                field("y", TypeExpr::Primitive(Primitive::F64)),
            ],
        );
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record
    }

    fn method(method_name: &str, receiver: Receiver) -> MethodDef {
        MethodDef::new(
            SourceMethodId::new(method_name),
            name(method_name),
            receiver,
        )
    }

    fn method_with(
        method_name: &str,
        receiver: Receiver,
        parameters: Vec<ParameterDef>,
        returns: ReturnDef,
    ) -> MethodDef {
        let mut method = method(method_name, receiver);
        method.parameters = parameters;
        method.returns = returns;
        method
    }

    fn value_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn ref_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        let mut parameter = value_param(param_name, type_expr);
        parameter.passing = ParameterPassing::Ref;
        parameter
    }

    fn closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::impl_fn(FnTrait::new(
            FnTraitKind::Fn,
            FnSig::new(parameters, returns),
        ))
    }

    fn record_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(id.into(), SourcePath::single(path))
    }

    fn enum_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::enumeration(id.into(), SourcePath::single(path))
    }

    fn enum_with_methods(mut enumeration: EnumDef, methods: Vec<MethodDef>) -> EnumDef {
        enumeration.methods = methods;
        enumeration
    }

    fn lower_enum<S: SurfaceLower>(enumeration: EnumDef) -> Bindings<S> {
        lower_enums(vec![enumeration])
    }

    fn lower_enum_result<S: SurfaceLower>(enumeration: EnumDef) -> Result<Bindings<S>, LowerError> {
        lower_enums_result(vec![enumeration])
    }

    fn lower_enums<S: SurfaceLower>(enums: Vec<EnumDef>) -> Bindings<S> {
        lower_enums_result(enums).expect("enum should lower")
    }

    fn lower_enums_result<S: SurfaceLower>(enums: Vec<EnumDef>) -> Result<Bindings<S>, LowerError> {
        lower_contract_result(Vec::new(), enums)
    }

    fn lower_contract<S: SurfaceLower>(
        records: Vec<RecordDef>,
        enums: Vec<EnumDef>,
    ) -> Bindings<S> {
        lower_contract_result(records, enums).expect("contract should lower")
    }

    fn lower_contract_result<S: SurfaceLower>(
        records: Vec<RecordDef>,
        enums: Vec<EnumDef>,
    ) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.records = records;
        contract.enums = enums;
        lower::<S>(&contract)
    }

    fn enum_decl_at<S: SurfaceLower>(bindings: &Bindings<S>, index: usize) -> &EnumDecl<S> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Enum(enumeration) => Some(enumeration.as_ref()),
                _ => None,
            })
            .nth(index)
            .expect("expected enum declaration")
    }

    fn c_style_enum<S: SurfaceLower>(bindings: &Bindings<S>) -> &CStyleEnumDecl<S> {
        match enum_decl_at(bindings, 0) {
            EnumDecl::CStyle(enumeration) => enumeration,
            EnumDecl::Data(_) => panic!("expected c-style enum"),
        }
    }

    fn data_enum<S: SurfaceLower>(bindings: &Bindings<S>) -> &DataEnumDecl<S> {
        match enum_decl_at(bindings, 0) {
            EnumDecl::Data(enumeration) => enumeration.as_ref(),
            EnumDecl::CStyle(_) => panic!("expected data enum"),
        }
    }

    fn enum_methods_at<S: SurfaceLower>(
        bindings: &Bindings<S>,
        index: usize,
    ) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        enum_decl_at(bindings, index).methods()
    }

    fn enum_initializers_at<S: SurfaceLower>(
        bindings: &Bindings<S>,
        index: usize,
    ) -> &[InitializerDecl<S>] {
        enum_decl_at(bindings, index).initializers()
    }

    fn only_method<S: SurfaceLower>(
        bindings: &Bindings<S>,
    ) -> &ExportedMethodDecl<S, NativeSymbol> {
        let methods = enum_methods_at(bindings, 0);
        assert_eq!(methods.len(), 1);
        &methods[0]
    }

    fn only_initializer<S: SurfaceLower>(bindings: &Bindings<S>) -> &InitializerDecl<S> {
        let initializers = enum_initializers_at(bindings, 0);
        assert_eq!(initializers.len(), 1);
        &initializers[0]
    }

    fn assert_encoded_string_error(error: &ErrorDecl<Native, OutOfRust>) {
        match error {
            ErrorDecl::EncodedViaReturnSlot {
                ty,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded string error, got {other:?}"),
        }
    }

    fn tuple_fields(payload: &DataVariantPayload) -> &[EncodedFieldDecl] {
        match payload {
            DataVariantPayload::Tuple(fields) => fields,
            other => panic!("expected tuple payload, got {other:?}"),
        }
    }

    fn struct_fields(payload: &DataVariantPayload) -> &[EncodedFieldDecl] {
        match payload {
            DataVariantPayload::Struct(fields) => fields,
            other => panic!("expected struct payload, got {other:?}"),
        }
    }

    #[test]
    fn classifies_c_style_enum_without_repr_as_i32() {
        let bindings = lower_enum::<Native>(enumeration(
            "demo::Direction",
            "direction",
            vec![unit_variant("north"), unit_variant("south")],
        ));
        let enumeration = c_style_enum(&bindings);

        assert_eq!(enumeration.id(), EnumId::from_raw(0));
        assert_eq!(enumeration.name(), &binding_name("direction"));
        assert_eq!(enumeration.repr(), IntegerRepr::I32);
        assert_eq!(
            enumeration
                .variants()
                .iter()
                .map(|variant| (variant.name().clone(), variant.discriminant().get()))
                .collect::<Vec<_>>(),
            vec![(binding_name("north"), 0), (binding_name("south"), 1)]
        );
    }

    #[test]
    fn advances_c_style_enum_discriminants_from_explicit_values() {
        let bindings = lower_enum::<Native>(enumeration(
            "demo::Status",
            "status",
            vec![
                unit_variant_with_discriminant("created", 10),
                unit_variant("running"),
                unit_variant_with_discriminant("stopped", 8),
                unit_variant("finished"),
            ],
        ));
        let enumeration = c_style_enum(&bindings);

        assert_eq!(
            enumeration
                .variants()
                .iter()
                .map(|variant| (variant.name().clone(), variant.discriminant().get()))
                .collect::<Vec<_>>(),
            vec![
                (binding_name("created"), 10),
                (binding_name("running"), 11),
                (binding_name("stopped"), 8),
                (binding_name("finished"), 9),
            ]
        );
    }

    #[test]
    fn classifies_c_style_enum_with_integer_repr() {
        let mut code = enumeration(
            "demo::Code",
            "code",
            vec![unit_variant("ok"), unit_variant("failed")],
        );
        code.repr = ReprAttr::new(vec![ReprItem::Primitive(Primitive::U8)]);

        let bindings = lower_enum::<Native>(code);
        let enumeration = c_style_enum(&bindings);

        assert_eq!(enumeration.repr(), IntegerRepr::U8);
    }

    #[test]
    fn classifies_fieldless_repr_c_enum_without_integer_repr_as_data() {
        let mut direction = enumeration(
            "demo::Direction",
            "direction",
            vec![unit_variant("north"), unit_variant("south")],
        );
        direction.repr = ReprAttr::new(vec![ReprItem::C]);

        let bindings = lower_enum::<Native>(direction);
        let enumeration = data_enum(&bindings);

        assert_eq!(enumeration.id(), EnumId::from_raw(0));
        assert_eq!(
            enumeration.read().root(),
            &CodecNode::DataEnum(EnumId::from_raw(0))
        );
        assert_eq!(
            enumeration.write().root(),
            &CodecNode::DataEnum(EnumId::from_raw(0))
        );
        assert_eq!(enumeration.write().value(), &ValueRef::self_value());
        assert_eq!(
            enumeration
                .variants()
                .iter()
                .map(|variant| (variant.name().clone(), variant.tag().get()))
                .collect::<Vec<_>>(),
            vec![(binding_name("north"), 0), (binding_name("south"), 1)]
        );
        assert!(matches!(
            enumeration.variants()[0].payload(),
            DataVariantPayload::Unit
        ));
    }

    #[test]
    fn tuple_payload_lowers_field_keys_types_codecs_and_value_paths() {
        let bindings = lower_enum::<Native>(event_enum());
        let enumeration = data_enum(&bindings);
        let message = &enumeration.variants()[1];
        let fields = tuple_fields(message.payload());

        assert_eq!(message.name(), &binding_name("message"));
        assert_eq!(message.tag().get(), 1);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key(), &FieldKey::Position(0));
        assert_eq!(fields[0].ty(), &TypeRef::String);
        assert_eq!(fields[0].read().root(), &CodecNode::String);
        assert_eq!(fields[0].write().root(), &CodecNode::String);
        assert_eq!(
            fields[0].write().value(),
            &ValueRef::self_value().field(FieldKey::Position(0))
        );
    }

    #[test]
    fn struct_payload_lowers_field_metadata_and_codec_paths() {
        let mut count = field("count", TypeExpr::Primitive(Primitive::I32));
        count.doc = Some(SourceDocComment::new("number of events"));
        count.default = Some(SourceDefaultValue::Integer(IntegerLiteral::new(7, "7")));
        let event = enumeration(
            "demo::Event",
            "Event",
            vec![struct_variant("counted", vec![count])],
        );

        let bindings = lower_enum::<Native>(event);
        let enumeration = data_enum(&bindings);
        let fields = struct_fields(enumeration.variants()[0].payload());
        let key = FieldKey::Named(binding_name("count"));

        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key(), &key);
        assert_eq!(fields[0].ty(), &TypeRef::Primitive(BindingPrimitive::I32));
        assert_eq!(
            fields[0].read().root(),
            &CodecNode::Primitive(BindingPrimitive::I32)
        );
        assert_eq!(
            fields[0].write().root(),
            &CodecNode::Primitive(BindingPrimitive::I32)
        );
        assert_eq!(
            fields[0].write().value(),
            &ValueRef::self_value().field(key)
        );
        assert_eq!(
            fields[0].meta().doc().map(|doc| doc.as_str()),
            Some("number of events")
        );
        assert_eq!(
            fields[0].meta().default(),
            Some(&DefaultValue::Integer(IntegerValue::new(7)))
        );
    }

    #[test]
    fn data_enum_tags_ignore_source_discriminants() {
        let mut event = enumeration("demo::Event", "Event", Vec::new());
        event.variants = vec![unit_variant_with_discriminant("none", 10), {
            let mut message = tuple_variant("message", vec![TypeExpr::String]);
            message.discriminant = Some(20);
            message
        }];

        let bindings = lower_enum::<Native>(event);
        let enumeration = data_enum(&bindings);

        assert_eq!(
            enumeration
                .variants()
                .iter()
                .map(|variant| (variant.name().clone(), variant.tag().get()))
                .collect::<Vec<_>>(),
            vec![(binding_name("none"), 0), (binding_name("message"), 1)]
        );
    }

    #[test]
    fn methods_lower_on_a_c_style_enum() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method("rotate", Receiver::Mutable)],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.target().name().as_str(),
            "boltffi_method_enum_demo_direction_rotate"
        );
        assert_eq!(method.callable().receiver(), Some(Receive::ByMutRef));
        assert!(matches!(enum_decl_at(&bindings, 0), EnumDecl::CStyle(_)));
    }

    #[test]
    fn methods_lower_on_a_data_enum() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            event_enum(),
            vec![method("describe", Receiver::Shared)],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.target().name().as_str(),
            "boltffi_method_enum_demo_event_describe"
        );
        assert_eq!(method.callable().receiver(), Some(Receive::ByRef));
        assert!(matches!(enum_decl_at(&bindings, 0), EnumDecl::Data(_)));
    }

    #[test]
    fn enum_method_returning_self_on_c_style_lowers_self_to_direct_enum() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "flip",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(TypeExpr::SelfType),
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
            }
        );
    }

    #[test]
    fn enum_method_returning_self_on_data_enum_lowers_self_to_encoded_enum() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            event_enum(),
            vec![method_with(
                "clone_event",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(TypeExpr::SelfType),
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Enum(EnumId::from_raw(0)),
                codec: ReadPlan::new(CodecNode::DataEnum(EnumId::from_raw(0))),
                shape: native::BufferShape::Buffer,
            }
        );
    }

    #[test]
    fn enum_static_method_returning_self_is_an_initializer() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "default_direction",
                Receiver::None,
                Vec::new(),
                ReturnDef::value(TypeExpr::SelfType),
            )],
        ));
        let initializer = only_initializer(&bindings);

        assert!(enum_methods_at(&bindings, 0).is_empty());
        assert_eq!(initializer.callable().receiver(), None);
        assert_eq!(
            initializer.symbol().name().as_str(),
            "boltffi_init_enum_demo_direction_default_direction"
        );
        assert_eq!(
            initializer.callable().returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
            }
        );
    }

    #[test]
    fn enum_result_self_initializer_uses_success_out_and_encoded_error() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "try_default",
                Receiver::None,
                Vec::new(),
                ReturnDef::value(TypeExpr::Result {
                    ok: Box::new(TypeExpr::SelfType),
                    err: Box::new(TypeExpr::String),
                }),
            )],
        ));
        let initializer = only_initializer(&bindings);

        assert!(enum_methods_at(&bindings, 0).is_empty());
        assert_eq!(
            initializer.callable().returns().plan(),
            &ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
            }
        );
        assert_encoded_string_error(initializer.callable().error());
    }

    #[test]
    fn data_enum_result_self_initializer_uses_encoded_success_out_and_encoded_error() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            event_enum(),
            vec![method_with(
                "try_event",
                Receiver::None,
                Vec::new(),
                ReturnDef::value(TypeExpr::Result {
                    ok: Box::new(TypeExpr::SelfType),
                    err: Box::new(TypeExpr::String),
                }),
            )],
        ));
        let initializer = only_initializer(&bindings);

        assert!(enum_methods_at(&bindings, 0).is_empty());
        assert_eq!(
            initializer.callable().returns().plan(),
            &ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Enum(EnumId::from_raw(0)),
                codec: ReadPlan::new(CodecNode::DataEnum(EnumId::from_raw(0))),
                shape: native::BufferShape::Buffer,
            }
        );
        assert_encoded_string_error(initializer.callable().error());
    }

    #[test]
    fn enum_self_in_parameter_position_substitutes_to_owning_enum() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "equals",
                Receiver::Shared,
                vec![value_param("other", TypeExpr::SelfType)],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().params()[0].as_value().unwrap(),
            &ParamPlan::Direct {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
                receive: Receive::ByValue,
            }
        );
    }

    #[test]
    fn enum_method_with_vec_self_return_substitutes_through_sequence() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "neighbours",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(TypeExpr::vec(TypeExpr::SelfType)),
            )],
        ));
        let method = only_method(&bindings);

        match method.callable().returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(
                    ty,
                    &TypeRef::Sequence(Box::new(TypeRef::Enum(EnumId::from_raw(0))))
                );
                match codec.root() {
                    CodecNode::Sequence { element, .. } => {
                        assert_eq!(
                            element.as_ref(),
                            &CodecNode::CStyleEnum(EnumId::from_raw(0))
                        );
                    }
                    other => panic!("expected sequence codec, got {other:?}"),
                }
            }
            other => panic!("expected encoded sequence return, got {other:?}"),
        }
    }

    #[test]
    fn async_enum_method_lowers_to_poll_handle_protocol_on_native() {
        let mut async_method = method("compute", Receiver::Shared);
        async_method.execution = ExecutionKind::Async;
        let bindings =
            lower_enum::<Native>(enum_with_methods(direction_enum(), vec![async_method]));
        let method = only_method(&bindings);

        assert_eq!(
            method.target().name().as_str(),
            "boltffi_method_enum_demo_direction_compute"
        );
        match method.callable().execution() {
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                handle,
                poll,
                complete,
                cancel,
                free,
                panic_message,
            }) => {
                assert_eq!(handle, &native::HandleCarrier::U64);
                assert_eq!(
                    poll.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_enum_method_lowers_to_poll_handle_protocol_on_wasm32() {
        let mut async_method = method("compute", Receiver::Shared);
        async_method.execution = ExecutionKind::Async;
        let bindings =
            lower_enum::<Wasm32>(enum_with_methods(direction_enum(), vec![async_method]));
        let methods = enum_methods_at(&bindings, 0);

        match methods[0].callable().execution() {
            ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::PollHandle {
                handle,
                poll_sync,
                complete,
                cancel,
                free,
                panic_message,
            }) => {
                assert_eq!(handle, &wasm32::HandleCarrier::U32);
                assert_eq!(
                    poll_sync.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_poll_sync"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_method_enum_demo_direction_compute_panic_message"
                );
            }
            other => panic!("expected wasm32 PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_enum_initializer_lowers_to_poll_handle_protocol() {
        let mut parse = method("parse", Receiver::None);
        parse.execution = ExecutionKind::Async;
        parse.returns = ReturnDef::value(TypeExpr::SelfType);
        let bindings = lower_enum::<Native>(enum_with_methods(direction_enum(), vec![parse]));
        let initializer = only_initializer(&bindings);

        assert_eq!(
            initializer.symbol().name().as_str(),
            "boltffi_init_enum_demo_direction_parse"
        );
        match initializer.callable().execution() {
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => {
                assert_eq!(
                    poll.name().as_str(),
                    "boltffi_async_init_enum_demo_direction_parse_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_init_enum_demo_direction_parse_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_init_enum_demo_direction_parse_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_init_enum_demo_direction_parse_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_init_enum_demo_direction_parse_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn enum_method_returning_result_uses_success_out_and_encoded_error() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "try_value",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(TypeExpr::Result {
                    ok: Box::new(TypeExpr::Primitive(Primitive::I32)),
                    err: Box::new(TypeExpr::String),
                }),
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
            }
        );
        assert_encoded_string_error(method.callable().error());
    }

    #[test]
    fn enum_method_string_param_lowers_encoded_with_native_slice_shape() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "set_label",
                Receiver::Mutable,
                vec![value_param("label", TypeExpr::String)],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        match method.callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
            } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(codec.root(), &CodecNode::String);
                assert_eq!(codec.value(), &ValueRef::named(binding_name("label")));
            }
            other => panic!("expected encoded String param with native slice, got {other:?}"),
        }
    }

    #[test]
    fn enum_method_ref_parameter_lowers_to_by_ref_receive() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "matches",
                Receiver::Shared,
                vec![ref_param("other", TypeExpr::Primitive(Primitive::I32))],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().params()[0].as_value().unwrap(),
            &ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
                receive: Receive::ByRef,
            }
        );
    }

    #[test]
    fn enum_method_closure_parameter_lowers_to_lower_plan_closure_with_callable() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "on_each",
                Receiver::Shared,
                vec![value_param(
                    "callback",
                    closure(vec![TypeExpr::Primitive(Primitive::I32)], ReturnDef::Void),
                )],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        let callable = method.callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure on enum method")
            .invoke();
        let params = callable.params();
        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
                ..
            }
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn wasm32_enum_method_encoded_param_uses_slice_shape() {
        let bindings = lower_enum::<Wasm32>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "set_label",
                Receiver::Mutable,
                vec![value_param("label", TypeExpr::String)],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        match method.callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: wasm32::BufferShape::Slice,
                receive: Receive::ByValue,
            } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(codec.root(), &CodecNode::Utf8String);
                assert_eq!(codec.value(), &ValueRef::named(binding_name("label")));
            }
            other => panic!("expected wasm32 slice param shape, got {other:?}"),
        }
    }

    #[test]
    fn wasm32_enum_method_encoded_return_uses_packed_shape() {
        let bindings = lower_enum::<Wasm32>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "describe",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(TypeExpr::String),
            )],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec: ReadPlan::new(CodecNode::Utf8String),
                shape: wasm32::BufferShape::Packed,
            }
        );
    }

    #[test]
    fn wasm32_enum_method_closure_parameter_lowers_to_lower_plan_closure_with_callable() {
        let bindings = lower_enum::<Wasm32>(enum_with_methods(
            direction_enum(),
            vec![method_with(
                "on_each",
                Receiver::Shared,
                vec![value_param(
                    "callback",
                    closure(vec![TypeExpr::Primitive(Primitive::I32)], ReturnDef::Void),
                )],
                ReturnDef::Void,
            )],
        ));
        let method = only_method(&bindings);

        let callable = method.callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure on wasm32 enum method")
            .invoke();
        let params = callable.params();
        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
                ..
            }
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn multiple_enum_methods_get_sequential_ids_in_source_order() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![
                method("rotate", Receiver::Mutable),
                method("opposite", Receiver::Shared),
                method("describe", Receiver::Shared),
            ],
        ));
        let methods = enum_methods_at(&bindings, 0);

        assert_eq!(methods.len(), 3);
        assert_eq!(
            methods
                .iter()
                .map(|method| (
                    method.id().raw(),
                    method.name().parts().last().unwrap().as_str()
                ))
                .collect::<Vec<_>>(),
            vec![(0, "rotate"), (1, "opposite"), (2, "describe")]
        );
    }

    #[test]
    fn enum_method_doc_and_deprecation_propagate_to_decl_meta() {
        let mut rotate = method("rotate", Receiver::Mutable);
        rotate.doc = Some(SourceDocComment::new("rotates the heading"));
        rotate.deprecated = Some(SourceDeprecationInfo {
            note: Some("use turn instead".to_owned()),
            since: Some("0.3".to_owned()),
        });

        let bindings = lower_enum::<Native>(enum_with_methods(direction_enum(), vec![rotate]));
        let method = only_method(&bindings);
        let meta = method.meta();

        assert_eq!(
            meta.doc().map(|doc| doc.as_str()),
            Some("rotates the heading")
        );
        assert_eq!(
            meta.deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use turn instead")
        );
        assert_eq!(
            meta.deprecated().and_then(|deprecated| deprecated.since()),
            Some("0.3")
        );
    }

    #[test]
    fn enum_method_can_reference_record_in_signature() {
        let direction = enum_with_methods(
            direction_enum(),
            vec![method_with(
                "to_point",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(record_type("demo::Point", "Point")),
            )],
        );

        let bindings = lower_contract::<Native>(vec![point_record()], vec![direction]);
        let method = &enum_methods_at(&bindings, 0)[0];

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Record(RecordId::from_raw(0)),
            }
        );
    }

    #[test]
    fn enum_method_can_reference_another_enum_in_signature() {
        let event = enum_with_methods(
            event_enum(),
            vec![method_with(
                "heading_for",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(enum_type("demo::Direction", "Direction")),
            )],
        );

        let bindings = lower_enums::<Native>(vec![direction_enum(), event]);
        let method = &enum_methods_at(&bindings, 1)[0];

        assert_eq!(
            method.callable().returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
            }
        );
    }

    #[test]
    fn enum_method_native_symbol_is_registered_in_table() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![
                method("rotate", Receiver::Mutable),
                method("opposite", Receiver::Shared),
            ],
        ));
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();

        assert_eq!(
            names,
            vec![
                "boltffi_method_enum_demo_direction_rotate",
                "boltffi_method_enum_demo_direction_opposite"
            ]
        );
    }

    #[test]
    fn acronym_enum_name_lowers_to_snake_cased_symbol() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            enumeration(
                "demo::HTTPStatus",
                "HTTPStatus",
                vec![unit_variant("ok"), unit_variant("not_found")],
            ),
            vec![method("describe", Receiver::Shared)],
        ));
        let method = only_method(&bindings);

        assert_eq!(
            method.target().name().as_str(),
            "boltffi_method_enum_demo_http_status_describe"
        );
    }

    #[test]
    fn duplicate_enum_method_names_fail_validation() {
        let error = lower_enum_result::<Native>(enum_with_methods(
            direction_enum(),
            vec![
                method("rotate", Receiver::Mutable),
                method("rotate", Receiver::Mutable),
            ],
        ))
        .expect_err("duplicate enum method symbol should fail validation");

        match error.kind() {
            LowerErrorKind::InvalidBindings(error) => match error.kind() {
                BindingErrorKind::DuplicateSymbolName(name) => {
                    assert_eq!(name, "boltffi_method_enum_demo_direction_rotate");
                }
                other => panic!("expected DuplicateSymbolName, got {other:?}"),
            },
            other => panic!("expected InvalidBindings, got {other:?}"),
        }
    }

    #[test]
    fn enum_method_callable_has_synchronous_execution_and_no_error_channel() {
        let bindings = lower_enum::<Native>(enum_with_methods(
            direction_enum(),
            vec![method("rotate", Receiver::Mutable)],
        ));
        let method = only_method(&bindings);
        let callable = method.callable();

        assert!(matches!(
            callable.execution(),
            ExecutionDecl::Synchronous(_)
        ));
        assert!(matches!(callable.error(), ErrorDecl::None(_)));
    }
}
