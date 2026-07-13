use boltffi_ast::{RecordDef as SourceRecord, RecordEncoding, TypeExpr};

use crate::{
    CanonicalName, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    EncodedRecordStorage, ExportedMethodDecl, FieldKey, InitializerDecl, NativeOpaqueFieldExports,
    NativeOpaqueRecordExports, NativeSymbol, RecordDecl, TypeRef, ValueRef,
};

use super::{
    LowerError, codecs, error::UnsupportedType, ids::DeclarationIds, index::Index, layout,
    metadata, methods, opaque, primitive, surface::SurfaceLower, symbol::SymbolAllocator, types,
};

/// Lowers every record in the source contract.
///
/// `allocator` is shared across the whole pass so the [`SymbolId`]
/// each method's [`NativeSymbol`] receives is unique inside the
/// [`Bindings<S>`] under construction.
///
/// [`SymbolId`]: crate::SymbolId
/// [`NativeSymbol`]: crate::NativeSymbol
/// [`Bindings<S>`]: crate::Bindings
pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<RecordDecl<S>>, LowerError> {
    index
        .records()
        .iter()
        .map(|record| lower_one(index, ids, allocator, record))
        .collect()
}

/// Reports whether a source record crosses by direct memory.
///
/// Exposed to the codec lane so a nested `TypeExpr::Record(id)` can
/// pick `DirectRecord` vs `EncodedRecord` from the same predicate the
/// record's own declaration uses.
pub fn is_direct(record: &SourceRecord) -> bool {
    record.encoding == RecordEncoding::Standard
        && primitive::has_repr_c(&record.repr)
        && !record.fields.is_empty()
        && record
            .fields
            .iter()
            .all(|field| primitive::direct_field_type(&field.type_expr).is_some())
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &SourceRecord,
) -> Result<RecordDecl<S>, LowerError> {
    if record.encoding == RecordEncoding::NativeOpaque && !record.methods.is_empty() {
        return Err(LowerError::unsupported_type(
            UnsupportedType::NativeOpaqueRecordMethod,
        ));
    }
    if record.encoding != RecordEncoding::NativeOpaque
        && record
            .fields
            .iter()
            .any(|field| opaque::contains(index, &field.type_expr))
    {
        return Err(LowerError::unsupported_type(
            UnsupportedType::NativeOpaqueRecordField,
        ));
    }
    // Validate the source expression before `types::lower` erases the distinction
    // between owned and borrowed string/byte representations (e.g. `String` vs
    // `str`, and `Vec<u8>` vs `[u8]`). Native opaque accessors borrow from the
    // record's owned storage, so borrowed source forms must never reach dsize or
    // accessor generation.
    if record.encoding == RecordEncoding::NativeOpaque
        && record
            .fields
            .iter()
            .any(|field| !native_opaque_source_field_supported(&field.type_expr))
    {
        return Err(LowerError::unsupported_type(
            UnsupportedType::NativeOpaqueRecordField,
        ));
    }
    let initializers = methods::lower_record_initializers::<S>(index, ids, allocator, record)?;
    let record_methods = methods::lower_record_methods::<S>(index, ids, allocator, record)?;
    if is_direct(record) {
        lower_direct(ids, record, initializers, record_methods).map(RecordDecl::direct)
    } else {
        lower_encoded(index, ids, allocator, record, initializers, record_methods)
            .map(RecordDecl::encoded)
    }
}

fn lower_direct<S: SurfaceLower>(
    ids: &DeclarationIds,
    record: &SourceRecord,
    initializers: Vec<InitializerDecl<S>>,
    record_methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
) -> Result<DirectRecordDecl<S>, LowerError> {
    let fields = record
        .fields
        .iter()
        .map(|field| {
            Ok(DirectFieldDecl::new(
                FieldKey::from(field),
                primitive::direct_field_type(&field.type_expr)
                    .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::RecordField))?,
                metadata::element_meta(field.doc.as_ref(), None, field.default.as_ref())?,
            ))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;

    Ok(DirectRecordDecl::new(
        ids.record(&record.id)?,
        CanonicalName::from(&record.name),
        metadata::decl_meta(record.doc.as_ref(), record.deprecated.as_ref()),
        fields,
        initializers,
        record_methods,
        layout::compute(record)?,
    ))
}

fn lower_encoded<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    record: &SourceRecord,
    initializers: Vec<InitializerDecl<S>>,
    record_methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
) -> Result<EncodedRecordDecl<S>, LowerError> {
    let fields = record
        .fields
        .iter()
        .map(|field| {
            let key = FieldKey::from(field);
            let value = ValueRef::self_value().field(key.clone());
            let ty = types::lower(ids, &field.type_expr)?;
            let codec = codecs::plan(index, ids, &field.type_expr, value)?;
            Ok(EncodedFieldDecl::new(
                key,
                ty,
                codec,
                metadata::element_meta(field.doc.as_ref(), None, field.default.as_ref())?,
            ))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    let storage = match record.encoding {
        RecordEncoding::NativeOpaque => EncodedRecordStorage::NativeOpaque,
        _ => EncodedRecordStorage::Decoded,
    };
    let native_opaque_exports = (storage == EncodedRecordStorage::NativeOpaque)
        .then(|| lower_native_opaque_exports(allocator, record, &fields))
        .transpose()?;

    let mut lowered = EncodedRecordDecl::new(
        ids.record(&record.id)?,
        CanonicalName::from(&record.name),
        metadata::decl_meta(record.doc.as_ref(), record.deprecated.as_ref()),
        fields,
        initializers,
        record_methods,
        codecs::plan(
            index,
            ids,
            &TypeExpr::record(
                record.id.clone(),
                boltffi_ast::Path::single(record.name.spelling()),
            ),
            ValueRef::self_value(),
        )?,
    )
    .with_storage(storage);
    if let Some(exports) = native_opaque_exports {
        lowered = lowered.with_native_opaque_exports(exports);
    }
    Ok(lowered)
}

fn lower_native_opaque_exports(
    allocator: &mut SymbolAllocator,
    record: &SourceRecord,
    fields: &[EncodedFieldDecl],
) -> Result<NativeOpaqueRecordExports, LowerError> {
    if fields
        .iter()
        .any(|field| !native_opaque_field_supported(field.ty()))
    {
        return Err(LowerError::unsupported_type(
            UnsupportedType::NativeOpaqueRecordField,
        ));
    }

    let drop = allocator.mint_native_opaque_record_export(&record.name, "drop")?;
    let dsize = allocator.mint_native_opaque_record_export(&record.name, "dsize")?;
    let fields = record
        .fields
        .iter()
        .zip(fields.iter())
        .map(|(source, field)| {
            let field_suffix = source_member_suffix(&source.name);
            let (ty, optional) = match field.ty() {
                TypeRef::Optional(inner) => (inner.as_ref(), true),
                ty => (ty, false),
            };
            let has = optional
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("has_{field_suffix}"),
                    )
                })
                .transpose()?;
            let get = matches!(ty, TypeRef::Primitive(_))
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("get_{field_suffix}"),
                    )
                })
                .transpose()?;
            let borrow = matches!(ty, TypeRef::String | TypeRef::Bytes)
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("borrow_{field_suffix}"),
                    )
                })
                .transpose()?;
            let interned_tag = matches!(ty, TypeRef::InternedString { .. })
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("interned_{field_suffix}_tag"),
                    )
                })
                .transpose()?;
            let interned_id = matches!(ty, TypeRef::InternedString { .. })
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("interned_{field_suffix}_id"),
                    )
                })
                .transpose()?;
            let interned_borrow_dynamic = matches!(ty, TypeRef::InternedString { .. })
                .then(|| {
                    allocator.mint_native_opaque_record_export(
                        &record.name,
                        &format!("interned_{field_suffix}_borrow_dynamic"),
                    )
                })
                .transpose()?;
            Ok(NativeOpaqueFieldExports::new(
                field.key().clone(),
                has,
                get,
                borrow,
                interned_tag,
                interned_id,
                interned_borrow_dynamic,
            ))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    Ok(NativeOpaqueRecordExports::new(drop, dsize, fields))
}

fn native_opaque_source_field_supported(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Option(inner) => native_opaque_owned_field_supported(inner),
        ty => native_opaque_owned_field_supported(ty),
    }
}

fn native_opaque_owned_field_supported(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::InternedString { .. }
    ) || matches!(
        ty,
        TypeExpr::Vec(element) if matches!(element.as_ref(), TypeExpr::Primitive(boltffi_ast::Primitive::U8))
    )
}

// Keep this check after lowering as a defense-in-depth assertion over the IR.
// Source validation above is authoritative because TypeRef intentionally erases
// whether String/Bytes originated from owned or borrowed Rust syntax.
fn native_opaque_field_supported(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::InternedString { .. } => true,
        TypeRef::Optional(inner) => {
            !matches!(inner.as_ref(), TypeRef::Optional(_)) && native_opaque_field_supported(inner)
        }
        _ => false,
    }
}

fn source_member_suffix(name: &boltffi_ast::SourceName) -> String {
    name.parts()
        .map(|part| part.as_str())
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_")
}

