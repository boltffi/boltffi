use boltffi_ast::{ClassDef as SourceClass, ClassThreadSafety as SourceClassThreadSafety};

use crate::{CanonicalName, ClassDecl, ClassDeclParts, ClassThreadSafety, InvalidClassDecl};

use super::{
    LowerError, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata, methods,
    surface::SurfaceLower, symbol::SymbolAllocator,
};

/// Lowers every class in the source contract.
///
/// `allocator` is shared across the whole pass so each class's
/// release symbol and each method/initializer symbol receives a
/// unique [`SymbolId`] inside the [`Bindings<S>`] under construction.
///
/// [`SymbolId`]: crate::SymbolId
/// [`Bindings<S>`]: crate::Bindings
pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<ClassDecl<S>>, LowerError> {
    index
        .classes()
        .iter()
        .map(|class| lower_one(index, ids, allocator, class))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    class: &SourceClass,
) -> Result<ClassDecl<S>, LowerError> {
    let class_id = ids.class(&class.id)?;
    let canonical = CanonicalName::from(&class.name);
    let release = allocator.mint_class_release(class.id.as_str())?;
    let initializers = methods::lower_class_initializers::<S>(index, ids, allocator, class)?;
    let class_methods = methods::lower_class_methods::<S>(index, ids, allocator, class)?;

    ClassDecl::new(ClassDeclParts {
        id: class_id,
        name: canonical,
        meta: metadata::decl_meta(class.doc.as_ref(), class.deprecated.as_ref()),
        thread_safety: ClassThreadSafety::from(class.thread_safety),
        handle: S::class_handle_carrier(),
        release,
        initializers,
        methods: class_methods,
    })
    .map_err(lower_invalid_class)
}

impl From<SourceClassThreadSafety> for ClassThreadSafety {
    fn from(value: SourceClassThreadSafety) -> Self {
        match value {
            SourceClassThreadSafety::RequireSendSync => Self::RequireSendSync,
            SourceClassThreadSafety::UnsafeSingleThreaded => Self::UnsafeSingleThreaded,
        }
    }
}

