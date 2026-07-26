//! Free-function lowering.
//!
//! Walks every [`FunctionDef`] the source contract exposes and produces
//! a [`FunctionDecl<S>`] carrying the native symbol foreign code links
//! against and the [`CallableDecl<S>`] that describes the crossing.
//! Free functions have no owning declaration and no `Self`, so the
//! callable owner used while resolving parameter and return types is
//! [`callable::CallableOwner::Function`], which rejects any `Self`
//! reference it encounters.

use boltffi_ast::{CallableForm, FunctionDef as SourceFunction};

use crate::{CanonicalName, FunctionDecl};

use super::{
    LowerError, callable, error::LowerErrorKind, ids::DeclarationIds, index::Index, metadata,
    surface::SurfaceLower, symbol::SymbolAllocator,
};

pub fn lower<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
) -> Result<Vec<FunctionDecl<S>>, LowerError> {
    index
        .functions()
        .iter()
        .map(|function| lower_one::<S>(index, ids, allocator, function))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    function: &SourceFunction,
) -> Result<FunctionDecl<S>, LowerError> {
    if !matches!(function.form, CallableForm::Function) {
        return Err(LowerError::new(LowerErrorKind::InvalidFunctionForm));
    }

    let function_id = ids.function(&function.id)?;
    let symbol = allocator.mint_function(function.id.as_str())?;
    let callable_decl =
        callable::lower_function::<S>(index, ids, allocator, function, symbol.name().as_str())?;
    Ok(FunctionDecl::new(
        function_id,
        CanonicalName::from(&function.name),
        metadata::decl_meta(function.doc.as_ref(), function.deprecated.as_ref()),
        symbol,
        callable_decl,
    ))
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CallableForm, CanonicalName as SourceName, DeprecationInfo as SourceDeprecationInfo,
        DocComment as SourceDocComment, ExecutionKind, FieldDef, FunctionDef,
        FunctionId as SourceFunctionId, MethodDef, MethodId as SourceMethodId,
        PackageInfo as SourcePackage, ParameterDef, Path as SourcePath, Primitive, Receiver,
        RecordDef, ReprAttr, ReprItem, ReturnDef, SourceContract, TypeExpr,
    };

    use crate::lower::{LowerError, LowerErrorKind, UnsupportedType, lower};
    use crate::{
        Bindings, CodecNode, Decl, DirectValueType, DirectVectorElementType, ErrorDecl,
        ExecutionDecl, FunctionDecl, IntoRust, Native, OutOfRust, ParamPlan,
        Primitive as BindingPrimitive, Receive, RecordDecl, RecordId, ReturnPlan, SurfaceLower,
        TypeRef, ValueRef, Wasm32, native, wasm32,
    };

    struct TestContract {
        source: SourceContract,
    }

    impl TestContract {
        fn new() -> Self {
            Self {
                source: SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned()))),
            }
        }

        fn with_function(mut self, function: FunctionDef) -> Self {
            self.source.functions.push(function);
            self
        }

        fn with_record(mut self, record: RecordDef) -> Self {
            self.source.records.push(record);
            self
        }

        fn lower<S: SurfaceLower>(self) -> Result<Bindings<S>, LowerError> {
            lower::<S>(&self.source)
        }

        fn lower_ok<S: SurfaceLower>(self) -> Bindings<S> {
            self.lower::<S>().expect("contract should lower")
        }
    }

    fn function(id: &str, function_name: &str) -> FunctionDef {
        FunctionDef::new(SourceFunctionId::new(id), name(function_name))
    }

    fn method(method_name: &str, receiver: Receiver) -> MethodDef {
        MethodDef::new(
            SourceMethodId::new(method_name),
            name(method_name),
            receiver,
        )
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn value_param(param_name: &str, type_expr: TypeExpr) -> ParameterDef {
        ParameterDef::value(name(param_name), type_expr)
    }

    fn record_type(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(id.into(), SourcePath::single(path))
    }

    fn point_record() -> RecordDef {
        let mut point = RecordDef::new("demo::Point".into(), name("Point"));
        point.repr = ReprAttr::new(vec![ReprItem::C]);
        point.fields = vec![FieldDef::new(
            name("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        point
    }

    fn function_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&FunctionDecl<S>> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Function(function) => Some(function.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn first_function<S: SurfaceLower>(bindings: &Bindings<S>) -> &FunctionDecl<S> {
        function_decls(bindings)
            .into_iter()
            .next()
            .expect("expected function declaration")
    }

    fn record_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&RecordDecl<S>> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Record(record) => Some(record.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn symbol_names<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&str> {
        bindings
            .symbols()
            .symbols()
            .iter()
            .map(|symbol| symbol.name().as_str())
            .collect()
    }

    fn returning(id: &str, function_name: &str, type_expr: TypeExpr) -> FunctionDef {
        let mut decl = function(id, function_name);
        decl.returns = ReturnDef::value(type_expr);
        decl
    }

    fn taking(id: &str, function_name: &str, param_name: &str, type_expr: TypeExpr) -> FunctionDef {
        let mut decl = function(id, function_name);
        decl.parameters = vec![value_param(param_name, type_expr)];
        decl
    }

    fn first_param_lower<S: SurfaceLower>(bindings: &Bindings<S>) -> &ParamPlan<S, IntoRust> {
        first_function(bindings).callable().params()[0]
            .as_value()
            .unwrap()
    }

    fn assert_native_string_error(error: &ErrorDecl<Native, OutOfRust>) {
        match error {
            ErrorDecl::EncodedViaReturnSlot { ty, codec, shape } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(codec.root(), &CodecNode::String);
                assert_eq!(shape, &native::BufferShape::Buffer);
            }
            other => panic!("expected encoded string error channel, got {other:?}"),
        }
    }

    #[test]
    fn void_function_lowers_to_void_lift_with_no_error_channel() {
        let bindings = TestContract::new()
            .with_function(function("demo::ping", "ping"))
            .lower_ok::<Native>();
        let function = first_function(&bindings);

        assert_eq!(function.name(), &crate::CanonicalName::single("ping"));
        assert!(matches!(
            function.callable().returns().plan(),
            ReturnPlan::Void
        ));
        assert!(matches!(function.callable().error(), ErrorDecl::None(_)));
        assert_eq!(function.callable().receiver(), None);
    }

    #[test]
    fn primitive_param_lowers_to_direct() {
        let mut add = function("demo::add", "add");
        add.parameters = vec![
            value_param("lhs", TypeExpr::Primitive(Primitive::I32)),
            value_param("rhs", TypeExpr::Primitive(Primitive::I32)),
        ];
        add.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::I32));

        let bindings = TestContract::new().with_function(add).lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert_eq!(callable.params().len(), 2);
        assert_eq!(
            callable.params()[0].as_value().unwrap(),
            &ParamPlan::Direct {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
                receive: Receive::ByValue,
            }
        );
        assert_eq!(
            callable.returns().plan(),
            &ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
            }
        );
    }

    #[test]
    fn string_param_lowers_to_encoded_with_native_slice_shape() {
        let mut greet = function("demo::greet", "greet");
        greet.parameters = vec![value_param("name", TypeExpr::String)];
        greet.returns = ReturnDef::value(TypeExpr::String);

        let bindings = TestContract::new()
            .with_function(greet)
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        match callable.params()[0].as_value().unwrap() {
            ParamPlan::Encoded {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Slice,
                receive: Receive::ByValue,
            } => {
                assert_eq!(
                    codec.value(),
                    &ValueRef::named(crate::CanonicalName::single("name"))
                );
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded String param with slice shape, got {other:?}"),
        }
        match callable.returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: native::BufferShape::Buffer,
            } => {
                assert_eq!(codec.root(), &CodecNode::String);
            }
            other => panic!("expected encoded String return with buffer shape, got {other:?}"),
        }
    }

    #[test]
    fn nested_interned_string_param_lowers_to_tagged_codec_node() {
        let type_expr = TypeExpr::option(TypeExpr::vec(TypeExpr::interned_string(
            SourcePath::single("InternedString"),
            "demo::pools::BrowserName",
            SourcePath::single("BrowserName"),
            vec!["Chrome".to_owned(), "Firefox".to_owned()],
        )));
        let bindings = TestContract::new()
            .with_function(taking("demo::detect", "detect", "names", type_expr))
            .lower_ok::<Native>();

        let ParamPlan::Encoded { codec, .. } = first_param_lower(&bindings) else {
            panic!("nested interned string must use an encoded parameter");
        };
        let CodecNode::Optional(inner) = codec.root() else {
            panic!("expected optional codec root, got {:?}", codec.root());
        };
        let CodecNode::Sequence { element, .. } = inner.as_ref() else {
            panic!("expected nested sequence codec, got {inner:?}");
        };
        assert_eq!(
            element.as_ref(),
            &CodecNode::InternedString {
                static_values: vec!["Chrome".to_owned(), "Firefox".to_owned()]
            }
        );
        assert!(codec.uses_interned_string());
    }

    #[test]
    fn wasm32_string_return_uses_packed_shape() {
        let mut greet = function("demo::greet", "greet");
        greet.returns = ReturnDef::value(TypeExpr::String);

        let bindings = TestContract::new()
            .with_function(greet)
            .lower_ok::<Wasm32>();
        let callable = first_function(&bindings).callable();

        match callable.returns().plan() {
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => {
                assert_eq!(codec.root(), &CodecNode::Utf8String);
            }
            other => panic!("expected wasm32 packed string return, got {other:?}"),
        }
    }

    #[test]
    fn result_return_splits_into_lift_and_encoded_error() {
        let mut try_open = function("demo::try_open", "try_open");
        try_open.returns = ReturnDef::value(TypeExpr::result(
            TypeExpr::Primitive(Primitive::I32),
            TypeExpr::String,
        ));

        let bindings = TestContract::new()
            .with_function(try_open)
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert_eq!(
            callable.returns().plan(),
            &ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Primitive(BindingPrimitive::I32),
            }
        );
        assert_native_string_error(callable.error());
    }

    #[test]
    fn result_unit_ok_emits_void_lift_with_encoded_error() {
        let mut try_init = function("demo::try_init", "try_init");
        try_init.returns = ReturnDef::value(TypeExpr::result(TypeExpr::Unit, TypeExpr::String));

        let bindings = TestContract::new()
            .with_function(try_init)
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
        assert_native_string_error(callable.error());
    }

    #[test]
    fn function_symbol_uses_function_lane_and_snake_path() {
        let bindings = TestContract::new()
            .with_function(function("demo::nested::DoTheThing", "DoTheThing"))
            .lower_ok::<Native>();
        let function = first_function(&bindings);

        assert_eq!(
            function.symbol().name().as_str(),
            "boltffi_function_demo_nested_do_the_thing"
        );
    }

    #[test]
    fn function_symbol_replaces_hyphenated_package_segments() {
        let bindings = TestContract::new()
            .with_function(function("my-crate::add", "add"))
            .lower_ok::<Native>();
        let function = first_function(&bindings);

        assert_eq!(
            function.symbol().name().as_str(),
            "boltffi_function_my_crate_add"
        );
    }

    #[test]
    fn async_free_function_lowers_to_poll_handle_protocol_on_native() {
        let mut spin = function("demo::spin", "spin");
        spin.execution = ExecutionKind::Async;

        let bindings = TestContract::new().with_function(spin).lower_ok::<Native>();
        let function = first_function(&bindings);

        assert_eq!(
            function.symbol().name().as_str(),
            "boltffi_function_demo_spin"
        );
        match function.callable().execution() {
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
                    "boltffi_async_function_demo_spin_poll"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_function_demo_spin_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_function_demo_spin_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_function_demo_spin_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_function_demo_spin_panic_message"
                );
            }
            other => panic!("expected native PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_free_function_lowers_to_poll_handle_protocol_on_wasm32() {
        let mut spin = function("demo::spin", "spin");
        spin.execution = ExecutionKind::Async;

        let bindings = TestContract::new().with_function(spin).lower_ok::<Wasm32>();
        let function = first_function(&bindings);

        match function.callable().execution() {
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
                    "boltffi_async_function_demo_spin_poll_sync"
                );
                assert_eq!(
                    complete.name().as_str(),
                    "boltffi_async_function_demo_spin_complete"
                );
                assert_eq!(
                    cancel.name().as_str(),
                    "boltffi_async_function_demo_spin_cancel"
                );
                assert_eq!(
                    free.name().as_str(),
                    "boltffi_async_function_demo_spin_free"
                );
                assert_eq!(
                    panic_message.name().as_str(),
                    "boltffi_async_function_demo_spin_panic_message"
                );
            }
            other => panic!("expected wasm32 PollHandle protocol, got {other:?}"),
        }
    }

    #[test]
    fn async_free_function_registers_lifecycle_symbols_in_symbol_table() {
        let mut spin = function("demo::spin", "spin");
        spin.execution = ExecutionKind::Async;

        let bindings = TestContract::new().with_function(spin).lower_ok::<Native>();
        let names = symbol_names(&bindings);

        assert!(names.contains(&"boltffi_function_demo_spin"));
        assert!(names.contains(&"boltffi_async_function_demo_spin_poll"));
        assert!(names.contains(&"boltffi_async_function_demo_spin_complete"));
        assert!(names.contains(&"boltffi_async_function_demo_spin_cancel"));
        assert!(names.contains(&"boltffi_async_function_demo_spin_free"));
        assert!(names.contains(&"boltffi_async_function_demo_spin_panic_message"));
    }

    #[test]
    fn async_result_unit_success_keeps_void_success_and_encoded_error() {
        let mut run = function("demo::run", "run");
        run.execution = ExecutionKind::Async;
        run.returns = ReturnDef::value(TypeExpr::result(TypeExpr::Unit, TypeExpr::String));

        let bindings = TestContract::new().with_function(run).lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert!(matches!(
            callable.execution(),
            ExecutionDecl::Asynchronous(_)
        ));
        assert!(matches!(callable.returns().plan(), ReturnPlan::Void));
        assert_native_string_error(callable.error());
    }

    #[test]
    fn async_option_scalar_return_keeps_scalar_option_plan() {
        let mut maybe_count = function("demo::maybe_count", "maybe_count");
        maybe_count.execution = ExecutionKind::Async;
        maybe_count.returns = ReturnDef::value(TypeExpr::Option(Box::new(TypeExpr::Primitive(
            Primitive::I32,
        ))));

        let bindings = TestContract::new()
            .with_function(maybe_count)
            .lower_ok::<Wasm32>();
        let callable = first_function(&bindings).callable();

        assert_eq!(
            callable.returns().plan(),
            &ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: BindingPrimitive::I32,
            }
        );
        assert!(matches!(callable.error(), ErrorDecl::None(_)));
    }

    #[test]
    fn async_result_vec_success_falls_back_to_encoded_out_pointer() {
        let mut load_all = function("demo::load_all", "load_all");
        load_all.execution = ExecutionKind::Async;
        load_all.returns = ReturnDef::value(TypeExpr::result(
            TypeExpr::Vec(Box::new(TypeExpr::Primitive(Primitive::I32))),
            TypeExpr::String,
        ));

        let bindings = TestContract::new()
            .with_function(load_all)
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        match callable.returns().plan() {
            ReturnPlan::EncodedViaOutPointer { ty, codec, shape } => {
                assert_eq!(
                    ty,
                    &TypeRef::Sequence(Box::new(TypeRef::Primitive(BindingPrimitive::I32)))
                );
                match codec.root() {
                    CodecNode::Sequence { element, .. } => {
                        assert_eq!(
                            element.as_ref(),
                            &CodecNode::Primitive(BindingPrimitive::I32)
                        );
                    }
                    other => panic!("expected sequence codec, got {other:?}"),
                }
                assert_eq!(shape, &native::BufferShape::Buffer);
            }
            other => panic!("expected encoded async result vec success, got {other:?}"),
        }
        assert_native_string_error(callable.error());
    }

    #[test]
    fn self_in_free_function_param_is_rejected() {
        let mut weird = function("demo::weird", "weird");
        weird.parameters = vec![value_param("value", TypeExpr::SelfType)];

        let error = TestContract::new()
            .with_function(weird)
            .lower::<Native>()
            .expect_err("Self has no meaning in a free function");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::SelfType)
        ));
    }

    #[test]
    fn non_function_callable_form_inside_function_list_is_rejected() {
        let mut malformed = function("demo::malformed", "malformed");
        malformed.form = CallableForm::AssociatedFunction;

        let error = TestContract::new()
            .with_function(malformed)
            .lower::<Native>()
            .expect_err("function list only accepts function-form callables");

        assert!(matches!(error.kind(), LowerErrorKind::InvalidFunctionForm));
    }

    #[test]
    fn multiple_free_functions_get_sequential_ids_in_source_order() {
        let bindings = TestContract::new()
            .with_function(function("demo::one", "one"))
            .with_function(function("demo::two", "two"))
            .with_function(function("demo::three", "three"))
            .lower_ok::<Native>();
        let function_ids: Vec<u32> = function_decls(&bindings)
            .into_iter()
            .map(|function| function.id().raw())
            .collect();

        assert_eq!(function_ids, vec![0, 1, 2]);
    }

    #[test]
    fn duplicate_function_source_id_is_rejected() {
        let error = TestContract::new()
            .with_function(function("demo::dup", "dup"))
            .with_function(function("demo::dup", "DupAgain"))
            .lower::<Native>()
            .expect_err("two functions cannot share one source id");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::DuplicateSourceId { .. }
        ));
    }

    #[test]
    fn function_doc_and_deprecation_propagate_to_decl_meta() {
        let mut greet = function("demo::greet", "greet");
        greet.doc = Some(SourceDocComment::new("greet by name"));
        greet.deprecated = Some(SourceDeprecationInfo {
            note: Some("use greet_v2 instead".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = TestContract::new()
            .with_function(greet)
            .lower_ok::<Native>();
        let meta = first_function(&bindings).meta();

        assert_eq!(meta.doc().map(|d| d.as_str()), Some("greet by name"));
        assert_eq!(
            meta.deprecated().and_then(|d| d.message()),
            Some("use greet_v2 instead")
        );
    }

    #[test]
    fn function_taking_record_param_lowers_through_record_decl() {
        let mut translate = function("demo::translate", "translate");
        translate.parameters = vec![value_param("point", record_type("demo::Point", "Point"))];
        translate.returns = ReturnDef::value(TypeExpr::Primitive(Primitive::F64));

        let bindings = TestContract::new()
            .with_record(point_record())
            .with_function(translate)
            .lower_ok::<Native>();
        let records = record_decls(&bindings);
        let function = first_function(&bindings);

        assert_eq!(records.len(), 1);
        assert!(matches!(records[0], RecordDecl::Direct(_)));
        assert_eq!(function.callable().params().len(), 1);
        assert_eq!(
            function.callable().params()[0].as_value().unwrap(),
            &ParamPlan::Direct {
                ty: DirectValueType::Record(RecordId::from_raw(0)),
                receive: Receive::ByValue,
            }
        );
    }

    #[test]
    fn function_symbol_lane_does_not_collide_with_record_method_lane() {
        let mut point = point_record();
        point.methods.push(method("free", Receiver::Shared));

        let bindings = TestContract::new()
            .with_record(point)
            .with_function(function("demo::Point::free", "free"))
            .lower_ok::<Native>();

        assert_eq!(
            symbol_names(&bindings),
            vec![
                "boltffi_method_record_demo_point_free",
                "boltffi_function_demo_point_free",
            ]
        );
    }

    #[test]
    fn free_function_callable_is_synchronous() {
        let bindings = TestContract::new()
            .with_function(function("demo::ping", "ping"))
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert_eq!(
            callable.execution(),
            &ExecutionDecl::Synchronous(Default::default())
        );
    }

    #[test]
    fn option_primitive_return_lowers_to_scalar_option() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::maybe_count",
                "maybe_count",
                TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
            ))
            .lower_ok::<Native>();

        assert_eq!(
            first_function(&bindings).callable().returns().plan(),
            &ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: BindingPrimitive::I32,
            }
        );
    }

    #[test]
    fn option_primitive_return_lowers_to_scalar_option_on_wasm32() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::maybe_count",
                "maybe_count",
                TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
            ))
            .lower_ok::<Wasm32>();

        assert_eq!(
            first_function(&bindings).callable().returns().plan(),
            &ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: BindingPrimitive::I32,
            }
        );
    }

    #[test]
    fn wasm32_lossy_scalar_options_use_encoded_returns() {
        [Primitive::I64, Primitive::U64, Primitive::F32]
            .into_iter()
            .for_each(|primitive| {
                let bindings = TestContract::new()
                    .with_function(returning(
                        "demo::maybe_value",
                        "maybe_value",
                        TypeExpr::option(TypeExpr::Primitive(primitive)),
                    ))
                    .lower_ok::<Wasm32>();

                assert!(matches!(
                    first_function(&bindings).callable().returns().plan(),
                    ReturnPlan::EncodedViaReturnSlot { .. }
                ));
            });
    }

    #[test]
    fn wasm32_f64_option_return_uses_scalar_carrier() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::maybe_value",
                "maybe_value",
                TypeExpr::option(TypeExpr::Primitive(Primitive::F64)),
            ))
            .lower_ok::<Wasm32>();

        assert_eq!(
            first_function(&bindings).callable().returns().plan(),
            &ReturnPlan::ScalarOptionViaReturnSlot {
                primitive: BindingPrimitive::F64,
            }
        );
    }

    #[test]
    fn option_string_return_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::maybe_name",
                "maybe_name",
                TypeExpr::option(TypeExpr::String),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_function(&bindings).callable().returns().plan(),
            ReturnPlan::EncodedViaReturnSlot { .. }
        ));
    }

    #[test]
    fn option_vec_return_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::maybe_bytes",
                "maybe_bytes",
                TypeExpr::option(TypeExpr::vec(TypeExpr::Primitive(Primitive::U8))),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_function(&bindings).callable().returns().plan(),
            ReturnPlan::EncodedViaReturnSlot { .. }
        ));
    }

    #[test]
    fn vec_primitive_return_lowers_to_direct_vec() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::counts",
                "counts",
                TypeExpr::vec(TypeExpr::Primitive(Primitive::U32)),
            ))
            .lower_ok::<Native>();

        assert_eq!(
            first_function(&bindings).callable().returns().plan(),
            &ReturnPlan::DirectVecViaReturnSlot {
                element: DirectVectorElementType::primitive(BindingPrimitive::U32)
                    .expect("u32 is a direct-vector primitive"),
            }
        );
    }

    #[test]
    fn vec_primitive_return_lowers_to_direct_vec_on_wasm32() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::counts",
                "counts",
                TypeExpr::vec(TypeExpr::Primitive(Primitive::U32)),
            ))
            .lower_ok::<Wasm32>();

        assert_eq!(
            first_function(&bindings).callable().returns().plan(),
            &ReturnPlan::DirectVecViaReturnSlot {
                element: DirectVectorElementType::primitive(BindingPrimitive::U32)
                    .expect("u32 is a direct-vector primitive"),
            }
        );
    }

    #[test]
    fn vec_direct_record_return_lowers_to_direct_vec() {
        let bindings = TestContract::new()
            .with_record(point_record())
            .with_function(returning(
                "demo::points",
                "points",
                TypeExpr::vec(record_type("demo::Point", "Point")),
            ))
            .lower_ok::<Native>();

        match first_function(&bindings).callable().returns().plan() {
            ReturnPlan::DirectVecViaReturnSlot {
                element: DirectVectorElementType::Record(_),
            } => {}
            other => panic!("expected DirectVec of direct record, got {other:?}"),
        }
    }

    #[test]
    fn vec_string_return_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::lines",
                "lines",
                TypeExpr::vec(TypeExpr::String),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_function(&bindings).callable().returns().plan(),
            ReturnPlan::EncodedViaReturnSlot { .. }
        ));
    }

    #[test]
    fn nested_vec_return_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::matrix",
                "matrix",
                TypeExpr::vec(TypeExpr::vec(TypeExpr::Primitive(Primitive::F64))),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_function(&bindings).callable().returns().plan(),
            ReturnPlan::EncodedViaReturnSlot { .. }
        ));
    }

    #[test]
    fn result_option_primitive_ok_does_not_specialize() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::try_count",
                "try_count",
                TypeExpr::result(
                    TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
                    TypeExpr::String,
                ),
            ))
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert!(matches!(
            callable.returns().plan(),
            ReturnPlan::EncodedViaOutPointer { .. }
        ));
        assert_native_string_error(callable.error());
    }

    #[test]
    fn result_vec_primitive_ok_does_not_specialize() {
        let bindings = TestContract::new()
            .with_function(returning(
                "demo::try_samples",
                "try_samples",
                TypeExpr::result(
                    TypeExpr::vec(TypeExpr::Primitive(Primitive::F64)),
                    TypeExpr::String,
                ),
            ))
            .lower_ok::<Native>();
        let callable = first_function(&bindings).callable();

        assert!(matches!(
            callable.returns().plan(),
            ReturnPlan::EncodedViaOutPointer { .. }
        ));
        assert_native_string_error(callable.error());
    }

    #[test]
    fn option_primitive_param_lowers_to_scalar_option() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_count",
                "set_count",
                "count",
                TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
            ))
            .lower_ok::<Native>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::ScalarOption {
                primitive: BindingPrimitive::I32,
            }
        );
    }

    #[test]
    fn option_primitive_param_lowers_to_scalar_option_on_wasm32() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_count",
                "set_count",
                "count",
                TypeExpr::option(TypeExpr::Primitive(Primitive::I32)),
            ))
            .lower_ok::<Wasm32>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::ScalarOption {
                primitive: BindingPrimitive::I32,
            }
        );
    }

    #[test]
    fn wasm32_lossy_scalar_options_use_encoded_parameters() {
        [Primitive::I64, Primitive::U64, Primitive::F32]
            .into_iter()
            .for_each(|primitive| {
                let bindings = TestContract::new()
                    .with_function(taking(
                        "demo::set_value",
                        "set_value",
                        "value",
                        TypeExpr::option(TypeExpr::Primitive(primitive)),
                    ))
                    .lower_ok::<Wasm32>();

                assert!(matches!(
                    first_param_lower(&bindings),
                    ParamPlan::Encoded { .. }
                ));
            });
    }

    #[test]
    fn wasm32_f64_option_parameter_uses_scalar_carrier() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_value",
                "set_value",
                "value",
                TypeExpr::option(TypeExpr::Primitive(Primitive::F64)),
            ))
            .lower_ok::<Wasm32>();

        assert!(matches!(
            first_param_lower(&bindings),
            ParamPlan::ScalarOption {
                primitive: BindingPrimitive::F64,
                ..
            }
        ));
    }

    #[test]
    fn option_string_param_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_name",
                "set_name",
                "name",
                TypeExpr::option(TypeExpr::String),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_param_lower(&bindings),
            ParamPlan::Encoded { .. }
        ));
    }

    #[test]
    fn vec_primitive_param_lowers_to_direct_vec() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_counts",
                "set_counts",
                "counts",
                TypeExpr::vec(TypeExpr::Primitive(Primitive::U32)),
            ))
            .lower_ok::<Native>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::DirectVec {
                element: DirectVectorElementType::primitive(BindingPrimitive::U32)
                    .expect("u32 is a direct-vector primitive"),
                receive: Receive::ByValue,
            }
        );
    }

    #[test]
    fn vec_primitive_param_lowers_to_direct_vec_on_wasm32() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_counts",
                "set_counts",
                "counts",
                TypeExpr::vec(TypeExpr::Primitive(Primitive::U32)),
            ))
            .lower_ok::<Wasm32>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::DirectVec {
                element: DirectVectorElementType::primitive(BindingPrimitive::U32)
                    .expect("u32 is a direct-vector primitive"),
                receive: Receive::ByValue,
            }
        );
    }

    #[test]
    fn vec_direct_record_param_lowers_to_direct_vec() {
        let bindings = TestContract::new()
            .with_record(point_record())
            .with_function(taking(
                "demo::set_points",
                "set_points",
                "points",
                TypeExpr::vec(record_type("demo::Point", "Point")),
            ))
            .lower_ok::<Native>();

        match first_param_lower(&bindings) {
            ParamPlan::DirectVec {
                element: DirectVectorElementType::Record(_),
                ..
            } => {}
            other => panic!("expected DirectVec of direct record, got {other:?}"),
        }
    }

    #[test]
    fn vec_string_param_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_lines",
                "set_lines",
                "lines",
                TypeExpr::vec(TypeExpr::String),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_param_lower(&bindings),
            ParamPlan::Encoded { .. }
        ));
    }

    #[test]
    fn nested_vec_param_stays_encoded() {
        let bindings = TestContract::new()
            .with_function(taking(
                "demo::set_matrix",
                "set_matrix",
                "rows",
                TypeExpr::vec(TypeExpr::vec(TypeExpr::Primitive(Primitive::F64))),
            ))
            .lower_ok::<Native>();

        assert!(matches!(
            first_param_lower(&bindings),
            ParamPlan::Encoded { .. }
        ));
    }

    #[test]
    fn ref_vec_primitive_param_stays_encoded() {
        let mut decl = function("demo::peek", "peek");
        decl.parameters = vec![ParameterDef {
            name: name("values").into(),
            type_expr: TypeExpr::vec(TypeExpr::Primitive(Primitive::U32)),
            passing: boltffi_ast::ParameterPassing::Ref,
            doc: None,
            default: None,
            user_attrs: Vec::new(),
            source: boltffi_ast::Source::exported(),
        }];
        let bindings = TestContract::new().with_function(decl).lower_ok::<Native>();

        assert!(matches!(
            first_param_lower(&bindings),
            ParamPlan::Encoded { .. }
        ));
    }

    #[test]
    fn ref_slice_primitive_param_lowers_to_borrowed_direct_vec() {
        let mut decl = function("demo::peek", "peek");
        decl.parameters = vec![ParameterDef {
            name: name("values").into(),
            type_expr: TypeExpr::Slice(Box::new(TypeExpr::Primitive(Primitive::U32))),
            passing: boltffi_ast::ParameterPassing::Ref,
            doc: None,
            default: None,
            user_attrs: Vec::new(),
            source: boltffi_ast::Source::exported(),
        }];
        let bindings = TestContract::new().with_function(decl).lower_ok::<Native>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::DirectVec {
                element: DirectVectorElementType::primitive(BindingPrimitive::U32)
                    .expect("u32 is a direct-vector primitive"),
                receive: Receive::ByRef,
            }
        );
    }

    #[test]
    fn mut_slice_primitive_param_lowers_to_mutable_direct_vec() {
        let mut decl = function("demo::increment", "increment");
        decl.parameters = vec![ParameterDef {
            name: name("values").into(),
            type_expr: TypeExpr::Slice(Box::new(TypeExpr::Primitive(Primitive::U64))),
            passing: boltffi_ast::ParameterPassing::RefMut,
            doc: None,
            default: None,
            user_attrs: Vec::new(),
            source: boltffi_ast::Source::exported(),
        }];
        let bindings = TestContract::new().with_function(decl).lower_ok::<Native>();

        assert_eq!(
            first_param_lower(&bindings),
            &ParamPlan::DirectVec {
                element: DirectVectorElementType::primitive(BindingPrimitive::U64)
                    .expect("u64 is a direct-vector primitive"),
                receive: Receive::ByMutRef,
            }
        );
    }
}