fn to_snake_case(input: &str) -> String {
    let mut output = String::new();
    let mut prev_is_lower_or_digit = false;
    for ch in input.chars() {
        if matches!(ch, '-' | ' ' | '.') {
            if !output.ends_with('_') {
                output.push('_');
            }
            prev_is_lower_or_digit = false;
        } else if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
        } else {
            output.push(ch);
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, DefaultValue as SourceDefaultValue,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, EnumDef,
        ExecutionKind, FieldDef, FnSig, FnTrait, FnTraitKind, IntegerLiteral, MapKind, MethodDef,
        MethodId as SourceMethodId, PackageInfo as SourcePackage, ParameterDef, ParameterPassing,
        Path as SourcePath, Primitive, Receiver, RecordDef, RecordEncoding, ReprAttr, ReprItem,
        ReturnDef, Source, SourceContract, TypeExpr, VariantDef, VariantPayload,
    };

    use crate::lower::lower;
    use crate::{
        BindingErrorKind, Bindings, ByteSize, CanonicalName, CodecNode, Decl, DefaultValue,
        DirectRecordDecl, DirectValueType, DirectVectorElementType, EncodedRecordDecl,
        EncodedRecordStorage, EnumId, ErrorDecl, ExecutionDecl, ExportedMethodDecl, FieldKey,
        HandlePresence, InitializerDecl, IntegerValue, IntrinsicOp, LowerError, LowerErrorKind,
        Native, NativeSymbol, OpNode, OutOfRust, ParamPlan, Primitive as BindingPrimitive,
        ReadPlan, Receive, RecordDecl, RecordId, ReturnPlan, SurfaceLower, TypeRef,
        UnsupportedType, ValueRef, Wasm32, native, wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn record(id: &str, record_name: &str, fields: Vec<FieldDef>) -> RecordDef {
        let mut record = RecordDef::new(id.into(), name(record_name));
        record.fields = fields;
        record
    }

    fn field(field_name: &str, type_expr: TypeExpr) -> FieldDef {
        FieldDef::new(name(field_name), type_expr)
    }

    fn direct_record(bindings: &Bindings<Native>) -> &DirectRecordDecl<Native> {
        match bindings.decls().first() {
            Some(Decl::Record(record)) => match record.as_ref() {
                RecordDecl::Direct(record) => record,
                RecordDecl::Encoded(_) => panic!("expected direct record"),
            },
            _ => panic!("expected record declaration"),
        }
    }

    fn encoded_record(bindings: &Bindings<Native>) -> &EncodedRecordDecl<Native> {
        match bindings.decls().first() {
            Some(Decl::Record(record)) => match record.as_ref() {
                RecordDecl::Encoded(record) => record,
                RecordDecl::Direct(_) => panic!("expected encoded record"),
            },
            _ => panic!("expected record declaration"),
        }
    }

    fn sequence_len_value(node: &CodecNode) -> &ValueRef {
        match node {
            CodecNode::Sequence { len, .. } => match len.node() {
                OpNode::Intrinsic {
                    intrinsic: IntrinsicOp::SequenceLen,
                    args,
                } => match args.first() {
                    Some(OpNode::Value(value)) => value,
                    _ => panic!("expected sequence length value argument"),
                },
                _ => panic!("expected sequence length intrinsic"),
            },
            _ => panic!("expected sequence codec"),
        }
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

    #[test]
    fn classifies_unannotated_primitive_record_as_encoded() {
        let bindings = lower_record::<Native>(record(
            "demo::Point",
            "point",
            vec![
                field("x", TypeExpr::Primitive(Primitive::F64)),
                field("y", TypeExpr::Primitive(Primitive::F64)),
            ],
        ));
        let record = encoded_record(&bindings);

        assert_eq!(record.fields().len(), 2);
    }

    #[test]
    fn lays_out_direct_record_with_padding() {
        let mut source = record(
            "demo::Header",
            "header",
            vec![
                field("tag", TypeExpr::Primitive(Primitive::U8)),
                field("count", TypeExpr::Primitive(Primitive::U32)),
            ],
        );
        source.repr = ReprAttr::new(vec![ReprItem::C]);
        let bindings = lower_record::<Native>(source);
        let record = direct_record(&bindings);

        assert_eq!(record.layout().size(), ByteSize::new(8));
        assert_eq!(record.layout().alignment().get(), 4);
        assert_eq!(
            record
                .layout()
                .fields()
                .iter()
                .map(|field| field.offset().get())
                .collect::<Vec<_>>(),
            vec![0, 4]
        );
    }

    #[test]
    fn classifies_empty_record_as_encoded() {
        let bindings = lower_record::<Native>(record("demo::Empty", "empty", Vec::new()));
        let record = encoded_record(&bindings);

        assert_eq!(record.fields().len(), 0);
    }

    #[test]
    fn classifies_platform_sized_field_as_encoded() {
        let bindings = lower_record::<Native>(record(
            "demo::Index",
            "index",
            vec![field("raw", TypeExpr::Primitive(Primitive::USize))],
        ));

        encoded_record(&bindings);
    }

    #[test]
    fn classifies_non_primitive_field_as_encoded() {
        let bindings = lower_record::<Native>(record(
            "demo::User",
            "user",
            vec![field("name", TypeExpr::String)],
        ));

        encoded_record(&bindings);
    }

    #[test]
    fn native_opaque_record_lowers_helper_symbols_into_binding_ir() {
        let mut source = record(
            "demo::Snapshot",
            "snapshot",
            vec![
                field("count", TypeExpr::Primitive(Primitive::U32)),
                field("name", TypeExpr::option(TypeExpr::String)),
            ],
        );
        source.encoding = RecordEncoding::NativeOpaque;

        let bindings = lower_record::<Native>(source);
        let record = encoded_record(&bindings);
        let exports = record
            .native_opaque_exports()
            .expect("native opaque helper exports");

        assert_eq!(record.storage(), EncodedRecordStorage::NativeOpaque);
        assert_eq!(
            exports.drop().name().as_str(),
            "boltffi_record_native_snapshot_drop"
        );
        assert_eq!(
            exports.dsize().name().as_str(),
            "boltffi_record_native_snapshot_dsize"
        );
        assert_eq!(exports.fields().len(), 2);
        assert_eq!(
            exports.fields()[0]
                .get()
                .expect("primitive getter")
                .name()
                .as_str(),
            "boltffi_record_native_snapshot_get_count"
        );
        let name_exports = &exports.fields()[1];
        assert_eq!(
            name_exports
                .has()
                .expect("optional string presence")
                .name()
                .as_str(),
            "boltffi_record_native_snapshot_has_name"
        );
        assert_eq!(
            name_exports
                .borrow()
                .expect("string borrow")
                .name()
                .as_str(),
            "boltffi_record_native_snapshot_borrow_name"
        );
        assert!(
            bindings
                .symbols()
                .symbols()
                .iter()
                .any(|symbol| symbol.name().as_str() == "boltffi_record_native_snapshot_drop")
        );

        let serialized = serde_json::to_vec(&bindings).expect("serialize opaque bindings");
        let decoded: Bindings<Native> =
            serde_json::from_slice(&serialized).expect("deserialize opaque bindings");
        let decoded_exports = encoded_record(&decoded)
            .native_opaque_exports()
            .expect("serialized opaque helper exports");
        assert_eq!(decoded_exports, exports);
    }

    #[test]
    fn native_opaque_record_rejects_unsupported_field_shapes() {
        let mut source = record(
            "demo::Snapshot",
            "snapshot",
            vec![field("names", TypeExpr::vec(TypeExpr::String))],
        );
        source.encoding = RecordEncoding::NativeOpaque;

        let error = lower_record_result::<Native>(source)
            .expect_err("opaque records only expose supported accessor fields");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordField)
        ));
    }

    #[test]
    fn native_opaque_record_rejects_borrowed_string_and_byte_fields_before_type_lowering() {
        for (name, type_expr) in [
            ("borrowed_str", TypeExpr::Str),
            (
                "borrowed_bytes",
                TypeExpr::slice(TypeExpr::Primitive(Primitive::U8)),
            ),
            ("optional_borrowed_str", TypeExpr::option(TypeExpr::Str)),
            (
                "optional_borrowed_bytes",
                TypeExpr::option(TypeExpr::slice(TypeExpr::Primitive(Primitive::U8))),
            ),
            (
                "nested_option",
                TypeExpr::option(TypeExpr::option(TypeExpr::String)),
            ),
        ] {
            let mut source = record("demo::Snapshot", "snapshot", vec![field(name, type_expr)]);
            source.encoding = RecordEncoding::NativeOpaque;

            let error = lower_record_result::<Native>(source)
                .expect_err("borrowed and nested optional fields cannot expose opaque accessors");
            assert!(matches!(
                error.kind(),
                LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordField)
            ));
        }
    }

    #[test]
    fn native_opaque_record_accepts_owned_string_and_byte_vector_fields() {
        let mut source = record(
            "demo::Snapshot",
            "snapshot",
            vec![
                field("name", TypeExpr::String),
                field("payload", TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))),
                field("maybe_name", TypeExpr::option(TypeExpr::String)),
                field(
                    "maybe_payload",
                    TypeExpr::option(TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))),
                ),
            ],
        );
        source.encoding = RecordEncoding::NativeOpaque;

        let bindings = lower_record::<Native>(source);
        assert!(encoded_record(&bindings).is_native_opaque());
    }

    #[test]
    fn classifies_transparent_record_as_encoded() {
        let mut record = record(
            "demo::UserId",
            "user_id",
            vec![field("raw", TypeExpr::Primitive(Primitive::U64))],
        );
        record.repr = ReprAttr::new(vec![ReprItem::Transparent]);

        let bindings = lower_record::<Native>(record);

        encoded_record(&bindings);
    }

    #[test]
    fn sequence_field_codec_counts_the_field_value() {
        let bindings = lower_record::<Native>(record(
            "demo::Names",
            "names",
            vec![field("items", TypeExpr::vec(TypeExpr::String))],
        ));
        let record = encoded_record(&bindings);
        let value = sequence_len_value(record.fields()[0].write().root());

        assert_eq!(
            value.path(),
            &[FieldKey::Named(CanonicalName::single("items"))]
        );
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

    fn value_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn record_decl_methods(
        bindings: &Bindings<Native>,
    ) -> &[ExportedMethodDecl<Native, NativeSymbol>] {
        direct_record(bindings).methods()
    }

    fn record_decl_initializers(bindings: &Bindings<Native>) -> &[InitializerDecl<Native>] {
        direct_record(bindings).initializers()
    }

    fn lower_record<S: SurfaceLower>(record: RecordDef) -> Bindings<S> {
        lower_record_result::<S>(record).expect("record should lower")
    }

    fn lower_record_result<S: SurfaceLower>(record: RecordDef) -> Result<Bindings<S>, LowerError> {
        lower_records_result::<S>(vec![record])
    }

    fn lower_records<S: SurfaceLower>(records: Vec<RecordDef>) -> Bindings<S> {
        lower_records_result::<S>(records).expect("record should lower")
    }

    fn lower_records_result<S: SurfaceLower>(
        records: Vec<RecordDef>,
    ) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.records = records;
        lower::<S>(&contract)
    }

    fn lower_contract<S: SurfaceLower>(
        records: Vec<RecordDef>,
        enums: Vec<EnumDef>,
    ) -> Bindings<S> {
        let mut contract = package();
        contract.records = records;
        contract.enums = enums;
        lower::<S>(&contract).expect("record should lower")
    }

    fn record_methods_at<S: SurfaceLower>(
        bindings: &Bindings<S>,
        index: usize,
    ) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        match bindings.decls().get(index) {
            Some(Decl::Record(record)) => record.methods(),
            _ => panic!("expected record declaration"),
        }
    }

    fn lower_point_methods<S: SurfaceLower>(methods: Vec<MethodDef>) -> Bindings<S> {
        lower_record::<S>(point_record_with_methods(methods))
    }

    fn lower_point_method<S: SurfaceLower>(method: MethodDef) -> Bindings<S> {
        lower_point_methods::<S>(vec![method])
    }

    fn point_record_with_methods(methods: Vec<MethodDef>) -> RecordDef {
        let mut record = point_record();
        record.methods = methods;
        record
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

    #[test]
    fn lowers_record_method_with_self_receiver_and_primitive_params() {
        let bindings = lower_point_method::<Native>(method_with(
            "translate",
            Receiver::Shared,
            vec![
                value_param("dx", TypeExpr::Primitive(Primitive::F64)),
                value_param("dy", TypeExpr::Primitive(Primitive::F64)),
            ],
            ReturnDef::Void,
        ));
        let methods = record_decl_methods(&bindings);

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name().parts().len(), 1);
        assert_eq!(
            methods[0].target().name().as_str(),
            "boltffi_method_record_demo_point_translate"
        );

        let callable = methods[0].callable();
        assert_eq!(callable.receiver(), Some(Receive::ByRef));
        assert_eq!(callable.params().len(), 2);
        assert!(matches!(
            callable.params()[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                receive: Receive::ByValue,
            }
        ));
        assert!(matches!(
            callable.params()[1].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                receive: Receive::ByValue,
            }
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn lowers_new_initializer_in_initializer_symbol_lane() {
        let bindings = lower_point_method::<Native>(method_with(
            "new",
            Receiver::None,
            vec![
                value_param("x", TypeExpr::Primitive(Primitive::F64)),
                value_param("y", TypeExpr::Primitive(Primitive::F64)),
            ],
            ReturnDef::value(TypeExpr::SelfType),
        ));
        let initializers = record_decl_initializers(&bindings);

        assert_eq!(initializers.len(), 1);
        assert_eq!(
            initializers[0].symbol().name().as_str(),
            "boltffi_init_record_demo_point_new"
        );
        assert_eq!(initializers[0].callable().receiver(), None);
        assert_eq!(initializers[0].callable().params().len(), 2);
    }

    #[test]
    fn lowers_named_initializer_in_initializer_symbol_lane() {
        let bindings = lower_point_method::<Native>(method_with(
            "from_xy",
            Receiver::None,
            vec![
                value_param("x", TypeExpr::Primitive(Primitive::F64)),
                value_param("y", TypeExpr::Primitive(Primitive::F64)),
            ],
            ReturnDef::value(TypeExpr::SelfType),
        ));
        let initializers = record_decl_initializers(&bindings);

        assert_eq!(initializers.len(), 1);
        assert_eq!(
            initializers[0].symbol().name().as_str(),
            "boltffi_init_record_demo_point_from_xy"
        );
    }

    #[test]
    fn result_self_initializer_uses_success_out_and_encoded_error() {
        let bindings = lower_point_method::<Native>(method_with(
            "try_new",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(TypeExpr::SelfType),
                err: Box::new(TypeExpr::String),
            }),
        ));
        let initializers = record_decl_initializers(&bindings);

        assert_eq!(initializers.len(), 1);
        assert!(record_decl_methods(&bindings).is_empty());
        assert_eq!(
            initializers[0].symbol().name().as_str(),
            "boltffi_init_record_demo_point_try_new"
        );
        assert_eq!(
            initializers[0].callable().returns().plan(),
            &ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Record(RecordId::from_raw(0)),
            }
        );
        assert_encoded_string_error(initializers[0].callable().error());
    }

    #[test]
    fn result_self_initializer_on_encoded_record_uses_encoded_success_out() {
        let mut user = user_record();
        user.methods.push(method_with(
            "try_new",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(TypeExpr::SelfType),
                err: Box::new(TypeExpr::String),
            }),
        ));
        let bindings = lower_record::<Native>(user);
        let record = encoded_record(&bindings);
        let initializers = record.initializers();

        assert_eq!(initializers.len(), 1);
        assert!(record.methods().is_empty());
        assert_eq!(
            initializers[0].callable().returns().plan(),
            &ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Record(RecordId::from_raw(0)),
                codec: ReadPlan::new(CodecNode::EncodedRecord(RecordId::from_raw(0))),
                shape: native::BufferShape::Buffer,
            }
        );
        assert_encoded_string_error(initializers[0].callable().error());
    }

    #[test]
    fn method_returning_self_lowers_self_to_owning_record_type() {
        let bindings = lower_point_method::<Native>(method_with(
            "shifted",
            Receiver::Shared,
            vec![value_param("delta", TypeExpr::Primitive(Primitive::F64))],
            ReturnDef::value(TypeExpr::SelfType),
        ));
        let methods = record_decl_methods(&bindings);
        let returns = methods[0].callable().returns().plan();

        match returns {
            ReturnPlan::DirectViaReturnSlot { ty } => {
                assert_eq!(ty, &DirectValueType::Record(RecordId::from_raw(0)))
            }
            other => panic!("expected direct record return, got {other:?}"),
        }
    }

    #[test]
    fn async_record_method_lowers_to_poll_handle_protocol_on_native() {
        let mut async_method = method("compute", Receiver::Shared);
        async_method.execution = ExecutionKind::Async;

        let bindings = lower_point_method::<Native>(async_method);
        let methods = record_decl_methods(&bindings);
        let callable = methods[0].callable();
        let start_symbol = methods[0].target();

        assert_eq!(
            start_symbol.name().as_str(),
            "boltffi_method_record_demo_point_compute"
        );
        match callable.execution() {
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
                    "boltffi_async_method_record_demo_point_compute_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_record_method_lowers_to_poll_handle_protocol_on_wasm32() {
        let mut async_method = method("compute", Receiver::Shared);
        async_method.execution = ExecutionKind::Async;

        let bindings = lower_point_method::<Wasm32>(async_method);
        let methods = record_methods_at(&bindings, 0);
        let callable = methods[0].callable();

        match callable.execution() {
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
                    "boltffi_async_method_record_demo_point_compute_poll_sync"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_method_record_demo_point_compute_panic_message"
                );
            }
            other => panic!("expected wasm32 PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_record_method_registers_lifecycle_symbols_in_symbol_table() {
        let mut async_method = method("compute", Receiver::Shared);
        async_method.execution = ExecutionKind::Async;

        let bindings = lower_point_method::<Native>(async_method);
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();

        assert!(names.contains(&"boltffi_method_record_demo_point_compute"));
        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_poll"));
        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_complete"));
        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_cancel"));
        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_free"));
        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_panic_message"));
    }

    #[test]
    fn async_lifecycle_symbols_do_not_collide_with_user_suffix_method_names() {
        let mut async_compute = method("compute", Receiver::Shared);
        async_compute.execution = ExecutionKind::Async;
        let sync_compute_poll = method("compute_poll", Receiver::Shared);

        let bindings = lower_point_methods::<Native>(vec![async_compute, sync_compute_poll]);
        let names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();

        assert!(names.contains(&"boltffi_async_method_record_demo_point_compute_poll"));
        assert!(names.contains(&"boltffi_method_record_demo_point_compute_poll"));
    }

    #[test]
    fn async_record_initializer_lowers_to_poll_handle_protocol() {
        let mut new_point = method_with(
            "new",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::SelfType),
        );
        new_point.execution = ExecutionKind::Async;

        let bindings = lower_point_method::<Native>(new_point);
        let initializers = record_decl_initializers(&bindings);

        assert_eq!(
            initializers[0].symbol().name().as_str(),
            "boltffi_init_record_demo_point_new"
        );
        match initializers[0].callable().execution() {
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
                    "boltffi_async_init_record_demo_point_new_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_init_record_demo_point_new_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_init_record_demo_point_new_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_init_record_demo_point_new_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_init_record_demo_point_new_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn method_returning_result_uses_success_out_and_encoded_error() {
        let bindings = lower_point_method::<Native>(method_with(
            "try_distance",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(TypeExpr::Primitive(Primitive::F64)),
                err: Box::new(TypeExpr::String),
            }),
        ));
        let methods = record_decl_methods(&bindings);

        assert_eq!(methods.len(), 1);
        assert_eq!(
            methods[0].callable().returns().plan(),
            &ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
            }
        );
        assert_encoded_string_error(methods[0].callable().error());
    }

    #[test]
    fn method_native_symbol_is_registered_in_table() {
        let bindings = lower_point_methods::<Native>(vec![
            method_with(
                "new",
                Receiver::None,
                vec![
                    value_param("x", TypeExpr::Primitive(Primitive::F64)),
                    value_param("y", TypeExpr::Primitive(Primitive::F64)),
                ],
                ReturnDef::value(TypeExpr::SelfType),
            ),
            method_with(
                "translate",
                Receiver::Shared,
                vec![
                    value_param("dx", TypeExpr::Primitive(Primitive::F64)),
                    value_param("dy", TypeExpr::Primitive(Primitive::F64)),
                ],
                ReturnDef::Void,
            ),
        ]);
        let symbols = bindings.symbols();
        let names: Vec<&str> = symbols
            .symbols()
            .iter()
            .map(|s| s.name().as_str())
            .collect();

        assert_eq!(
            names,
            vec![
                "boltffi_init_record_demo_point_new",
                "boltffi_method_record_demo_point_translate"
            ]
        );
    }

    fn ref_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        let mut parameter = value_param(param_name, type_expr);
        parameter.passing = ParameterPassing::Ref;
        parameter
    }

    fn ref_mut_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        let mut parameter = value_param(param_name, type_expr);
        parameter.passing = ParameterPassing::RefMut;
        parameter
    }

    fn closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        closure_with_trait(FnTraitKind::Fn, parameters, returns)
    }

    fn closure_with_trait(
        kind: FnTraitKind,
        parameters: Vec<TypeExpr>,
        returns: ReturnDef,
    ) -> TypeExpr {
        TypeExpr::impl_fn(FnTrait::new(kind, FnSig::new(parameters, returns)))
    }

    fn boxed_closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_fn(FnTrait::new(
            FnTraitKind::Fn,
            FnSig::new(parameters, returns),
        )))
    }

    fn function_pointer(parameters: Vec<TypeExpr>, returns: ReturnDef) -> TypeExpr {
        TypeExpr::fn_ptr(FnSig::new(parameters, returns))
    }

    fn nullable(type_expr: TypeExpr) -> TypeExpr {
        TypeExpr::option(type_expr)
    }

    fn record_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(id.into(), SourcePath::single(path))
    }

    fn enum_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::enumeration(id.into(), SourcePath::single(path))
    }

    fn data_enum(id: &str, enum_name: &str) -> EnumDef {
        let mut enumeration = EnumDef::new(id.into(), name(enum_name));
        enumeration.variants = vec![
            VariantDef::unit(name("none")),
            VariantDef {
                name: name("message").into(),
                discriminant: None,
                payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                doc: None,
                user_attrs: Vec::new(),
                source: Source::exported(),
                source_span: None,
            },
        ];
        enumeration
    }

    fn user_record() -> RecordDef {
        record("demo::User", "User", vec![field("name", TypeExpr::String)])
    }

    fn first_record<S: SurfaceLower>(bindings: &Bindings<S>) -> &RecordDecl<S> {
        match bindings.decls().first() {
            Some(Decl::Record(record)) => record.as_ref(),
            _ => panic!("expected record declaration"),
        }
    }

    fn first_record_methods<S: SurfaceLower>(
        bindings: &Bindings<S>,
    ) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        first_record(bindings).methods()
    }

    fn first_record_initializers<S: SurfaceLower>(bindings: &Bindings<S>) -> &[InitializerDecl<S>] {
        first_record(bindings).initializers()
    }

    #[test]
    fn mutable_receiver_lowers_to_by_mut_ref() {
        let bindings = lower_point_method::<Native>(method("mutate", Receiver::Mutable));
        let methods = first_record_methods(&bindings);

        assert_eq!(methods[0].callable().receiver(), Some(Receive::ByMutRef));
    }

    #[test]
    fn owned_receiver_lowers_to_by_value() {
        let bindings = lower_point_method::<Native>(method("consume", Receiver::Owned));
        let methods = first_record_methods(&bindings);

        assert_eq!(methods[0].callable().receiver(), Some(Receive::ByValue));
    }

    #[test]
    fn static_method_returning_non_self_is_method_not_initializer() {
        let bindings = lower_point_method::<Native>(method_with(
            "origin_x",
            Receiver::None,
            Vec::new(),
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
        ));

        assert_eq!(first_record_initializers(&bindings).len(), 0);
        let methods = first_record_methods(&bindings);
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].callable().receiver(), None);
        assert_eq!(
            methods[0].target().name().as_str(),
            "boltffi_method_record_demo_point_origin_x"
        );
    }

    #[test]
    fn ref_parameter_lowers_to_by_ref_receive() {
        let bindings = lower_point_method::<Native>(method_with(
            "inspect",
            Receiver::Shared,
            vec![ref_param("count", TypeExpr::Primitive(Primitive::I32))],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        assert!(matches!(
            methods[0].callable().params()[0].as_value().unwrap(),
            ParamPlan::Direct {
                receive: Receive::ByRef,
                ..
            }
        ));
    }

    #[test]
    fn ref_mut_parameter_lowers_to_by_mut_ref_receive() {
        let bindings = lower_point_method::<Native>(method_with(
            "update",
            Receiver::Shared,
            vec![ref_mut_param("count", TypeExpr::Primitive(Primitive::I32))],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        assert!(matches!(
            methods[0].callable().params()[0].as_value().unwrap(),
            ParamPlan::Direct {
                receive: Receive::ByMutRef,
                ..
            }
        ));
    }

    #[test]
    fn string_parameter_lowers_to_encoded_with_native_slice_shape() {
        let bindings = lower_point_method::<Native>(method_with(
            "greet",
            Receiver::Shared,
            vec![value_param("name", TypeExpr::String)],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty: TypeRef::String,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {}
            other => panic!("expected encoded String param with slice shape, got {other:?}"),
        }
    }

    #[test]
    fn encoded_vec_parameter_writeplan_value_uses_parameter_name() {
        let bindings = lower_point_method::<Native>(method_with(
            "collect",
            Receiver::Shared,
            vec![value_param("items", TypeExpr::vec(TypeExpr::String))],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let ParamPlan::Encoded { codec, .. } =
            methods[0].callable().params()[0].as_value().unwrap()
        else {
            panic!("expected encoded Vec<String> param");
        };
        assert_eq!(
            codec.value(),
            &ValueRef::named(CanonicalName::single("items"))
        );
    }

    #[test]
    fn option_parameter_lowers_to_encoded() {
        let bindings = lower_point_method::<Native>(method_with(
            "update",
            Receiver::Shared,
            vec![value_param(
                "name",
                TypeExpr::Option(Box::new(TypeExpr::String)),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {
                assert_eq!(ty, &TypeRef::Optional(Box::new(TypeRef::String)));
                assert_eq!(
                    codec.root(),
                    &CodecNode::Optional(Box::new(CodecNode::String))
                );
            }
            other => panic!("expected encoded optional string param, got {other:?}"),
        }
    }

    #[test]
    fn tuple_parameter_lowers_to_encoded() {
        let bindings = lower_point_method::<Native>(method_with(
            "pair",
            Receiver::Shared,
            vec![value_param(
                "couple",
                TypeExpr::tuple(vec![TypeExpr::Primitive(Primitive::I32), TypeExpr::String]),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {
                assert_eq!(
                    ty,
                    &TypeRef::Tuple(vec![
                        TypeRef::Primitive(BindingPrimitive::I32),
                        TypeRef::String
                    ])
                );
                assert_eq!(
                    codec.root(),
                    &CodecNode::Tuple(vec![
                        CodecNode::Primitive(BindingPrimitive::I32),
                        CodecNode::String
                    ])
                );
            }
            other => panic!("expected encoded tuple param, got {other:?}"),
        }
    }

    #[test]
    fn map_parameter_lowers_to_encoded() {
        let bindings = lower_point_method::<Native>(method_with(
            "annotate",
            Receiver::Shared,
            vec![value_param(
                "labels",
                TypeExpr::hash_map(TypeExpr::String, TypeExpr::Primitive(Primitive::I32)),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {
                assert_eq!(
                    ty,
                    &TypeRef::Map {
                        key: Box::new(TypeRef::String),
                        value: Box::new(TypeRef::Primitive(BindingPrimitive::I32)),
                    }
                );
                assert_eq!(
                    codec.root(),
                    &CodecNode::Map {
                        kind: MapKind::Hash,
                        key: Box::new(CodecNode::String),
                        value: Box::new(CodecNode::Primitive(BindingPrimitive::I32)),
                    }
                );
            }
            other => panic!("expected encoded map param, got {other:?}"),
        }
    }

    #[test]
    fn direct_record_parameter_lowers_to_lower_plan_direct() {
        let mut other = record(
            "demo::Path",
            "Path",
            vec![field("len", TypeExpr::Primitive(Primitive::U32))],
        );
        other.methods.push(method_with(
            "contains",
            Receiver::Shared,
            vec![value_param("point", record_type("demo::Point", "Point"))],
            ReturnDef::Void,
        ));

        let bindings = lower_records::<Native>(vec![point_record(), other]);
        let path_methods = record_methods_at(&bindings, 1);

        match path_methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty,
                receive: Receive::ByValue,
            } => assert_eq!(ty, &DirectValueType::Record(RecordId::from_raw(0))),
            other => panic!("expected direct record param, got {other:?}"),
        }
    }

    #[test]
    fn encoded_record_parameter_lowers_to_lower_plan_encoded() {
        let mut other = record(
            "demo::Greeter",
            "Greeter",
            vec![field("seed", TypeExpr::Primitive(Primitive::U32))],
        );
        other.methods.push(method_with(
            "greet_user",
            Receiver::Shared,
            vec![value_param("user", record_type("demo::User", "User"))],
            ReturnDef::Void,
        ));

        let bindings = lower_records::<Native>(vec![user_record(), other]);
        let greeter_methods = record_methods_at(&bindings, 1);

        match greeter_methods[0].callable().params()[0]
            .as_value()
            .unwrap()
        {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {
                assert_eq!(ty, &TypeRef::Record(RecordId::from_raw(0)));
                assert_eq!(
                    codec.root(),
                    &CodecNode::EncodedRecord(RecordId::from_raw(0))
                );
            }
            other => panic!("expected encoded record param, got {other:?}"),
        }
    }

    #[test]
    fn c_style_enum_parameter_lowers_to_lower_plan_direct() {
        let mut direction = EnumDef::new("demo::Direction".into(), name("Direction"));
        direction.variants = vec![
            VariantDef::unit(name("north")),
            VariantDef::unit(name("south")),
        ];

        let bindings = lower_contract::<Native>(
            vec![point_record_with_methods(vec![method_with(
                "face",
                Receiver::Mutable,
                vec![value_param(
                    "heading",
                    enum_type("demo::Direction", "Direction"),
                )],
                ReturnDef::Void,
            )])],
            vec![direction],
        );
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty,
                receive: Receive::ByValue,
            } => assert_eq!(ty, &DirectValueType::Enum(EnumId::from_raw(0))),
            other => panic!("expected direct enum param, got {other:?}"),
        }
    }

    #[test]
    fn data_enum_parameter_lowers_to_lower_plan_encoded() {
        let mut event = EnumDef::new("demo::Event".into(), name("Event"));
        event.variants = vec![
            VariantDef::unit(name("none")),
            VariantDef {
                name: name("message").into(),
                discriminant: None,
                payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                doc: None,
                user_attrs: Vec::new(),
                source: Source::exported(),
                source_span: None,
            },
        ];

        let bindings = lower_contract::<Native>(
            vec![point_record_with_methods(vec![method_with(
                "dispatch",
                Receiver::Shared,
                vec![value_param("event", enum_type("demo::Event", "Event"))],
                ReturnDef::Void,
            )])],
            vec![event],
        );
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => {
                assert_eq!(ty, &TypeRef::Enum(EnumId::from_raw(0)));
                assert_eq!(codec.root(), &CodecNode::DataEnum(EnumId::from_raw(0)));
            }
            other => panic!("expected encoded enum param, got {other:?}"),
        }
    }

    #[test]
    fn closure_parameter_lowers_to_lower_plan_closure_with_callable() {
        let bindings = lower_point_method::<Native>(method_with(
            "on_each",
            Receiver::Shared,
            vec![value_param(
                "callback",
                closure(vec![TypeExpr::Primitive(Primitive::F64)], ReturnDef::Void),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let closure = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure");
        assert_eq!(closure.presence(), HandlePresence::Required);
        assert!(matches!(
            closure.registration().shape(),
            native::ClosureRegistration::InvokeContextRelease
        ));
        let callable = closure.invoke();
        let params = callable.params();
        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                ..
            }
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn nullable_closure_parameter_lowers_to_nullable_crossing() {
        let bindings = lower_point_method::<Native>(method_with(
            "maybe_each",
            Receiver::Shared,
            vec![value_param(
                "callback",
                nullable(closure(
                    vec![TypeExpr::Primitive(Primitive::F64)],
                    ReturnDef::Void,
                )),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let closure = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected nullable closure param");
        assert_eq!(closure.presence(), HandlePresence::Nullable);
        assert_eq!(closure.form(), crate::ClosureForm::Fn);
        assert!(matches!(
            closure.registration().shape(),
            native::ClosureRegistration::InvokeContextRelease
        ));
        assert!(matches!(
            closure.invoke().returns().plan(),
            ReturnPlan::Void
        ));
    }

    #[test]
    fn closure_invoke_contract_flips_encoded_param_and_return_direction() {
        let bindings = lower_point_method::<Native>(method_with(
            "map_name",
            Receiver::Shared,
            vec![value_param(
                "callback",
                closure(vec![TypeExpr::String], ReturnDef::value(TypeExpr::String)),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let callable = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected closure param")
            .invoke();
        match callable.params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Slice,
                receive: (),
                ..
            } => {
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected closure string arg to use read codec, got {other:?}"),
        }
        match callable.returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(codec.value(), &ValueRef::self_value());
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected closure string return to use write codec, got {other:?}"),
        }
    }

    #[test]
    fn closure_return_lowers_to_closure_via_out_pointer_on_native() {
        let mut record = point_record();
        record.methods.push(method_with(
            "project",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(closure(
                vec![TypeExpr::Primitive(Primitive::F64)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
            )),
        ));

        let bindings =
            lower_record_result::<Native>(record).expect("native closure return should lower");
        let methods = direct_record(&bindings).methods();
        let plan = methods[0].callable().returns().plan();

        let closure_crossing = match plan {
            ReturnPlan::ClosureViaOutPointer(crossing) => crossing,
            other => panic!("expected ClosureViaOutPointer, got {other:?}"),
        };
        assert_eq!(closure_crossing.form(), crate::ClosureForm::Fn);
        assert_eq!(closure_crossing.presence(), HandlePresence::Required);
        // Native closure params and returns share the same logical invoke/context
        // marker, but the parent enum keeps the wire positions separate.
        // Parameter closures can only appear through IncomingParam/OutgoingParam;
        // closure returns can only appear through ClosureViaOutPointer.
        assert_eq!(
            closure_crossing.registration().shape(),
            &native::ClosureRegistration::InvokeContextRelease
        );
        let invoke = closure_crossing.invoke();
        assert_eq!(invoke.params().len(), 1);
        match invoke.params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                receive: Receive::ByValue,
            } => {}
            other => panic!("expected f64 direct invoke param, got {other:?}"),
        }
        match invoke.returns().plan() {
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
            } => {}
            other => panic!("expected f64 direct invoke return, got {other:?}"),
        }
    }

    #[test]
    fn nullable_closure_return_lowers_to_nullable_closure_via_out_pointer() {
        let mut record = point_record();
        record.methods.push(method_with(
            "maybe_project",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(nullable(closure(
                vec![TypeExpr::Primitive(Primitive::F64)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
            ))),
        ));

        let bindings =
            lower_record_result::<Native>(record).expect("nullable closure return should lower");
        let methods = direct_record(&bindings).methods();

        match methods[0].callable().returns().plan() {
            ReturnPlan::ClosureViaOutPointer(closure) => {
                assert_eq!(closure.presence(), HandlePresence::Nullable);
                assert_eq!(closure.form(), crate::ClosureForm::Fn);
            }
            other => panic!("expected nullable ClosureViaOutPointer, got {other:?}"),
        }
    }

    #[test]
    fn closure_return_lowers_to_closure_via_out_pointer_on_wasm32() {
        let mut record = point_record();
        record.methods.push(method_with(
            "project",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(closure(
                vec![TypeExpr::Primitive(Primitive::F64)],
                ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
            )),
        ));

        let bindings =
            lower_record_result::<Wasm32>(record).expect("wasm closure return should lower");
        let record_decl = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                crate::Decl::Record(record) => Some(record.as_ref()),
                _ => None,
            })
            .expect("expected record");
        let methods = record_decl.methods();
        let plan = methods[0].callable().returns().plan();

        let closure_param = match plan {
            ReturnPlan::ClosureViaOutPointer(closure_param) => closure_param,
            other => panic!("expected ClosureViaOutPointer, got {other:?}"),
        };

        assert_eq!(closure_param.form(), crate::ClosureForm::Fn);
        let invoke = closure_param.invoke();
        assert_eq!(invoke.params().len(), 1);
        match invoke.params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                receive: Receive::ByValue,
            } => {}
            other => panic!("expected f64 direct invoke param, got {other:?}"),
        }
        match invoke.returns().plan() {
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
            } => {}
            other => panic!("expected f64 direct invoke return, got {other:?}"),
        }

        let call_symbol = closure_param.registration().shape().call().name().as_str();
        let free_symbol = closure_param.registration().shape().free().name().as_str();
        let symbol_names: Vec<&str> = bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect();
        assert!(
            symbol_names.contains(&call_symbol),
            "wasm closure return's call export must register in the symbol table: {symbol_names:?}"
        );
        assert!(
            symbol_names.contains(&free_symbol),
            "wasm closure return's free export must register in the symbol table: {symbol_names:?}"
        );
    }

    #[test]
    fn result_closure_return_uses_out_pointer_success_and_encoded_error() {
        let mut record = point_record();
        record.methods.push(method_with(
            "try_project",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(closure(
                    vec![TypeExpr::Primitive(Primitive::F64)],
                    ReturnDef::value(TypeExpr::Primitive(Primitive::F64)),
                )),
                err: Box::new(TypeExpr::String),
            }),
        ));

        let bindings =
            lower_record_result::<Wasm32>(record).expect("Result<closure, E> should lower");
        let record_decl = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                crate::Decl::Record(record) => Some(record.as_ref()),
                _ => None,
            })
            .expect("expected record");
        let methods = record_decl.methods();
        let callable = methods[0].callable();

        assert!(
            matches!(
                callable.returns().plan(),
                ReturnPlan::ClosureViaOutPointer(_)
            ),
            "closure success must use an out-pointer so the error can own the return slot"
        );
        match callable.error() {
            ErrorDecl::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => {
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded string error in return slot, got {other:?}"),
        }
    }

    #[test]
    fn closure_with_result_return_lowers_with_encoded_error_channel() {
        let bindings = lower_contract::<Native>(
            vec![point_record_with_methods(vec![method_with(
                "run",
                Receiver::Shared,
                vec![value_param(
                    "callback",
                    closure(
                        vec![TypeExpr::Primitive(Primitive::I32)],
                        ReturnDef::value(TypeExpr::Result {
                            ok: Box::new(TypeExpr::Primitive(Primitive::I32)),
                            err: Box::new(enum_type("demo::ParseError", "ParseError")),
                        }),
                    ),
                )],
                ReturnDef::Void,
            )])],
            vec![data_enum("demo::ParseError", "ParseError")],
        );
        let methods = first_record_methods(&bindings);

        let callable = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure for Result-returning closure")
            .invoke();
        assert!(matches!(
            callable.returns().plan(),
            ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
            }
        ));
        match callable.error() {
            ErrorDecl::EncodedViaReturnSlot {
                ty: TypeRef::Enum(_),
                ..
            } => {}
            other => panic!("expected encoded enum error channel, got {other:?}"),
        }
    }

    #[test]
    fn closure_with_result_unit_return_lowers_with_void_success_and_encoded_error() {
        let bindings = lower_contract::<Native>(
            vec![point_record_with_methods(vec![method_with(
                "run",
                Receiver::Shared,
                vec![value_param(
                    "callback",
                    closure(
                        Vec::new(),
                        ReturnDef::value(TypeExpr::Result {
                            ok: Box::new(TypeExpr::Unit),
                            err: Box::new(enum_type("demo::ParseError", "ParseError")),
                        }),
                    ),
                )],
                ReturnDef::Void,
            )])],
            vec![data_enum("demo::ParseError", "ParseError")],
        );
        let methods = first_record_methods(&bindings);

        let callable = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure for Result<(), E>-returning closure")
            .invoke();
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
        match callable.error() {
            ErrorDecl::EncodedViaReturnSlot {
                ty: TypeRef::Enum(_),
                ..
            } => {}
            other => panic!("expected encoded enum error channel, got {other:?}"),
        }
    }

    #[test]
    fn closure_with_fn_mut_kind_lowers_to_closure_plan() {
        assert_closure_type_lowers(closure_with_trait(
            FnTraitKind::FnMut,
            vec![TypeExpr::Primitive(Primitive::F64)],
            ReturnDef::Void,
        ));
    }

    #[test]
    fn closure_with_fn_once_kind_lowers_to_closure_plan() {
        assert_closure_type_lowers(closure_with_trait(
            FnTraitKind::FnOnce,
            vec![TypeExpr::Primitive(Primitive::F64)],
            ReturnDef::Void,
        ));
    }

    #[test]
    fn closure_with_boxed_fn_kind_lowers_to_closure_plan() {
        assert_closure_type_lowers(boxed_closure(
            vec![TypeExpr::Primitive(Primitive::F64)],
            ReturnDef::Void,
        ));
    }

    #[test]
    fn closure_with_function_pointer_kind_lowers_to_closure_plan() {
        assert_closure_type_lowers(function_pointer(
            vec![TypeExpr::Primitive(Primitive::F64)],
            ReturnDef::Void,
        ));
    }

    fn assert_closure_type_lowers(closure: TypeExpr) {
        let bindings = lower_point_method::<Native>(method_with(
            "on_each",
            Receiver::Shared,
            vec![value_param("callback", closure)],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let closure = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure for {kind:?}");
        assert_eq!(closure.presence(), HandlePresence::Required);
        let callable = closure.invoke();
        assert_eq!(callable.params().len(), 1);
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn closure_inside_vec_parameter_is_rejected() {
        let mut record = point_record();
        record.methods.push(method_with(
            "register",
            Receiver::Shared,
            vec![value_param(
                "callbacks",
                TypeExpr::vec(closure(
                    vec![TypeExpr::Primitive(Primitive::I32)],
                    ReturnDef::Void,
                )),
            )],
            ReturnDef::Void,
        ));

        let error =
            lower_record_result::<Native>(record).expect_err("closure inside Vec is not supported");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::ClosureInValuePosition)
        ));
    }

    #[test]
    fn closure_inside_record_field_is_rejected() {
        let mut record = point_record();
        record.fields.push(field(
            "on_change",
            closure(vec![TypeExpr::Primitive(Primitive::F64)], ReturnDef::Void),
        ));

        let error = lower_record_result::<Native>(record)
            .expect_err("closure inside record field is not supported");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::ClosureInValuePosition)
        ));
    }

    #[test]
    fn string_return_lowers_to_encoded_with_native_buffer_shape() {
        let bindings = lower_point_method::<Native>(method_with(
            "describe",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => {}
            other => panic!("expected encoded String return with buffer shape, got {other:?}"),
        }
    }

    #[test]
    fn vec_of_primitive_return_lowers_to_direct_vec() {
        let bindings = lower_point_method::<Native>(method_with(
            "samples",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::vec(TypeExpr::Primitive(Primitive::F64))),
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().returns().plan() {
            ReturnPlan::DirectVecViaReturnSlot { element } => {
                assert_eq!(
                    element,
                    &DirectVectorElementType::primitive(BindingPrimitive::F64)
                        .expect("f64 is a direct-vector primitive")
                );
            }
            other => panic!("expected DirectVec lift, got {other:?}"),
        }
    }

    #[test]
    fn vec_self_return_substitutes_to_owning_record_and_lowers_direct_vec() {
        let bindings = lower_point_method::<Native>(method_with(
            "neighbours",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::vec(TypeExpr::SelfType)),
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().returns().plan() {
            ReturnPlan::DirectVecViaReturnSlot { element } => {
                assert_eq!(
                    element,
                    &DirectVectorElementType::record(RecordId::from_raw(0))
                );
            }
            other => panic!("expected DirectVec lift, got {other:?}"),
        }
    }

    #[test]
    fn option_self_return_substitutes_to_owning_record() {
        let bindings = lower_point_method::<Native>(method_with(
            "maybe",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::Option(Box::new(TypeExpr::SelfType))),
        ));
        let methods = first_record_methods(&bindings);

        let ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } =
            methods[0].callable().returns().plan()
        else {
            panic!("expected encoded return");
        };
        assert_eq!(
            ty,
            &TypeRef::Optional(Box::new(TypeRef::Record(RecordId::from_raw(0))))
        );
        assert_eq!(
            codec.root(),
            &CodecNode::Optional(Box::new(CodecNode::DirectRecord(RecordId::from_raw(0))))
        );
    }

    #[test]
    fn tuple_with_self_substitutes_each_self_position() {
        let bindings = lower_point_method::<Native>(method_with(
            "pair",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::tuple(vec![
                TypeExpr::SelfType,
                TypeExpr::SelfType,
            ])),
        ));
        let methods = first_record_methods(&bindings);

        let ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } =
            methods[0].callable().returns().plan()
        else {
            panic!("expected encoded return");
        };
        assert_eq!(
            ty,
            &TypeRef::Tuple(vec![
                TypeRef::Record(RecordId::from_raw(0)),
                TypeRef::Record(RecordId::from_raw(0)),
            ])
        );
        assert_eq!(
            codec.root(),
            &CodecNode::Tuple(vec![
                CodecNode::DirectRecord(RecordId::from_raw(0)),
                CodecNode::DirectRecord(RecordId::from_raw(0)),
            ])
        );
    }

    #[test]
    fn closure_with_self_substitutes_in_param_and_return_positions() {
        let bindings = lower_point_method::<Native>(method_with(
            "transform",
            Receiver::Shared,
            vec![value_param(
                "callback",
                closure(
                    vec![TypeExpr::SelfType],
                    ReturnDef::value(TypeExpr::SelfType),
                ),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let callable = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure")
            .invoke();
        let params = callable.params();
        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Record(_),
                ..
            }
        ));
        assert!(matches!(
            callable.returns().plan(),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Record(_),
            }
        ));
    }

    #[test]
    fn self_in_parameter_position_substitutes_to_owning_record() {
        let bindings = lower_point_method::<Native>(method_with(
            "merge",
            Receiver::Mutable,
            vec![value_param("other", TypeExpr::SelfType)],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Direct {
                ty,
                receive: Receive::ByValue,
            } => assert_eq!(ty, &DirectValueType::Record(RecordId::from_raw(0))),
            other => panic!("expected direct self param, got {other:?}"),
        }
    }

    #[test]
    fn wasm32_encoded_param_uses_slice_shape() {
        let bindings = lower_point_method::<Wasm32>(method_with(
            "greet",
            Receiver::Shared,
            vec![value_param("name", TypeExpr::String)],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty: TypeRef::String,
                codec,
                shape: wasm32::BufferShape::Slice,
                receive: Receive::ByValue,
                ..
            } => assert_eq!(codec.root(), &CodecNode::Utf8String),
            other => panic!("expected wasm32 slice param shape, got {other:?}"),
        }
    }

    #[test]
    fn wasm32_encoded_return_uses_packed_shape() {
        let bindings = lower_point_method::<Wasm32>(method_with(
            "describe",
            Receiver::Shared,
            Vec::new(),
            ReturnDef::value(TypeExpr::String),
        ));
        let methods = first_record_methods(&bindings);

        match methods[0].callable().returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => assert_eq!(codec.root(), &CodecNode::Utf8String),
            other => panic!("expected wasm32 packed return shape, got {other:?}"),
        }
    }

    #[test]
    fn wasm32_closure_parameter_lowers_to_lower_plan_closure_with_callable() {
        let bindings = lower_point_method::<Wasm32>(method_with(
            "on_each",
            Receiver::Shared,
            vec![value_param(
                "callback",
                closure(vec![TypeExpr::Primitive(Primitive::F64)], ReturnDef::Void),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let closure = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected ParamPlan::Closure on wasm32");
        assert_eq!(closure.presence(), HandlePresence::Required);
        assert_eq!(
            closure.registration().shape().call().name().as_str(),
            "__boltffi_callback_closure____closure__f64_call"
        );
        assert_eq!(
            closure.registration().shape().free().name().as_str(),
            "__boltffi_callback_closure____closure__f64_free"
        );
        let callable = closure.invoke();
        let params = callable.params();
        assert_eq!(params.len(), 1);
        assert!(matches!(
            params[0].as_value().unwrap(),
            ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::F64),
                ..
            }
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
    }

    #[test]
    fn wasm32_nullable_closure_parameter_lowers_to_nullable_crossing() {
        let bindings = lower_point_method::<Wasm32>(method_with(
            "maybe_each",
            Receiver::Shared,
            vec![value_param(
                "callback",
                nullable(closure(
                    vec![TypeExpr::Primitive(Primitive::F64)],
                    ReturnDef::Void,
                )),
            )],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);

        let closure = methods[0].callable().params()[0]
            .as_closure()
            .expect("expected nullable wasm32 closure param");
        assert_eq!(closure.presence(), HandlePresence::Nullable);
        assert_eq!(
            closure.registration().shape().call().name().as_str(),
            "__boltffi_callback_closure____closure__f64_call"
        );
        assert!(matches!(
            closure.invoke().returns().plan(),
            ReturnPlan::Void
        ));
    }

    #[test]
    fn methods_lower_on_an_encoded_record() {
        let mut record = user_record();
        record.methods.push(method_with(
            "greet",
            Receiver::Shared,
            vec![value_param("greeting", TypeExpr::String)],
            ReturnDef::Void,
        ));

        let bindings = lower_record::<Native>(record);
        let methods = first_record_methods(&bindings);

        assert_eq!(methods.len(), 1);
        assert_eq!(
            methods[0].target().name().as_str(),
            "boltffi_method_record_demo_user_greet"
        );
        let RecordDecl::Encoded(_) = first_record(&bindings) else {
            panic!("expected encoded record");
        };
    }

    #[test]
    fn multiple_initializers_get_sequential_ids_in_source_order() {
        let bindings = lower_point_methods::<Native>(vec![
            method_with(
                "new",
                Receiver::None,
                vec![
                    value_param("x", TypeExpr::Primitive(Primitive::F64)),
                    value_param("y", TypeExpr::Primitive(Primitive::F64)),
                ],
                ReturnDef::value(TypeExpr::SelfType),
            ),
            method_with(
                "from_xy",
                Receiver::None,
                vec![
                    value_param("x", TypeExpr::Primitive(Primitive::F64)),
                    value_param("y", TypeExpr::Primitive(Primitive::F64)),
                ],
                ReturnDef::value(TypeExpr::SelfType),
            ),
            method_with(
                "origin",
                Receiver::None,
                Vec::new(),
                ReturnDef::value(TypeExpr::SelfType),
            ),
        ]);
        let initializers = first_record_initializers(&bindings);

        assert_eq!(initializers.len(), 3);
        assert_eq!(initializers[0].id().raw(), 0);
        assert_eq!(initializers[1].id().raw(), 1);
        assert_eq!(initializers[2].id().raw(), 2);
        assert_eq!(
            initializers[0].name().parts().last().unwrap().as_str(),
            "new"
        );
        assert_eq!(
            initializers[1].name().parts().last().unwrap().as_str(),
            "from_xy"
        );
        assert_eq!(
            initializers[2].name().parts().last().unwrap().as_str(),
            "origin"
        );
    }

    #[test]
    fn multiple_methods_get_sequential_ids_in_source_order() {
        let bindings = lower_point_methods::<Native>(vec![
            method("translate", Receiver::Mutable),
            method("magnitude", Receiver::Shared),
            method("normalize", Receiver::Mutable),
        ]);
        let methods = first_record_methods(&bindings);

        assert_eq!(methods.len(), 3);
        assert_eq!(methods[0].id().raw(), 0);
        assert_eq!(methods[1].id().raw(), 1);
        assert_eq!(methods[2].id().raw(), 2);
        assert_eq!(
            methods[0].name().parts().last().unwrap().as_str(),
            "translate"
        );
        assert_eq!(
            methods[1].name().parts().last().unwrap().as_str(),
            "magnitude"
        );
        assert_eq!(
            methods[2].name().parts().last().unwrap().as_str(),
            "normalize"
        );
    }

    #[test]
    fn method_can_reference_another_record_in_signature() {
        let mut path = record(
            "demo::Path",
            "Path",
            vec![field("count", TypeExpr::Primitive(Primitive::U32))],
        );
        path.methods.push(method_with(
            "find_point",
            Receiver::Shared,
            vec![value_param("key", TypeExpr::Primitive(Primitive::U32))],
            ReturnDef::value(record_type("demo::Point", "Point")),
        ));

        let bindings = lower_records::<Native>(vec![point_record(), path]);
        let path_methods = record_methods_at(&bindings, 1);

        match path_methods[0].callable().returns().plan() {
            ReturnPlan::DirectViaReturnSlot { ty } => {
                assert_eq!(ty, &DirectValueType::Record(RecordId::from_raw(0)));
            }
            other => panic!("expected direct record return, got {other:?}"),
        }
    }

    #[test]
    fn method_can_reference_enum_in_signature() {
        let mut direction = EnumDef::new("demo::Direction".into(), name("Direction"));
        direction.variants = vec![
            VariantDef::unit(name("north")),
            VariantDef::unit(name("south")),
        ];

        let bindings = lower_contract::<Native>(
            vec![point_record_with_methods(vec![method_with(
                "heading",
                Receiver::Shared,
                Vec::new(),
                ReturnDef::value(enum_type("demo::Direction", "Direction")),
            )])],
            vec![direction],
        );
        let methods = first_record_methods(&bindings);

        match methods[0].callable().returns().plan() {
            ReturnPlan::DirectViaReturnSlot { ty } => {
                assert_eq!(ty, &DirectValueType::Enum(EnumId::from_raw(0)));
            }
            other => panic!("expected direct enum return, got {other:?}"),
        }
    }

    #[test]
    fn method_doc_and_deprecation_propagate_to_decl_meta() {
        let mut translate = method("translate", Receiver::Mutable);
        translate.doc = Some(SourceDocComment::new("shifts the point"));
        translate.deprecated = Some(SourceDeprecationInfo {
            note: Some("use shifted instead".to_owned()),
            since: Some("0.2".to_owned()),
        });

        let bindings = lower_point_method::<Native>(translate);
        let methods = first_record_methods(&bindings);
        let meta = methods[0].meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("shifts the point"));
        assert_eq!(
            meta.deprecated().and_then(|d| d.message()),
            Some("use shifted instead")
        );
        assert_eq!(meta.deprecated().and_then(|d| d.since()), Some("0.2"));
    }

    #[test]
    fn parameter_doc_and_default_propagate_to_element_meta() {
        let mut factor = value_param("factor", TypeExpr::Primitive(Primitive::I32));
        factor.doc = Some(SourceDocComment::new("scaling factor"));
        factor.default = Some(SourceDefaultValue::Integer(IntegerLiteral::new(1, "1")));

        let bindings = lower_point_method::<Native>(method_with(
            "scale",
            Receiver::Mutable,
            vec![factor],
            ReturnDef::Void,
        ));
        let methods = first_record_methods(&bindings);
        let meta = methods[0].callable().params()[0].meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("scaling factor"));
        match meta.default() {
            Some(DefaultValue::Integer(value)) => assert_eq!(value, &IntegerValue::new(1)),
            other => panic!("expected integer default, got {other:?}"),
        }
    }

    #[test]
    fn parameter_path_default_is_rejected_without_type_context() {
        let mut factor = value_param("factor", TypeExpr::Primitive(Primitive::I32));
        factor.default = Some(SourceDefaultValue::Path(SourcePath::single("Mode")));

        let error = lower_record_result::<Native>(point_record_with_methods(vec![method_with(
            "scale",
            Receiver::Mutable,
            vec![factor],
            ReturnDef::Void,
        )]))
        .expect_err("path defaults need declared-type validation and must reject here");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::DefaultValue)
        ));
    }

    #[test]
    fn initializer_doc_and_deprecation_propagate_to_decl_meta() {
        let mut new_init = method("new", Receiver::None);
        new_init.doc = Some(SourceDocComment::new("origin point"));
        new_init.deprecated = Some(SourceDeprecationInfo {
            note: Some("use Point::origin instead".to_owned()),
            since: None,
        });
        new_init.returns = ReturnDef::value(TypeExpr::SelfType);

        let bindings = lower_point_method::<Native>(new_init);
        let initializers = first_record_initializers(&bindings);
        let meta = initializers[0].meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("origin point"));
        assert_eq!(
            meta.deprecated().and_then(|d| d.message()),
            Some("use Point::origin instead")
        );
    }

    #[test]
    fn acronym_record_name_lowers_to_snake_cased_symbol() {
        let mut record = record(
            "demo::HTTPHeader",
            "HTTPHeader",
            vec![field("status", TypeExpr::Primitive(Primitive::U16))],
        );
        record.methods.push(method_with(
            "process",
            Receiver::Shared,
            vec![value_param("code", TypeExpr::Primitive(Primitive::U16))],
            ReturnDef::Void,
        ));

        let bindings = lower_record::<Native>(record);
        let methods = first_record_methods(&bindings);

        assert_eq!(
            methods[0].target().name().as_str(),
            "boltffi_method_record_demo_http_header_process"
        );
    }

    #[test]
    fn duplicate_method_names_on_one_record_fail_validation() {
        let record = point_record_with_methods(vec![
            method("translate", Receiver::Mutable),
            method("translate", Receiver::Mutable),
        ]);

        let error = lower_record_result::<Native>(record)
            .expect_err("duplicate symbol should fail validation");

        match error.kind() {
            LowerErrorKind::InvalidBindings(error) => match error.kind() {
                BindingErrorKind::DuplicateSymbolName(name) => {
                    assert_eq!(name, "boltffi_method_record_demo_point_translate");
                }
                other => panic!("expected DuplicateSymbolName, got {other:?}"),
            },
            other => panic!("expected InvalidBindings, got {other:?}"),
        }
    }

    // Negative tests: native opaque record in method/initializer positions.
    // Opaque records are only allowed as synchronous, infallible, exact top-level
    // free-function returns; all method/initializer positions must be rejected.

    fn native_opaque_record_def() -> RecordDef {
        let mut record = RecordDef::new("demo::Snapshot".into(), name("Snapshot"));
        record.encoding = RecordEncoding::NativeOpaque;
        record.fields = vec![field("count", TypeExpr::Primitive(Primitive::U32))];
        record
    }

    fn opaque_record_type() -> TypeExpr {
        TypeExpr::record(
            boltffi_ast::RecordId::new("demo::Snapshot"),
            boltffi_ast::Path::single("Snapshot"),
        )
    }

    #[test]
    fn native_opaque_record_return_in_record_method_is_rejected() {
        // A record method whose return type is another native opaque record should
        // be rejected because opaque records are only valid as free-function returns.
        let other_opaque_id = "demo::OtherSnapshot".into();
        let mut other_opaque = RecordDef::new(other_opaque_id, name("OtherSnapshot"));
        other_opaque.encoding = RecordEncoding::NativeOpaque;
        other_opaque.fields = vec![field("x", TypeExpr::Primitive(Primitive::U32))];
        let other_type = TypeExpr::record(
            boltffi_ast::RecordId::new("demo::OtherSnapshot"),
            boltffi_ast::Path::single("OtherSnapshot"),
        );
        let mut m = method("get_other", Receiver::Shared);
        m.returns = ReturnDef::value(other_type);
        let mut point = point_record();
        point.methods = vec![m];
        let error = lower_records_result::<Native>(vec![point, other_opaque])
            .expect_err("native opaque return in record method must be rejected");
        assert!(
            matches!(
                error.kind(),
                LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordMethod)
            ),
            "expected NativeOpaqueRecordMethod, got {error:?}"
        );
    }

    #[test]
    fn native_opaque_record_return_in_record_static_method_is_rejected() {
        // A no-receiver method (which lowers as an initializer slot) returning a
        // native opaque record should be rejected: opaque records are free-function-only.
        let mut m = method("create_snapshot", Receiver::None);
        m.returns = ReturnDef::value(opaque_record_type());
        let opaque = native_opaque_record_def();
        let mut point = point_record();
        point.methods = vec![m];
        let error = lower_records_result::<Native>(vec![point, opaque])
            .expect_err("native opaque return in static record method must be rejected");
        assert!(
            matches!(
                error.kind(),
                LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordMethod)
            ),
            "expected NativeOpaqueRecordMethod for static method, got {error:?}"
        );
    }

    #[test]
    fn native_opaque_record_rejects_primitive_method_before_callable_lowering() {
        let mut method = method("count", Receiver::Shared);
        method.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::U32));
        let mut opaque = native_opaque_record_def();
        opaque.methods = vec![method];

        let error = lower_record_result::<Native>(opaque)
            .expect_err("opaque record methods are never admitted");
        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordMethod)
        ));
    }

    #[test]
    fn native_opaque_record_rejects_string_initializer_before_callable_lowering() {
        let mut initializer = method("new", Receiver::None);
        initializer.returns = ReturnDef::value(TypeExpr::String);
        let mut opaque = native_opaque_record_def();
        opaque.methods = vec![initializer];

        let error = lower_record_result::<Native>(opaque)
            .expect_err("opaque record initializers are never admitted");
        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordMethod)
        ));
    }

    #[test]
    fn native_opaque_record_true_initializer_returning_self_is_rejected() {
        // A true record initializer (no receiver, returns Self) on a native
        // opaque record must be rejected: the initializer would wrap the
        // CallableOwner::Record context and is not a free-function return.
        let self_type = TypeExpr::record(
            boltffi_ast::RecordId::new("demo::Snapshot"),
            boltffi_ast::Path::single("Snapshot"),
        );
        // is_initializer: Receiver::None + returns_owner (type matches the record)
        let mut ctor = method("new", Receiver::None);
        ctor.returns = ReturnDef::value(self_type);
        let mut opaque = native_opaque_record_def();
        opaque.methods = vec![ctor];
        let error = lower_record_result::<Native>(opaque)
            .expect_err("native opaque record initializer must be rejected");
        assert!(
            matches!(
                error.kind(),
                LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordMethod)
            ),
            "expected NativeOpaqueRecordMethod for true initializer, got {error:?}"
        );
    }

    #[test]
    fn native_opaque_record_method_returning_option_self_is_rejected() {
        // Regression: Option<Self> on a native opaque record method must be
        // rejected. Previously, Self substitution ran after the nested-opaque
        // admission check, so Option<SelfType> evaded the guard and lowered
        // as encoded. The fix moves the check to after substitute_self_type.
        let mut m = method("maybe_self", Receiver::Shared);
        m.returns = ReturnDef::value(TypeExpr::Option(Box::new(TypeExpr::SelfType)));
        let mut opaque = native_opaque_record_def();
        opaque.methods = vec![m];
        let error = lower_record_result::<Native>(opaque)
            .expect_err("Option<Self> return on native opaque record method must be rejected");
        assert!(
            matches!(
                error.kind(),
                LowerErrorKind::UnsupportedType(
                    UnsupportedType::NativeOpaqueRecordNestedReturn
                        | UnsupportedType::NativeOpaqueRecordMethod
                )
            ),
            "expected NativeOpaqueRecordNestedReturn or NativeOpaqueRecordMethod, got {error:?}"
        );
    }

    #[test]
    fn lowered_method_callable_has_synchronous_execution_and_no_error_channel() {
        let bindings = lower_point_method::<Native>(method("translate", Receiver::Mutable));
        let methods = first_record_methods(&bindings);
        let callable = methods[0].callable();

        assert!(matches!(
            callable.execution(),
            ExecutionDecl::Synchronous(_)
        ));
        assert!(matches!(callable.error(), ErrorDecl::None(_)));
    }
}