fn lower_invalid_class(error: InvalidClassDecl) -> LowerError {
    match error {
        InvalidClassDecl::MutableReceiverRequiresUnsafeSingleThreaded => {
            LowerError::unsupported_type(
                UnsupportedType::MutableClassReceiverRequiresUnsafeSingleThreaded,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ClassDef, ClassThreadSafety as SourceClassThreadSafety,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, EnumDef,
        ExecutionKind, FieldDef, MethodDef, MethodId as SourceMethodId,
        PackageInfo as SourcePackage, ParameterDef, Path as SourcePath, Primitive, Receiver,
        RecordDef, ReprAttr, ReprItem, ReturnDef, SourceContract, TypeExpr, VariantDef,
    };

    use crate::lower::lower;
    use crate::{
        BindingErrorKind, Bindings, CanonicalName, ClassDecl, ClassId, ClassThreadSafety,
        CodecNode, Decl, DirectValueType, EnumId, ErrorDecl, ExecutionDecl, FutureMobility,
        HandlePresence, HandleTarget, InitializerId, LowerError, LowerErrorKind, MethodId, Native,
        NativeSymbol, ParamPlan, Primitive as BindingPrimitive, Receive, RecordId, ReturnPlan,
        ReturnTypeRef, Surface, SurfaceLower, TypeRef, UnsupportedType, Wasm32, native, wasm32,
    };
    use serde_json::{Value, json};

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn class(id: &str, class_name: &str, methods: Vec<MethodDef>) -> ClassDef {
        let mut class = ClassDef::new(id.into(), name(class_name));
        class.methods = methods;
        class
    }

    fn method(method_name: &str, receiver: Receiver, returns: ReturnDef) -> MethodDef {
        let mut method = MethodDef::new(
            SourceMethodId::new(method_name.to_owned()),
            name(method_name),
            receiver,
        );
        method.returns = returns;
        method
    }

    fn param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn field(field_name: &str, type_expr: TypeExpr) -> FieldDef {
        FieldDef::new(name(field_name), type_expr)
    }

    fn class_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::class(id.into(), SourcePath::single(path))
    }

    fn nullable_class_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::option(class_type(id, path))
    }

    fn record(id: &str, record_name: &str, fields: Vec<FieldDef>) -> RecordDef {
        let mut record = RecordDef::new(id.into(), name(record_name));
        record.repr = ReprAttr::new(vec![ReprItem::C]);
        record.fields = fields;
        record
    }

    fn enumeration(id: &str, enum_name: &str, variants: Vec<VariantDef>) -> EnumDef {
        let mut enumeration = EnumDef::new(id.into(), name(enum_name));
        enumeration.variants = variants;
        enumeration
    }

    fn lower_class<S: SurfaceLower>(class: ClassDef) -> Bindings<S> {
        let mut contract = package();
        contract.classes.push(class);
        lower::<S>(&contract).expect("class should lower")
    }

    fn lower_contract<S: SurfaceLower>(
        contract: SourceContract,
    ) -> Result<Bindings<S>, LowerError> {
        lower::<S>(&contract)
    }

    fn class_by_id<S: Surface>(bindings: &Bindings<S>, id: ClassId) -> &ClassDecl<S> {
        bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Class(class) if class.id() == id => Some(class.as_ref()),
                _ => None,
            })
            .expect("expected class declaration")
    }

    fn symbol_name(symbol: &NativeSymbol) -> &str {
        symbol.name().as_str()
    }

    fn symbol_names<S: Surface>(bindings: &Bindings<S>) -> Vec<&str> {
        bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect()
    }

    #[test]
    fn lowers_class_with_initializer_method_release_and_metadata() {
        let mut new = method("new", Receiver::None, ReturnDef::value(TypeExpr::SelfType));
        new.parameters
            .push(param("seed", TypeExpr::Primitive(Primitive::U64)));
        let start = method("start", Receiver::Mutable, ReturnDef::Void);
        let mut source = class("demo::Engine", "Engine", vec![new, start]);
        source.thread_safety = SourceClassThreadSafety::UnsafeSingleThreaded;
        source.doc = Some(SourceDocComment::new("Runs work."));
        source.deprecated = Some(SourceDeprecationInfo::new(
            Some("use Runtime".to_owned()),
            Some("0.24.0".to_owned()),
        ));

        let bindings = lower_class::<Native>(source);
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert_eq!(class.id(), ClassId::from_raw(0));
        assert_eq!(class.name(), &CanonicalName::single("Engine"));
        assert_eq!(
            class.meta().doc().map(|doc| doc.as_str()),
            Some("Runs work.")
        );
        assert_eq!(
            class
                .meta()
                .deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use Runtime")
        );
        assert_eq!(
            class
                .meta()
                .deprecated()
                .and_then(|deprecated| deprecated.since()),
            Some("0.24.0")
        );
        assert_eq!(class.handle(), native::HandleCarrier::U64);
        assert_eq!(
            symbol_name(class.release()),
            "boltffi_release_class_demo_engine"
        );

        let initializer = class.initializers().first().expect("expected initializer");
        assert_eq!(class.initializers().len(), 1);
        assert_eq!(initializer.id(), InitializerId::from_raw(0));
        assert_eq!(initializer.name(), &CanonicalName::single("new"));
        assert_eq!(
            symbol_name(initializer.symbol()),
            "boltffi_init_class_demo_engine_new"
        );
        assert_eq!(
            initializer.returns(),
            &ReturnTypeRef::Value(TypeRef::Class(ClassId::from_raw(0)))
        );
        assert_eq!(initializer.callable().receiver(), None);
        assert_eq!(
            initializer
                .callable()
                .params()
                .first()
                .map(|param| param.as_value().unwrap()),
            Some(&ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::U64),
                receive: Receive::ByValue,
            })
        );
        assert_eq!(
            initializer.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Required,
            }
        );
        assert!(matches!(initializer.callable().error(), ErrorDecl::None(_)));
        assert!(matches!(
            initializer.callable().execution(),
            ExecutionDecl::Synchronous(_)
        ));

        let class_method = class.methods().first().expect("expected method");
        assert_eq!(class.methods().len(), 1);
        assert_eq!(class_method.id(), MethodId::from_raw(0));
        assert_eq!(class_method.name(), &CanonicalName::single("start"));
        assert_eq!(
            symbol_name(class_method.target()),
            "boltffi_method_class_demo_engine_start"
        );
        assert_eq!(class_method.callable().receiver(), Some(Receive::ByMutRef));
        assert_eq!(class_method.callable().returns().plan(), &ReturnPlan::Void);
    }

    #[test]
    fn lowers_class_thread_safety_policy() {
        let mut source = class(
            "demo::Engine",
            "Engine",
            vec![method("start", Receiver::Mutable, ReturnDef::Void)],
        );
        source.thread_safety = SourceClassThreadSafety::UnsafeSingleThreaded;

        let bindings = lower_class::<Native>(source);
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert_eq!(
            class.thread_safety(),
            ClassThreadSafety::UnsafeSingleThreaded
        );
    }

    #[test]
    fn result_self_initializer_uses_handle_out_and_encoded_error() {
        let try_new = method(
            "try_new",
            Receiver::None,
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(TypeExpr::SelfType),
                err: Box::new(TypeExpr::String),
            }),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![try_new]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let initializer = class.initializers().first().expect("expected initializer");

        assert_eq!(class.initializers().len(), 1);
        assert!(class.methods().is_empty());
        assert_eq!(
            symbol_name(initializer.symbol()),
            "boltffi_init_class_demo_engine_try_new"
        );
        assert_eq!(
            initializer.callable().returns().plan(),
            &ReturnPlan::HandleViaOutPointer {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Required,
            }
        );
        match initializer.callable().error() {
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
    fn keeps_static_non_constructor_as_method() {
        let version = method(
            "version",
            Receiver::None,
            ReturnDef::value(TypeExpr::Primitive(Primitive::I32)),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![version]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert!(class.initializers().is_empty());
        let class_method = class.methods().first().expect("expected method");
        assert_eq!(class.methods().len(), 1);
        assert_eq!(class_method.callable().receiver(), None);
        assert_eq!(
            symbol_name(class_method.target()),
            "boltffi_method_class_demo_engine_version"
        );
        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
            }
        );
    }

    #[test]
    fn lowers_self_and_named_class_references_to_exact_handle_targets() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut merge = method(
            "merge",
            Receiver::Shared,
            ReturnDef::value(TypeExpr::SelfType),
        );
        merge.parameters.push(param("other", TypeExpr::SelfType));
        merge
            .parameters
            .push(param("driver", class_type("demo::Driver", "Driver")));
        let engine = class("demo::Engine", "Engine", vec![merge]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("classes should lower");
        let class = class_by_id(&bindings, ClassId::from_raw(1));
        let class_method = class.methods().first().expect("expected method");

        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .map(|param| param.as_value().unwrap()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(1)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            })
        );
        assert_eq!(
            class_method
                .callable()
                .params()
                .get(1)
                .map(|param| param.as_value().unwrap()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            })
        );
        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(1)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Required,
            }
        );
    }

    #[test]
    fn lowers_wasm32_class_handles_to_surface_carrier() {
        let mut clone = method(
            "clone_handle",
            Receiver::Shared,
            ReturnDef::value(TypeExpr::SelfType),
        );
        clone.parameters.push(param("other", TypeExpr::SelfType));
        let bindings = lower_class::<Wasm32>(class("demo::Engine", "Engine", vec![clone]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let class_method = class.methods().first().expect("expected method");

        assert_eq!(class.handle(), wasm32::HandleCarrier::U32);
        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .map(|param| param.as_value().unwrap()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: wasm32::HandleCarrier::U32,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            })
        );
        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: wasm32::HandleCarrier::U32,
                presence: HandlePresence::Required,
            }
        );
    }

    #[test]
    fn registers_release_initializer_and_method_symbols_in_order() {
        let new = method("new", Receiver::None, ReturnDef::value(TypeExpr::SelfType));
        let start = method("start", Receiver::Shared, ReturnDef::Void);
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![new, start]));

        assert_eq!(
            symbol_names(&bindings),
            vec![
                "boltffi_release_class_demo_engine",
                "boltffi_init_class_demo_engine_new",
                "boltffi_method_class_demo_engine_start",
            ]
        );
    }

    #[test]
    fn class_method_named_free_uses_method_lane_not_release_lane() {
        let free = method("free", Receiver::Shared, ReturnDef::Void);
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![free]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));
        let method = class.methods().first().expect("expected method");

        assert_eq!(
            symbol_name(class.release()),
            "boltffi_release_class_demo_engine"
        );
        assert_eq!(
            symbol_name(method.target()),
            "boltffi_method_class_demo_engine_free"
        );
    }

    #[test]
    fn same_leaf_class_names_use_source_id_in_symbols() {
        let bindings = lower_contract::<Native>({
            let mut contract = package();
            contract
                .classes
                .push(class("demo::audio::Engine", "Engine", Vec::new()));
            contract
                .classes
                .push(class("demo::video::Engine", "Engine", Vec::new()));
            contract
        })
        .expect("same leaf names should lower");

        assert_eq!(
            symbol_names(&bindings),
            vec![
                "boltffi_release_class_demo_audio_engine",
                "boltffi_release_class_demo_video_engine",
            ]
        );
    }

    #[test]
    fn duplicate_class_method_symbols_fail_with_exact_name() {
        let error = lower_contract::<Native>({
            let mut contract = package();
            let mut class = class(
                "demo::Engine",
                "Engine",
                vec![
                    method("start", Receiver::Shared, ReturnDef::Void),
                    method("start", Receiver::Mutable, ReturnDef::Void),
                ],
            );
            class.thread_safety = SourceClassThreadSafety::UnsafeSingleThreaded;
            contract.classes.push(class);
            contract
        })
        .expect_err("duplicate method symbols should fail");

        match error.kind() {
            LowerErrorKind::InvalidBindings(binding_error) => {
                assert!(matches!(
                    binding_error.kind(),
                    BindingErrorKind::DuplicateSymbolName(name)
                        if name == "boltffi_method_class_demo_engine_start"
                ));
            }
            other => panic!("expected invalid bindings, got {other:?}"),
        }
    }

    #[test]
    fn rejects_owned_class_receiver() {
        let consume = method("consume", Receiver::Owned, ReturnDef::Void);
        let error = lower_contract::<Native>({
            let mut contract = package();
            contract
                .classes
                .push(class("demo::Engine", "Engine", vec![consume]));
            contract
        })
        .expect_err("owned receiver should be rejected");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::OwnedClassReceiver)
        ));
    }

    #[test]
    fn rejects_mutable_class_receiver_without_unsafe_single_threaded_export() {
        let error = lower_contract::<Native>({
            let mut contract = package();
            contract.classes.push(class(
                "demo::Engine",
                "Engine",
                vec![method("start", Receiver::Mutable, ReturnDef::Void)],
            ));
            contract
        })
        .expect_err("default class export should reject mutable receiver methods");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(
                UnsupportedType::MutableClassReceiverRequiresUnsafeSingleThreaded
            )
        ));
    }

    #[test]
    fn deserialized_bindings_reject_mutable_class_receiver_without_unsafe_single_threaded_export() {
        let bindings = lower_class::<Native>({
            let mut class = class(
                "demo::Engine",
                "Engine",
                vec![method("start", Receiver::Mutable, ReturnDef::Void)],
            );
            class.thread_safety = SourceClassThreadSafety::UnsafeSingleThreaded;
            class
        });
        let mut value = serde_json::to_value(&bindings).expect("bindings serialize");
        let Value::Array(decls) = &mut value["decls"] else {
            panic!("serialized bindings decls must be an array");
        };
        let class = decls
            .iter_mut()
            .find_map(|decl| decl.get_mut("Class"))
            .expect("serialized class declaration");
        class["thread_safety"] = json!("RequireSendSync");

        let error =
            serde_json::from_value::<Bindings<Native>>(value).expect_err("invalid contract");

        assert!(
            error
                .to_string()
                .contains("mutable class receivers require")
        );
    }

    #[test]
    fn serialized_class_callables_stay_top_level_for_version_compatibility() {
        let bindings = lower_class::<Native>(class(
            "demo::Engine",
            "Engine",
            vec![method("version", Receiver::Shared, ReturnDef::Void)],
        ));

        let value = serde_json::to_value(&bindings).expect("bindings serialize");
        let class = value["decls"]
            .as_array()
            .and_then(|decls| decls.iter().find_map(|decl| decl.get("Class")))
            .expect("serialized class declaration");

        assert!(class.get("callables").is_none());
        assert!(class.get("thread_safety").is_some());
        assert!(class.get("initializers").is_some());
        assert!(class.get("methods").is_some());
        serde_json::from_value::<Bindings<Native>>(value).expect("top-level class shape reads");
    }

    #[test]
    fn class_methods_reference_record_and_enum_ids_exactly() {
        let point = record(
            "demo::Point",
            "Point",
            vec![
                field("x", TypeExpr::Primitive(Primitive::F64)),
                field("y", TypeExpr::Primitive(Primitive::F64)),
            ],
        );
        let direction = enumeration(
            "demo::Direction",
            "Direction",
            vec![
                VariantDef::unit(name("North")),
                VariantDef::unit(name("South")),
            ],
        );
        let locate = method(
            "locate",
            Receiver::Shared,
            ReturnDef::value(TypeExpr::record(
                "demo::Point".into(),
                SourcePath::single("Point"),
            )),
        );
        let direction_method = method(
            "direction",
            Receiver::Shared,
            ReturnDef::value(TypeExpr::enumeration(
                "demo::Direction".into(),
                SourcePath::single("Direction"),
            )),
        );
        let mut contract = package();
        contract.records.push(point);
        contract.enums.push(direction);
        contract.classes.push(class(
            "demo::Engine",
            "Engine",
            vec![locate, direction_method],
        ));

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert_eq!(
            class
                .methods()
                .first()
                .map(|method| method.callable().returns().plan()),
            Some(&ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Record(RecordId::from_raw(0)),
            })
        );
        assert_eq!(
            class
                .methods()
                .get(1)
                .map(|method| method.callable().returns().plan()),
            Some(&ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(EnumId::from_raw(0)),
            })
        );
    }

    #[test]
    fn class_param_with_required_presence_lowers_to_required_handle() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut method = method("install", Receiver::Shared, ReturnDef::Void);
        method
            .parameters
            .push(param("driver", class_type("demo::Driver", "Driver")));
        let engine = class("demo::Engine", "Engine", vec![method]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let class_method = class_by_id(&bindings, ClassId::from_raw(1))
            .methods()
            .first()
            .expect("method")
            .clone();

        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .and_then(|param| param.as_value()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
                presence: HandlePresence::Required,
            })
        );
    }

    #[test]
    fn class_param_with_nullable_presence_lowers_to_nullable_handle() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut method = method("attach", Receiver::Shared, ReturnDef::Void);
        method.parameters.push(param(
            "driver",
            nullable_class_type("demo::Driver", "Driver"),
        ));
        let engine = class("demo::Engine", "Engine", vec![method]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let class_method = class_by_id(&bindings, ClassId::from_raw(1))
            .methods()
            .first()
            .expect("method")
            .clone();

        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .and_then(|param| param.as_value()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            })
        );
    }

    #[test]
    fn class_return_with_nullable_presence_lifts_to_nullable_handle() {
        let mut method = method(
            "maybe_driver",
            Receiver::Shared,
            ReturnDef::value(nullable_class_type("demo::Driver", "Driver")),
        );
        let driver = class("demo::Driver", "Driver", Vec::new());
        let _ = &mut method;
        let engine = class("demo::Engine", "Engine", vec![method]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let class_method = class_by_id(&bindings, ClassId::from_raw(1))
            .methods()
            .first()
            .expect("method")
            .clone();

        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Nullable,
            }
        );
    }

    #[test]
    fn static_nullable_class_return_stays_method_not_initializer() {
        let method = method(
            "maybe_new",
            Receiver::None,
            ReturnDef::value(nullable_class_type("demo::Engine", "Engine")),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![method]));
        let class = class_by_id(&bindings, ClassId::from_raw(0));

        assert!(class.initializers().is_empty());
        let class_method = class.methods().first().expect("expected method");
        assert_eq!(class_method.name(), &CanonicalName::single("maybe_new"));
        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Nullable,
            }
        );
    }

    #[test]
    fn option_self_return_lowers_to_nullable_class_handle() {
        let method = method(
            "maybe_self",
            Receiver::Shared,
            ReturnDef::value(TypeExpr::option(TypeExpr::SelfType)),
        );
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![method]));
        let class_method = class_by_id(&bindings, ClassId::from_raw(0))
            .methods()
            .first()
            .expect("expected method");

        assert_eq!(
            class_method.callable().returns().plan(),
            &ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: native::HandleCarrier::U64,
                presence: HandlePresence::Nullable,
            }
        );
    }

    #[test]
    fn nullable_class_param_uses_same_carrier_as_required() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut required_method = method("install", Receiver::Shared, ReturnDef::Void);
        required_method
            .parameters
            .push(param("driver", class_type("demo::Driver", "Driver")));
        let mut nullable_method = method("attach", Receiver::Shared, ReturnDef::Void);
        nullable_method.parameters.push(param(
            "driver",
            nullable_class_type("demo::Driver", "Driver"),
        ));
        let engine = class(
            "demo::Engine",
            "Engine",
            vec![required_method, nullable_method],
        );
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Native>(contract).expect("contract should lower");
        let methods = class_by_id(&bindings, ClassId::from_raw(1)).methods();

        let required_carrier = match methods[0].callable().params()[0].as_value().unwrap() {
            ParamPlan::Handle { carrier, .. } => *carrier,
            other => panic!("expected handle, got {other:?}"),
        };
        let nullable_carrier = match methods[1].callable().params()[0].as_value().unwrap() {
            ParamPlan::Handle { carrier, .. } => *carrier,
            other => panic!("expected handle, got {other:?}"),
        };
        assert_eq!(required_carrier, nullable_carrier);
    }

    #[test]
    fn nullable_class_param_uses_u32_carrier_on_wasm32() {
        let driver = class("demo::Driver", "Driver", Vec::new());
        let mut method = method("attach", Receiver::Shared, ReturnDef::Void);
        method.parameters.push(param(
            "driver",
            nullable_class_type("demo::Driver", "Driver"),
        ));
        let engine = class("demo::Engine", "Engine", vec![method]);
        let mut contract = package();
        contract.classes = vec![driver, engine];

        let bindings = lower_contract::<Wasm32>(contract).expect("contract should lower");
        let class_method = class_by_id(&bindings, ClassId::from_raw(1))
            .methods()
            .first()
            .expect("method")
            .clone();

        assert_eq!(
            class_method
                .callable()
                .params()
                .first()
                .and_then(|param| param.as_value()),
            Some(&ParamPlan::Handle {
                target: HandleTarget::Class(ClassId::from_raw(0)),
                carrier: wasm32::HandleCarrier::U32,
                receive: Receive::ByValue,
                presence: HandlePresence::Nullable,
            })
        );
    }

    #[test]
    fn async_class_method_lowers_to_poll_handle_protocol_on_native() {
        let mut compute = method("compute", Receiver::Shared, ReturnDef::Void);
        compute.execution = boltffi_ast::ExecutionKind::Async;
        let bindings = lower_class::<Native>(class("demo::Engine", "Engine", vec![compute]));
        let methods = class_by_id(&bindings, ClassId::from_raw(0)).methods();

        assert_eq!(
            methods[0].target().name().as_str(),
            "boltffi_method_class_demo_engine_compute"
        );
        match methods[0].callable().execution() {
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                handle,
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => {
                assert_eq!(handle, &native::HandleCarrier::U64);
                assert_eq!(
                    poll.name().as_str(),
                    "boltffi_async_method_class_demo_engine_compute_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_method_class_demo_engine_compute_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_method_class_demo_engine_compute_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_method_class_demo_engine_compute_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_method_class_demo_engine_compute_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn single_threaded_borrowed_async_is_thread_bound_but_detached_future_is_not() {
        let mut borrowed = method("borrowed", Receiver::Shared, ReturnDef::Void);
        borrowed.execution = ExecutionKind::Async;
        let mut detached = method("detached", Receiver::Shared, ReturnDef::Void);
        detached.execution = ExecutionKind::DetachedFuture;
        let mut source = class("demo::Engine", "Engine", vec![borrowed, detached]);
        source.thread_safety = SourceClassThreadSafety::UnsafeSingleThreaded;

        let bindings = lower_class::<Native>(source);
        let methods = class_by_id(&bindings, ClassId::from_raw(0)).methods();
        let mobilities = methods
            .iter()
            .map(|method| match method.callable().execution() {
                ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                    mobility, ..
                }) => *mobility,
                execution => panic!("expected poll-handle execution, got {execution:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            mobilities,
            vec![FutureMobility::ThreadBound, FutureMobility::CrossThread]
        );
    }

    #[test]
    fn async_class_initializer_lowers_to_poll_handle_protocol_on_wasm32() {
        let mut new_engine = method(
            "new",
            Receiver::None,
            ReturnDef::value(class_type("demo::Engine", "Engine")),
        );
        new_engine.execution = boltffi_ast::ExecutionKind::Async;
        let bindings = lower_class::<Wasm32>(class("demo::Engine", "Engine", vec![new_engine]));
        let initializers = class_by_id(&bindings, ClassId::from_raw(0)).initializers();

        assert_eq!(
            initializers[0].symbol().name().as_str(),
            "boltffi_init_class_demo_engine_new"
        );
        match initializers[0].callable().execution() {
            ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::PollHandle {
                handle,
                poll_sync,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => {
                assert_eq!(handle, &wasm32::HandleCarrier::U32);
                assert_eq!(
                    poll_sync.name().as_str(),
                    "boltffi_async_init_class_demo_engine_new_poll_sync"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_init_class_demo_engine_new_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_init_class_demo_engine_new_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_init_class_demo_engine_new_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_init_class_demo_engine_new_panic_message"
                );
            }
            other => panic!("expected wasm32 PollHandle protocol, got {other:?}"),
        }
    }
}
