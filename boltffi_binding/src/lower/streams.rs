//! Stream declaration lowering.
//!
//! Walks every [`StreamDef`] the source contract exposes and produces a
//! [`StreamDecl<S>`] carrying the owner, mode, item transport plan, the
//! surface handle carrier for the session token, and the six native
//! symbols foreign code links to drive the stream protocol.
//!
//! [`StreamDef`]: boltffi_ast::StreamDef
//! [`StreamDecl<S>`]: crate::StreamDecl
//! [`StreamProtocol`]: crate::StreamProtocol
//! [`Surface::HandleCarrier`]: crate::Surface::HandleCarrier
//! [`TypeRef`]: crate::TypeRef

use boltffi_ast::{StreamDef as SourceStream, TypeExpr};

use crate::{
    ByteSize, CanonicalName, DirectValueType, Primitive as BindingPrimitive, ReadPlan, StreamDecl,
    StreamDeclParts, StreamItemPlan, StreamMode, StreamProtocol, ValueRef,
};

use super::{
    LowerError, codecs, enums,
    error::UnsupportedType,
    ids::DeclarationIds,
    index::Index,
    layout, metadata, records,
    surface::SurfaceLower,
    symbol::{StreamLifecycle, SymbolAllocator},
    types,
};

pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<StreamDecl<S>>, LowerError> {
    index
        .streams()
        .iter()
        .map(|stream| lower_one::<S>(index, ids, allocator, stream))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    stream: &SourceStream,
) -> Result<StreamDecl<S>, LowerError> {
    let stream_id = ids.stream(&stream.id)?;
    let owner = stream
        .owner
        .as_ref()
        .map(|owner| ids.class(owner))
        .transpose()?;
    let item = lower_item::<S>(index, ids, &stream.item_type)?;
    let protocol = build_protocol(allocator, stream.id.as_str())?;
    Ok(StreamDecl::new(StreamDeclParts {
        id: stream_id,
        name: CanonicalName::from(&stream.name),
        meta: metadata::decl_meta(stream.doc.as_ref(), stream.deprecated.as_ref()),
        owner,
        mode: lower_mode(stream.mode),
        handle: S::stream_handle_carrier(),
        item,
        protocol,
    }))
}

fn lower_item<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<StreamItemPlan<S>, LowerError> {
    validate_item_type(type_expr)?;
    match type_expr {
        TypeExpr::Primitive(primitive) => {
            let size = BindingPrimitive::from(*primitive).byte_size::<S>();
            direct_item(ids, type_expr, size)
        }
        TypeExpr::Enum { id, .. } => match index.enumeration(id) {
            Some(enumeration) if enums::is_c_style(enumeration) => {
                let repr = enums::c_style_repr(enumeration)
                    .ok_or_else(|| LowerError::unsupported_type(UnsupportedType::EnumRepr))?;
                direct_item(ids, type_expr, repr.primitive().byte_size::<S>())
            }
            _ => encoded_item::<S>(index, ids, type_expr),
        },
        TypeExpr::Record { id, .. } => match index.record(id) {
            Some(record) if records::is_direct(record) => {
                direct_item(ids, type_expr, layout::compute(record)?.size())
            }
            _ => encoded_item::<S>(index, ids, type_expr),
        },
        _ => encoded_item::<S>(index, ids, type_expr),
    }
}

fn direct_item<S: SurfaceLower>(
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    size: ByteSize,
) -> Result<StreamItemPlan<S>, LowerError> {
    Ok(StreamItemPlan::Direct {
        ty: direct_value_type(ids, type_expr)?,
        size,
    })
}

fn direct_value_type(
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<DirectValueType, LowerError> {
    match type_expr {
        TypeExpr::Primitive(primitive) => Ok(DirectValueType::primitive(BindingPrimitive::from(
            *primitive,
        ))),
        TypeExpr::Record { id, .. } => Ok(DirectValueType::record(ids.record(id)?)),
        TypeExpr::Enum { id, .. } => Ok(DirectValueType::enumeration(ids.enumeration(id)?)),
        _ => Err(LowerError::unsupported_type(UnsupportedType::StreamItem)),
    }
}

fn encoded_item<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<StreamItemPlan<S>, LowerError> {
    let ty = types::lower(ids, type_expr)?;
    let root = codecs::node(index, ids, type_expr, ValueRef::self_value())?;
    Ok(StreamItemPlan::Encoded {
        ty,
        read: ReadPlan::new(root),
        shape: S::encoded_return_shape(),
    })
}

fn validate_item_type(type_expr: &TypeExpr) -> Result<(), LowerError> {
    match type_expr {
        TypeExpr::Class { .. }
        | TypeExpr::Dyn(_)
        | TypeExpr::ImplTrait(_)
        | TypeExpr::FnPtr(_)
        | TypeExpr::Boxed(_)
        | TypeExpr::Arc(_)
        | TypeExpr::Unit
        | TypeExpr::SelfType
        | TypeExpr::Parameter(_) => Err(LowerError::unsupported_type(UnsupportedType::StreamItem)),
        TypeExpr::Vec(inner) | TypeExpr::Slice(inner) | TypeExpr::Option(inner) => {
            validate_item_type(inner)
        }
        TypeExpr::Tuple(elements) => elements.iter().try_for_each(validate_item_type),
        TypeExpr::Result { ok, err } => {
            validate_item_type(ok)?;
            validate_item_type(err)
        }
        TypeExpr::Map { key, value, .. } => {
            validate_item_type(key)?;
            validate_item_type(value)
        }
        TypeExpr::Primitive(_)
        | TypeExpr::String
        | TypeExpr::Str
        | TypeExpr::Builtin(_)
        | TypeExpr::Record { .. }
        | TypeExpr::Enum { .. }
        | TypeExpr::Custom { .. } => Ok(()),
    }
}

fn lower_mode(mode: boltffi_ast::StreamMode) -> StreamMode {
    match mode {
        boltffi_ast::StreamMode::Async => StreamMode::Async,
        boltffi_ast::StreamMode::Batch => StreamMode::Batch,
        boltffi_ast::StreamMode::Callback => StreamMode::Callback,
    }
}

fn build_protocol(
    allocator: &mut SymbolAllocator,
    source_id: &str,
) -> Result<StreamProtocol, LowerError> {
    let subscribe = allocator.mint_stream(source_id, StreamLifecycle::Subscribe)?;
    let pop_batch = allocator.mint_stream(source_id, StreamLifecycle::PopBatch)?;
    let wait = allocator.mint_stream(source_id, StreamLifecycle::Wait)?;
    let poll = allocator.mint_stream(source_id, StreamLifecycle::Poll)?;
    let unsubscribe = allocator.mint_stream(source_id, StreamLifecycle::Unsubscribe)?;
    let free = allocator.mint_stream(source_id, StreamLifecycle::Free)?;
    Ok(StreamProtocol::new(
        subscribe,
        pop_batch,
        wait,
        poll,
        unsubscribe,
        free,
    ))
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ClassDef, ClassId as SourceClassId,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, EnumDef,
        EnumId as SourceEnumId, FieldDef, PackageInfo as SourcePackage, Path as SourcePath,
        Primitive, RecordDef, RecordId as SourceRecordId, ReprAttr, ReprItem, SourceContract,
        StreamDef, StreamId as SourceStreamId, StreamMode, TraitDef, TraitId as SourceTraitId,
        TypeExpr, VariantDef,
    };

    use crate::lower::{LowerError, LowerErrorKind, UnsupportedType, lower};
    use crate::{
        Bindings, ByteSize, CanonicalName, CodecNode, Decl, DirectValueType, Native,
        Primitive as BindingPrimitive, ReadPlan, StreamDecl, StreamId, StreamItemPlan,
        StreamMode as BindingStreamMode, SurfaceLower, TypeRef, Wasm32, native, wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn stream(id: &str, stream_name: &str, item_type: TypeExpr) -> StreamDef {
        StreamDef::new(SourceStreamId::new(id), name(stream_name), item_type)
    }

    fn stream_with_mode(
        id: &str,
        stream_name: &str,
        item_type: TypeExpr,
        mode: StreamMode,
    ) -> StreamDef {
        let mut stream = stream(id, stream_name, item_type);
        stream.mode = mode;
        stream
    }

    fn direct_record(id: &str, record_name: &str) -> RecordDef {
        let mut record = RecordDef::new(SourceRecordId::new(id), name(record_name));
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record.fields.push(FieldDef::new(
            name("x"),
            TypeExpr::Primitive(Primitive::F64),
        ));
        record.fields.push(FieldDef::new(
            name("y"),
            TypeExpr::Primitive(Primitive::F64),
        ));
        record
    }

    fn c_style_enum(id: &str, enum_name: &str) -> EnumDef {
        let mut enumeration = EnumDef::new(SourceEnumId::new(id), name(enum_name));
        enumeration.repr = ReprAttr::new(vec![ReprItem::Primitive(Primitive::U32)]);
        enumeration.variants.push(VariantDef::unit(name("failed")));
        enumeration
    }

    fn lower_streams<S: SurfaceLower>(streams: Vec<StreamDef>) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.streams = streams;
        lower::<S>(&contract)
    }

    fn lower_streams_ok<S: SurfaceLower>(streams: Vec<StreamDef>) -> Bindings<S> {
        lower_streams::<S>(streams).expect("streams should lower")
    }

    fn stream_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&StreamDecl<S>> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Stream(stream) => Some(stream.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn only_stream<S: SurfaceLower>(bindings: &Bindings<S>) -> &StreamDecl<S> {
        let decls = stream_decls(bindings);
        assert_eq!(decls.len(), 1, "expected exactly one stream declaration");
        decls[0]
    }

    fn symbol_names<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&str> {
        bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect()
    }

    #[test]
    fn result_stream_item_marks_error_enum_payload() {
        let error_id = SourceEnumId::new("demo::LoadError");
        let mut contract = package();
        contract
            .enums
            .push(c_style_enum(error_id.as_str(), "LoadError"));
        contract.streams.push(stream(
            "demo::loads",
            "loads",
            TypeExpr::Result {
                ok: Box::new(TypeExpr::String),
                err: Box::new(TypeExpr::enumeration(
                    error_id,
                    SourcePath::single("LoadError"),
                )),
            },
        ));

        let bindings = lower::<Native>(&contract).expect("stream should lower");
        let enumeration = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Enum(enumeration) => Some(enumeration.as_ref()),
                _ => None,
            })
            .expect("error enum should lower");

        assert!(enumeration.is_error_payload());
    }

    #[test]
    fn stream_lowers_with_all_six_protocol_symbols() {
        let bindings = lower_streams_ok::<Native>(vec![stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        )]);
        let decl = only_stream(&bindings);
        let protocol = decl.protocol();

        assert_eq!(decl.name(), &CanonicalName::single("events"));
        assert_eq!(
            decl.item(),
            &StreamItemPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::U32),
                size: ByteSize::new(4),
            }
        );
        assert_eq!(
            protocol.subscribe().name().as_str(),
            "boltffi_stream_demo_events_subscribe"
        );
        assert_eq!(
            protocol.pop_batch().name().as_str(),
            "boltffi_stream_demo_events_pop_batch"
        );
        assert_eq!(
            protocol.wait().name().as_str(),
            "boltffi_stream_demo_events_wait"
        );
        assert_eq!(
            protocol.poll().name().as_str(),
            "boltffi_stream_demo_events_poll"
        );
        assert_eq!(
            protocol.unsubscribe().name().as_str(),
            "boltffi_stream_demo_events_unsubscribe"
        );
        assert_eq!(
            protocol.free().name().as_str(),
            "boltffi_stream_demo_events_free"
        );
    }

    #[test]
    fn native_stream_uses_u64_handle_carrier() {
        let bindings = lower_streams_ok::<Native>(vec![stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        )]);

        assert_eq!(only_stream(&bindings).handle(), native::HandleCarrier::U64);
    }

    #[test]
    fn wasm32_stream_uses_u32_handle_carrier() {
        let bindings = lower_streams_ok::<Wasm32>(vec![stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        )]);

        assert_eq!(only_stream(&bindings).handle(), wasm32::HandleCarrier::U32);
    }

    #[test]
    fn class_owned_stream_preserves_owner() {
        let mut contract = package();
        contract
            .classes
            .push(ClassDef::new("demo::Engine".into(), name("Engine")));
        let mut emissions = stream(
            "demo::Engine::emissions",
            "emissions",
            TypeExpr::Primitive(Primitive::U32),
        );
        emissions.owner = Some(SourceClassId::new("demo::Engine"));
        contract.streams.push(emissions);

        let bindings = lower::<Native>(&contract).expect("class-owned stream should lower");
        let decl = stream_decls(&bindings)[0];

        assert_eq!(decl.owner(), Some(crate::ClassId::from_raw(0)));
        assert_eq!(
            decl.protocol().subscribe().name().as_str(),
            "boltffi_stream_demo_engine_emissions_subscribe"
        );
    }

    #[test]
    fn unknown_stream_owner_is_rejected() {
        let mut events = stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        );
        events.owner = Some(SourceClassId::new("demo::Missing"));

        let error = lower_streams::<Native>(vec![events]).expect_err("unknown owner must reject");

        assert!(matches!(error.kind(), LowerErrorKind::UnknownClass(id) if id == "demo::Missing"));
    }

    #[test]
    fn stream_mode_is_preserved_in_ir() {
        let async_bindings = lower_streams_ok::<Native>(vec![stream_with_mode(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
            StreamMode::Async,
        )]);
        let batch_bindings = lower_streams_ok::<Native>(vec![stream_with_mode(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
            StreamMode::Batch,
        )]);
        let callback_bindings = lower_streams_ok::<Native>(vec![stream_with_mode(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
            StreamMode::Callback,
        )]);

        assert_eq!(
            only_stream(&async_bindings).mode(),
            BindingStreamMode::Async
        );
        assert_eq!(
            only_stream(&batch_bindings).mode(),
            BindingStreamMode::Batch
        );
        assert_eq!(
            only_stream(&callback_bindings).mode(),
            BindingStreamMode::Callback
        );
    }

    #[test]
    fn stream_item_string_lowers_to_encoded_item_plan() {
        let bindings =
            lower_streams_ok::<Native>(vec![stream("demo::lines", "lines", TypeExpr::String)]);

        assert_eq!(
            only_stream(&bindings).item(),
            &StreamItemPlan::Encoded {
                ty: TypeRef::String,
                read: ReadPlan::new(CodecNode::String),
                shape: native::BufferShape::Buffer,
            }
        );
    }

    #[test]
    fn native_platform_sized_primitive_stream_item_uses_64_bit_size() {
        let bindings = lower_streams_ok::<Native>(vec![stream(
            "demo::offsets",
            "offsets",
            TypeExpr::Primitive(Primitive::USize),
        )]);

        assert_eq!(
            only_stream(&bindings).item(),
            &StreamItemPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::USize),
                size: ByteSize::new(8),
            }
        );
    }

    #[test]
    fn wasm_platform_sized_primitive_stream_item_uses_32_bit_size() {
        let bindings = lower_streams_ok::<Wasm32>(vec![stream(
            "demo::offsets",
            "offsets",
            TypeExpr::Primitive(Primitive::USize),
        )]);

        assert_eq!(
            only_stream(&bindings).item(),
            &StreamItemPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::USize),
                size: ByteSize::new(4),
            }
        );
    }

    #[test]
    fn direct_record_stream_item_carries_size() {
        let mut contract = package();
        contract.records.push(direct_record("demo::Point", "Point"));
        contract.streams.push(stream(
            "demo::points",
            "points",
            TypeExpr::record(
                SourceRecordId::new("demo::Point"),
                SourcePath::single("Point"),
            ),
        ));

        let bindings = lower::<Native>(&contract).expect("direct record stream should lower");

        assert_eq!(
            only_stream(&bindings).item(),
            &StreamItemPlan::Direct {
                ty: DirectValueType::Record(crate::RecordId::from_raw(0)),
                size: ByteSize::new(16),
            }
        );
    }

    #[test]
    fn stream_item_class_handle_is_rejected() {
        let mut contract = package();
        contract
            .classes
            .push(ClassDef::new("demo::Engine".into(), name("Engine")));
        contract.streams.push(stream(
            "demo::engines",
            "engines",
            TypeExpr::class(
                SourceClassId::new("demo::Engine"),
                SourcePath::single("Engine"),
            ),
        ));

        let error = lower::<Native>(&contract).expect_err("handle item must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::StreamItem)
        ));
    }

    #[test]
    fn nested_callback_stream_item_is_rejected() {
        let mut contract = package();
        contract.traits.push(TraitDef::new(
            SourceTraitId::new("demo::Listener"),
            name("Listener"),
        ));
        contract.streams.push(stream(
            "demo::listeners",
            "listeners",
            TypeExpr::option(TypeExpr::boxed(TypeExpr::dyn_trait(
                SourceTraitId::new("demo::Listener"),
                SourcePath::single("Listener"),
            ))),
        ));

        let error = lower::<Native>(&contract).expect_err("nested callback item must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::StreamItem)
        ));
    }

    #[test]
    fn stream_protocol_symbols_appear_in_native_symbol_table() {
        let bindings = lower_streams_ok::<Native>(vec![stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        )]);
        let names = symbol_names(&bindings);

        for suffix in [
            "subscribe",
            "pop_batch",
            "wait",
            "poll",
            "unsubscribe",
            "free",
        ] {
            let expected = format!("boltffi_stream_demo_events_{suffix}");
            assert!(
                names.contains(&expected.as_str()),
                "missing symbol `{expected}` in {names:?}"
            );
        }
    }

    #[test]
    fn duplicate_stream_source_ids_are_rejected() {
        let error = lower_streams::<Native>(vec![
            stream(
                "demo::events",
                "events",
                TypeExpr::Primitive(Primitive::U32),
            ),
            stream(
                "demo::events",
                "events_again",
                TypeExpr::Primitive(Primitive::U32),
            ),
        ])
        .expect_err("duplicate stream id must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::DuplicateSourceId { .. }
        ));
    }

    #[test]
    fn multiple_streams_get_sequential_ids_in_source_order() {
        let bindings = lower_streams_ok::<Native>(vec![
            stream("demo::one", "one", TypeExpr::Primitive(Primitive::U32)),
            stream("demo::two", "two", TypeExpr::Primitive(Primitive::U32)),
            stream("demo::three", "three", TypeExpr::Primitive(Primitive::U32)),
        ]);
        let ids: Vec<u32> = stream_decls(&bindings)
            .into_iter()
            .map(|decl| decl.id().raw())
            .collect();

        assert_eq!(ids, vec![0, 1, 2]);
        assert_eq!(stream_decls(&bindings)[0].id(), StreamId::from_raw(0));
    }

    #[test]
    fn stream_doc_and_deprecation_propagate_to_decl_meta() {
        let mut events = stream(
            "demo::events",
            "events",
            TypeExpr::Primitive(Primitive::U32),
        );
        events.doc = Some(SourceDocComment::new("event stream"));
        events.deprecated = Some(SourceDeprecationInfo {
            note: Some("use events_v2".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = lower_streams_ok::<Native>(vec![events]);
        let meta = only_stream(&bindings).meta();

        assert_eq!(meta.doc().map(|doc| doc.as_str()), Some("event stream"));
        assert_eq!(
            meta.deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use events_v2")
        );
    }
}
