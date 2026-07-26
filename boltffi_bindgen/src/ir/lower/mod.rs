use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::classification::{self, PassableCategory};
use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::transport::{
    CallbackParamStyle, DirectBufferParamStrategy, EnumTagStrategy, ErrorReturnStrategy,
    ParamContract, ParamPassingStrategy, ParamValueStrategy, ReturnContract, ScalarParamStrategy,
    ScalarReturnStrategy, ValueReturnStrategy, WireParamStrategy,
};

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, AsyncCall, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnShape, StreamItemTransport,
};
use crate::ir::codec::{
    BlittableField, CodecPlan, EncodedField, EnumLayout, RecordLayout, VariantLayout,
    VariantPayloadLayout, VecLayout,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, EnumDef, EnumRepr, FunctionDef,
    MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, StreamDef, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId,
};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape,
    WireSizeOwner, WriteOp, WriteSeq,
};
use crate::ir::plan::{
    AbiType, AsyncPlan, CallPlan, CallPlanKind, CallTarget, CallbackStyle, CompletionCallback,
    CompositeField, CompositeLayout, Mutability, ParamPlan, ReturnPlan, ScalarOrigin, SpanContent,
    Transport,
};
use crate::ir::types::{PrimitiveType, TypeExpr};

mod calls;
mod codec;

use self::calls::{AbiCallbackParamPlan, AbiCallbackParamStrategy};

trait MethodHost {
    fn classify(&self, lowerer: &Lowerer) -> Transport;
    fn type_expr(&self) -> TypeExpr;
    fn method_symbol(
        &self,
        method_id: &MethodId,
        lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol>;
    fn constructor_symbol(
        &self,
        name: Option<&MethodId>,
        lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol>;
    fn method_call_id(&self, method_id: &MethodId) -> CallId;
    fn constructor_call_id(&self, index: usize) -> CallId;
    fn constructors(&self) -> &[ConstructorDef];
    fn methods(&self) -> &[MethodDef];
    fn has_methods(&self) -> bool {
        !self.constructors().is_empty() || !self.methods().is_empty()
    }
}

impl MethodHost for RecordDef {
    fn classify(&self, lowerer: &Lowerer) -> Transport {
        lowerer.classify_record(&self.id)
    }

    fn type_expr(&self) -> TypeExpr {
        TypeExpr::Record(self.id.clone())
    }

    fn method_symbol(
        &self,
        method_id: &MethodId,
        _lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol> {
        naming::method_ffi_name(self.id.as_str(), method_id.as_str())
    }

    fn constructor_symbol(
        &self,
        name: Option<&MethodId>,
        _lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol> {
        match name {
            Some(n) => naming::method_ffi_name(self.id.as_str(), n.as_str()),
            None => naming::class_ffi_new(self.id.as_str()),
        }
    }

    fn method_call_id(&self, method_id: &MethodId) -> CallId {
        CallId::RecordMethod {
            record_id: self.id.clone(),
            method_id: method_id.clone(),
        }
    }

    fn constructor_call_id(&self, index: usize) -> CallId {
        CallId::RecordConstructor {
            record_id: self.id.clone(),
            index,
        }
    }

    fn constructors(&self) -> &[ConstructorDef] {
        &self.constructors
    }

    fn methods(&self) -> &[MethodDef] {
        &self.methods
    }
}

impl MethodHost for EnumDef {
    fn classify(&self, lowerer: &Lowerer) -> Transport {
        lowerer.classify_enum(&self.id)
    }

    fn type_expr(&self) -> TypeExpr {
        TypeExpr::Enum(self.id.clone())
    }

    fn method_symbol(
        &self,
        method_id: &MethodId,
        _lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol> {
        naming::method_ffi_name(self.id.as_str(), method_id.as_str())
    }

    fn constructor_symbol(
        &self,
        name: Option<&MethodId>,
        _lowerer: &Lowerer,
    ) -> naming::Name<naming::GlobalSymbol> {
        match name {
            Some(n) => naming::method_ffi_name(self.id.as_str(), n.as_str()),
            None => naming::class_ffi_new(self.id.as_str()),
        }
    }

    fn method_call_id(&self, method_id: &MethodId) -> CallId {
        CallId::EnumMethod {
            enum_id: self.id.clone(),
            method_id: method_id.clone(),
        }
    }

    fn constructor_call_id(&self, index: usize) -> CallId {
        CallId::EnumConstructor {
            enum_id: self.id.clone(),
            index,
        }
    }

    fn constructors(&self) -> &[ConstructorDef] {
        &self.constructors
    }

    fn methods(&self) -> &[MethodDef] {
        &self.methods
    }
}

/// Walks an [`FfiContract`] and produces an [`AbiContract`].
///
/// Most of the work is codec planning, figuring out which records are blittable,
/// which enums are C-style vs data-carrying, and detecting recursive types.
/// `record_stack` and `enum_stack` track what we are currently lowering so we
/// catch cycles: if lowering `TreeNode` hits `TreeNode` again in its own fields,
/// that is a recursive type, and it gets encoded layout because a fixed size
/// does not exist.
pub struct Lowerer<'c> {
    contract: &'c FfiContract,
    // tracks which records and enums we are currently lowering so we detect cycles.
    // if we hit the same id again mid-walk, the type is recursive and gets
    // encoded layout instead of blittable.
    record_stack: RefCell<HashSet<RecordId>>,
    enum_stack: RefCell<HashSet<EnumId>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'c> Lowerer<'c> {
    pub fn new(contract: &'c FfiContract) -> Self {
        Self {
            contract,
            record_stack: RefCell::new(HashSet::new()),
            enum_stack: RefCell::new(HashSet::new()),
        }
    }
}

mod abi;
mod ops;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::definitions::{
        CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef,
        EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, ParamPassing, Receiver,
        RecordDef, ReturnDef,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId,
        VariantName,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::callable::ExecutionKind;
    use boltffi_ffi_rules::naming;

    fn test_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "test".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![],
        }
    }

    fn lowerer_for_contract(contract: &FfiContract) -> Lowerer<'_> {
        Lowerer::new(contract)
    }

    #[test]
    fn param_strategy_primitive_is_direct() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Primitive(PrimitiveType::I32),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Scalar(ScalarOrigin::Primitive(PrimitiveType::I32))
        ));
    }

    #[test]
    fn param_strategy_string_is_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(&TypeExpr::String, &ParamPassing::Ref);

        assert!(matches!(strategy, Transport::Span(SpanContent::Utf8)));
    }

    #[test]
    fn param_strategy_vec_primitive_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::F32))),
            &ParamPassing::Ref,
        );

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::F32
            )))
        ));
    }

    #[test]
    fn param_strategy_handle_non_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy =
            lowerer.classify_param(&TypeExpr::Handle(class_id.clone()), &ParamPassing::Value);

        assert!(matches!(
            strategy,
            Transport::Handle { class_id: ref id, nullable: false } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("MyClass");
        let strategy = lowerer.classify_param(
            &TypeExpr::Option(Box::new(TypeExpr::Handle(class_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Handle { class_id: ref id, nullable: true } if id.as_str() == "MyClass"
        ));
    }

    #[test]
    fn param_strategy_callback_impl_trait() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.classify_param(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::ImplTrait,
        );

        assert!(matches!(
            strategy,
            Transport::Callback {
                callback_id: ref id,
                style: CallbackStyle::ImplTrait,
                nullable: false
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn param_strategy_option_callback_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("OnComplete");
        let strategy = lowerer.classify_param(
            &TypeExpr::Option(Box::new(TypeExpr::Callback(callback_id.clone()))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Callback {
                callback_id: ref id,
                style: CallbackStyle::BoxedDyn,
                nullable: true
            } if id.as_str() == "OnComplete"
        ));
    }

    #[test]
    fn lower_return_void() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan = lowerer.lower_return(&ReturnDef::Void);

        assert!(matches!(plan, ReturnPlan::Void));
    }

    #[test]
    fn lower_return_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let plan =
            lowerer.lower_return(&ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::Bool
            )))
        ));
    }

    #[test]
    fn lower_return_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Handle {
                nullable: false,
                ..
            })
        ));
    }

    #[test]
    fn lower_return_option_handle_is_nullable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Option(Box::new(
            TypeExpr::Handle(class_id),
        ))));

        assert!(matches!(
            plan,
            ReturnPlan::Value(Transport::Handle { nullable: true, .. })
        ));
    }

    #[test]
    fn lower_return_result_handle_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Connection");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: Transport::Handle {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn lower_return_result_callback_no_panic() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let plan = lowerer.lower_return(&ReturnDef::Result {
            ok: TypeExpr::Callback(callback_id),
            err: TypeExpr::String,
        });

        assert!(matches!(
            plan,
            ReturnPlan::Fallible {
                ok: Transport::Callback {
                    nullable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_primitive() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Primitive(PrimitiveType::U64));

        assert!(matches!(codec, CodecPlan::Primitive(PrimitiveType::U64)));
    }

    #[test]
    fn codec_from_transport_keeps_c_style_enum_identity() {
        let mut contract = test_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.codec_from_transport(&Transport::Scalar(ScalarOrigin::CStyleEnum {
            tag_type: PrimitiveType::I32,
            enum_id: EnumId::new("Direction"),
        }));

        assert!(matches!(
            codec,
            CodecPlan::Enum { id, .. } if id.as_str() == "Direction"
        ));
    }

    #[test]
    fn build_codec_string() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::String);

        assert!(matches!(codec, CodecPlan::String));
    }

    #[test]
    fn build_codec_option() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Option(Box::new(TypeExpr::String)));

        assert!(matches!(codec, CodecPlan::Option(_)));
    }

    #[test]
    fn build_codec_vec_primitive_is_blittable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Primitive(
            PrimitiveType::I32,
        ))));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Blittable { element_size: 4 },
                ..
            }
        ));
    }

    #[test]
    fn build_codec_vec_string_is_encoded() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::String)));

        assert!(matches!(
            codec,
            CodecPlan::Vec {
                layout: VecLayout::Encoded,
                ..
            }
        ));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_handle_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Handle(ClassId::new("Foo")));
    }

    #[test]
    #[should_panic(expected = "Handle and Callback types cannot be wire-encoded")]
    fn build_codec_callback_panics() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        lowerer.build_codec(&TypeExpr::Callback(CallbackId::new("Bar")));
    }

    #[test]
    fn lower_function_sync() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(
            &plan.target,
            CallTarget::GlobalSymbol(s) if s.as_str() == naming::function_ffi_name("greet").as_str()
        ));
        assert_eq!(plan.params.len(), 1);
        assert!(matches!(plan.kind, CallPlanKind::Sync { .. }));
    }

    #[test]
    fn lower_function_async() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("fetch"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_function(&func);

        assert!(matches!(plan.kind, CallPlanKind::Async { .. }));
    }

    #[test]
    fn lower_method_inserts_self_handle() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Client");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("connect"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 1);
        assert!(matches!(
            &plan.params[0].transport,
            Transport::Handle {
                nullable: false,
                ..
            }
        ));
        assert_eq!(plan.params[0].name.as_str(), "self");
    }

    #[test]
    fn lower_method_static_no_self() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Utils");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("helper"),
            receiver: Receiver::Static,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        assert_eq!(plan.params.len(), 0);
    }

    #[test]
    fn lower_constructor_non_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Builder");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(Transport::Handle {
                    nullable: false,
                    ..
                })
            }
        ));
    }

    #[test]
    fn lower_constructor_fallible() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Parser");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("try_new"),
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { .. }
            }
        ));
    }

    #[test]
    fn lower_callback_uses_vtable_field() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("EventHandler"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("on_event"),
                params: vec![],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans.len(), 1);
        assert!(matches!(
            &plans[0].target,
            CallTarget::VtableField(id) if id.as_str() == naming::vtable_field_name("on_event").as_str()
        ));
    }

    #[test]
    fn lower_callback_inserts_callback_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("notify"),
                params: vec![ParamDef {
                    name: ParamName::new("msg"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Ref,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        assert_eq!(plans[0].params.len(), 2);
        assert_eq!(plans[0].params[0].name.as_str(), "callback");
        assert!(matches!(
            &plans[0].params[0].transport,
            Transport::Callback {
                nullable: false,
                ..
            }
        ));
    }

    #[test]
    fn blittable_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Blittable { size: 8, .. }));
    }

    #[test]
    fn encoded_record_layout() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Person");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        assert!(matches!(layout, RecordLayout::Encoded { .. }));
    }

    #[test]
    fn async_result_handles_result_handle() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Session");
        let async_plan = lowerer.build_async_plan(&ReturnDef::Result {
            ok: TypeExpr::Handle(class_id.clone()),
            err: TypeExpr::String,
        });

        match async_plan.result {
            ReturnPlan::Fallible { ok, err_codec } => {
                match ok {
                    Transport::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Session");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Fallible"),
        }
    }

    #[test]
    fn param_strategy_vec_primitive_owned_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
            &ParamPassing::Value,
        );

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::U8
            )))
        ));
    }

    #[test]
    fn param_strategy_ref_mut_has_mutable() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let param = lowerer.lower_param(&ParamDef {
            name: ParamName::new("s"),
            type_expr: TypeExpr::String,
            passing: ParamPassing::RefMut,
            doc: None,
        });

        assert!(matches!(
            param.transport,
            Transport::Span(SpanContent::Utf8)
        ));
        assert_eq!(param.mutability, Mutability::Mutable);
    }

    #[test]
    fn param_strategy_byte_vec_is_buffer() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let strategy = lowerer.classify_param(
            &TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
            &ParamPassing::Ref,
        );

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                PrimitiveType::U8
            )))
        ));
    }

    #[test]
    fn lower_constructor_fallible_verifies_ok_and_err() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("connect"),
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_constructor(class, &ctor);

        match plan.kind {
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { ok, err_codec },
            } => {
                match ok {
                    Transport::Handle {
                        class_id: id,
                        nullable,
                    } => {
                        assert_eq!(id.as_str(), "Connection");
                        assert!(!nullable);
                    }
                    _ => panic!("expected Handle in ok"),
                }
                assert!(matches!(err_codec, CodecPlan::String));
            }
            _ => panic!("expected Sync Fallible"),
        }
    }

    #[test]
    fn blittable_record_layout_verifies_offsets() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Packed");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Blittable { size, fields } => {
                assert_eq!(size, 12);
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "a");
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[0].primitive, PrimitiveType::U8);
                assert_eq!(fields[1].name.as_str(), "b");
                assert_eq!(fields[1].offset, 4);
                assert_eq!(fields[1].primitive, PrimitiveType::U32);
                assert_eq!(fields[2].name.as_str(), "c");
                assert_eq!(fields[2].offset, 8);
                assert_eq!(fields[2].primitive, PrimitiveType::U8);
            }
            _ => panic!("expected Blittable"),
        }
    }

    #[test]
    fn vec_blittable_record_is_blittable() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let codec = lowerer.build_codec(&TypeExpr::Vec(Box::new(TypeExpr::Record(record_id))));

        match codec {
            CodecPlan::Vec { element, layout } => {
                assert!(matches!(layout, VecLayout::Blittable { element_size: 16 }));
                assert!(matches!(*element, CodecPlan::Record { .. }));
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn vec_blittable_record_uses_composite_span_transport() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let transport = lowerer.classify_type(&TypeExpr::Vec(Box::new(TypeExpr::Record(
            record_id.clone(),
        ))));

        match transport {
            Transport::Span(SpanContent::Composite(layout)) => {
                assert_eq!(layout.record_id, record_id);
                assert_eq!(layout.total_size, 16);
            }
            other => panic!(
                "expected composite span transport for Vec<blittable record>, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn composite_span_param_uses_vec_codec_ops() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Vec2");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(FunctionDef {
            id: FunctionId::new("take_points"),
            params: vec![ParamDef {
                name: ParamName::new("points"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Record(record_id.clone()))),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let abi = lowerer_for_contract(&contract).to_abi_contract();
        let points_param = abi
            .calls
            .iter()
            .find_map(|call| match &call.id {
                CallId::Function(function_id) if function_id.as_str() == "take_points" => call
                    .params
                    .iter()
                    .find(|param| param.name.as_str() == "points"),
                _ => None,
            })
            .expect("points param should exist");

        let ParamRole::Input {
            decode_ops: Some(decode_ops),
            encode_ops: Some(encode_ops),
            ..
        } = &points_param.role
        else {
            panic!("composite span param should expose vec codec ops");
        };

        match &decode_ops.ops[0] {
            ReadOp::Vec { element, .. } => {
                assert!(matches!(element.ops[0], ReadOp::Record { .. }));
            }
            other => panic!("expected vec decode op, got {:?}", other),
        }

        match &encode_ops.ops[0] {
            WriteOp::Vec {
                value,
                element_type,
                element,
                ..
            } => {
                assert_eq!(value, &ValueExpr::Named("points".to_string()));
                assert_eq!(element_type, &TypeExpr::Record(record_id));
                assert!(matches!(element.ops[0], WriteOp::Record { .. }));
            }
            other => panic!("expected vec write op, got {:?}", other),
        }
    }

    #[test]
    fn build_codec_result() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let codec = lowerer.build_codec(&TypeExpr::Result {
            ok: Box::new(TypeExpr::Primitive(PrimitiveType::I64)),
            err: Box::new(TypeExpr::String),
        });

        match codec {
            CodecPlan::Result { ok, err } => {
                assert!(matches!(*ok, CodecPlan::Primitive(PrimitiveType::I64)));
                assert!(matches!(*err, CodecPlan::String));
            }
            _ => panic!("expected Result"),
        }
    }

    #[test]
    fn lower_return_verifies_class_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let class_id = ClassId::new("Database");
        let plan = lowerer.lower_return(&ReturnDef::Value(TypeExpr::Handle(class_id)));

        match plan {
            ReturnPlan::Value(Transport::Handle { class_id, nullable }) => {
                assert_eq!(class_id.as_str(), "Database");
                assert!(!nullable);
            }
            _ => panic!("expected Value Handle"),
        }
    }

    #[test]
    fn lower_callback_verifies_callback_id() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback = CallbackTraitDef {
            id: CallbackId::new("MyCallback"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("invoke"),
                params: vec![],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        };

        let plans = lowerer.lower_callback(&callback);

        match &plans[0].params[0].transport {
            Transport::Callback {
                callback_id,
                style,
                nullable,
            } => {
                assert_eq!(callback_id.as_str(), "MyCallback");
                assert_eq!(style, &CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback strategy"),
        }
    }

    #[test]
    fn param_strategy_callback_boxed_dyn() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let callback_id = CallbackId::new("Handler");
        let strategy = lowerer.classify_param(
            &TypeExpr::Callback(callback_id.clone()),
            &ParamPassing::BoxedDyn,
        );

        match strategy {
            Transport::Callback {
                callback_id: id,
                style,
                nullable,
            } => {
                assert_eq!(id.as_str(), "Handler");
                assert_eq!(style, CallbackStyle::BoxedDyn);
                assert!(!nullable);
            }
            _ => panic!("expected Callback"),
        }
    }

    #[test]
    fn lower_method_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Service");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let method = MethodDef {
            id: MethodId::new("start"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_method(class, &method);

        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(
                    s.as_str(),
                    naming::method_ffi_name("Service", "start").as_str()
                );
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn lower_constructor_verifies_symbol() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Factory");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();

        let default_ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &default_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(s.as_str(), naming::class_ffi_new("Factory").as_str())
            }
            _ => panic!("expected GlobalSymbol"),
        }

        let named_ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("with_config"),
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        };
        let plan = lowerer.lower_constructor(class, &named_ctor);
        match &plan.target {
            CallTarget::GlobalSymbol(s) => {
                assert_eq!(
                    s.as_str(),
                    naming::method_ffi_name("Factory", "with_config").as_str()
                )
            }
            _ => panic!("expected GlobalSymbol"),
        }
    }

    #[test]
    fn encoded_record_verifies_field_codecs() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Message");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("body"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("tags"),
                    type_expr: TypeExpr::Vec(Box::new(TypeExpr::String)),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let layout = lowerer.record_layout(&record_id);

        match layout {
            RecordLayout::Encoded { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.as_str(), "id");
                assert!(matches!(
                    fields[0].codec,
                    CodecPlan::Primitive(PrimitiveType::U64)
                ));
                assert_eq!(fields[1].name.as_str(), "body");
                assert!(matches!(fields[1].codec, CodecPlan::String));
                assert_eq!(fields[2].name.as_str(), "tags");
                assert!(matches!(fields[2].codec, CodecPlan::Vec { .. }));
            }
            _ => panic!("expected Encoded"),
        }
    }

    #[test]
    fn string_param_produces_synthetic_len() {
        let contract = test_contract();
        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("greet"),
            params: vec![ParamDef {
                name: ParamName::new("name"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 2);
        assert!(matches!(
            abi.params[0].role,
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Utf8),
                ..
            }
        ));
        assert_eq!(abi.params[0].name.as_str(), "name");
        match &abi.params[1].role {
            ParamRole::SyntheticLen { for_param } => {
                assert_eq!(for_param.as_str(), "name");
            }
            other => panic!("expected SyntheticLen, got {:?}", other),
        }
    }

    #[test]
    fn blittable_record_param_produces_composite() {
        let mut contract = test_contract();
        let record_id = RecordId::new("Point");
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: record_id.clone(),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);

        let func = FunctionDef {
            id: FunctionId::new("move_to"),
            params: vec![ParamDef {
                name: ParamName::new("point"),
                type_expr: TypeExpr::Record(record_id),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let abi = lowerer.abi_call_for_function(&func);

        assert_eq!(abi.params.len(), 1);
        assert!(matches!(
            abi.params[0].role,
            ParamRole::Input {
                transport: Transport::Composite(_),
                len_param: None,
                decode_ops: Some(_),
                encode_ops: Some(_),
                ..
            }
        ));
        assert_eq!(abi.params[0].name.as_str(), "point");
        assert_eq!(
            abi.params[0].abi_type,
            AbiType::Struct(RecordId::new("Point"))
        );
    }

    #[test]
    fn fallible_constructor_produces_nullable_handle_not_panic() {
        let mut contract = test_contract();
        let class_id = ClassId::new("Connection");
        contract.catalog.insert_class(ClassDef {
            id: class_id.clone(),
            constructors: vec![ConstructorDef::Default {
                params: vec![ParamDef {
                    name: ParamName::new("url"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                is_fallible: true,
                is_optional: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let class = contract.catalog.resolve_class(&class_id).unwrap();
        let abi = lowerer.abi_call_for_constructor(class, &class.constructors[0], 0);

        assert!(matches!(
            abi.returns.transport,
            Some(Transport::Handle { nullable: true, .. })
        ));
        assert!(matches!(abi.error, ErrorTransport::Encoded { .. }));
    }

    fn contract_with_closure(
        callback_id: &str,
        params: Vec<ParamDef>,
        returns: ReturnDef,
    ) -> FfiContract {
        let mut contract = test_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new(callback_id),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params,
                returns,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract
    }

    #[test]
    fn closure_void_return_yields_void_abi_type() {
        let contract = contract_with_closure(
            "__Closure_I32",
            vec![ParamDef {
                name: ParamName::new("x"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Void,
        );
        let lowerer = lowerer_for_contract(&contract);
        let (params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_I32"));
        assert_eq!(params, vec![AbiType::I32]);
        assert_eq!(ret, AbiType::Void);
    }

    #[test]
    fn closure_primitive_return_yields_primitive_abi_type() {
        let contract = contract_with_closure(
            "__Closure_I32ToI32",
            vec![ParamDef {
                name: ParamName::new("x"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        );
        let lowerer = lowerer_for_contract(&contract);
        let (params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_I32ToI32"));
        assert_eq!(params, vec![AbiType::I32]);
        assert_eq!(ret, AbiType::I32);
    }

    #[test]
    fn closure_blittable_record_return_yields_struct_abi_type() {
        let mut contract = contract_with_closure(
            "__Closure_PointToPoint",
            vec![ParamDef {
                name: ParamName::new("p"),
                type_expr: TypeExpr::Record(RecordId::new("Point")),
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
        );
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Point"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        let lowerer = lowerer_for_contract(&contract);
        let (_params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_PointToPoint"));
        assert_eq!(ret, AbiType::Struct(RecordId::new("Point")));
    }

    #[test]
    fn closure_string_return_yields_owned_buffer_abi_type() {
        let contract = contract_with_closure(
            "__Closure_StringToString",
            vec![ParamDef {
                name: ParamName::new("s"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            ReturnDef::Value(TypeExpr::String),
        );
        let lowerer = lowerer_for_contract(&contract);
        let (_params, ret) =
            lowerer.inline_callback_fn_abi_signature(&CallbackId::new("__Closure_StringToString"));
        assert_eq!(ret, AbiType::OwnedBuffer);
    }

    fn blittable_point_record() -> RecordDef {
        RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Point"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn wire_encoded_person_record() -> RecordDef {
        RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Person"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("age"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn lower_record_method_inserts_self_as_value() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("magnitude"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert_eq!(plan.params.len(), 1);
        assert_eq!(plan.params[0].name.as_str(), "self");
        assert!(matches!(
            &plan.params[0].transport,
            Transport::Composite(layout) if layout.record_id.as_str() == "Point"
        ));
    }

    #[test]
    fn lower_record_method_static_no_self() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("origin"),
            receiver: Receiver::Static,
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert_eq!(plan.params.len(), 0);
    }

    #[test]
    fn lower_record_method_wire_encoded_self() {
        let mut contract = test_contract();
        let record = wire_encoded_person_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("greet"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert_eq!(plan.params.len(), 1);
        assert_eq!(plan.params[0].name.as_str(), "self");
        assert!(matches!(
            &plan.params[0].transport,
            Transport::Span(SpanContent::Encoded(_))
        ));
    }

    #[test]
    fn lower_record_constructor_infallible() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let ctor = ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_constructor(&record, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(Transport::Composite(_))
            }
        ));
    }

    #[test]
    fn lower_record_constructor_fallible() {
        let mut contract = test_contract();
        let record = wire_encoded_person_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let ctor = ConstructorDef::NamedFactory {
            name: MethodId::new("try_parse"),
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_constructor(&record, &ctor);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Fallible { .. }
            }
        ));
        if let CallPlanKind::Sync {
            returns: ReturnPlan::Fallible { ok, .. },
        } = &plan.kind
        {
            assert!(matches!(ok, Transport::Span(SpanContent::Encoded(_))));
        }
    }

    #[test]
    fn to_abi_contract_includes_record_calls() {
        let mut contract = test_contract();
        let mut record = blittable_point_record();
        record.constructors = vec![ConstructorDef::Default {
            params: vec![],
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }];
        record.methods = vec![MethodDef {
            id: MethodId::new("magnitude"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }];
        contract.catalog.insert_record(record);

        let lowerer = lowerer_for_contract(&contract);
        let abi = lowerer.to_abi_contract();

        let record_ctors: Vec<_> = abi
            .calls
            .iter()
            .filter(|c| matches!(&c.id, CallId::RecordConstructor { .. }))
            .collect();
        let record_methods: Vec<_> = abi
            .calls
            .iter()
            .filter(|c| matches!(&c.id, CallId::RecordMethod { .. }))
            .collect();

        assert_eq!(record_ctors.len(), 1);
        assert_eq!(record_methods.len(), 1);

        assert!(matches!(
            &record_ctors[0].id,
            CallId::RecordConstructor {
                record_id,
                index: 0,
            } if record_id.as_str() == "Point"
        ));
        assert!(matches!(
            &record_methods[0].id,
            CallId::RecordMethod {
                record_id,
                method_id,
            } if record_id.as_str() == "Point" && method_id.as_str() == "magnitude"
        ));
    }

    #[test]
    fn to_abi_contract_excludes_async_record_methods() {
        let mut contract = test_contract();
        let mut record = blittable_point_record();
        record.methods = vec![
            MethodDef {
                id: MethodId::new("sync_method"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            },
            MethodDef {
                id: MethodId::new("async_method"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                execution_kind: ExecutionKind::Async,
                doc: None,
                deprecated: None,
            },
        ];
        contract.catalog.insert_record(record);

        let lowerer = lowerer_for_contract(&contract);
        let abi = lowerer.to_abi_contract();

        let record_methods: Vec<_> = abi
            .calls
            .iter()
            .filter(|c| matches!(&c.id, CallId::RecordMethod { .. }))
            .collect();

        assert_eq!(record_methods.len(), 1);
        assert!(matches!(
            &record_methods[0].id,
            CallId::RecordMethod { method_id, .. } if method_id.as_str() == "sync_method"
        ));
    }

    #[test]
    fn lower_record_method_mut_self_has_mutable_param() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("normalize"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert_eq!(plan.params.len(), 1);
        assert_eq!(plan.params[0].mutability, Mutability::Mutable);
    }

    #[test]
    fn lower_record_method_mut_self_returns_record_writeback() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("normalize"),
            receiver: Receiver::RefMutSelf,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Value(Transport::Composite(_))
            }
        ));
    }

    #[test]
    fn lower_record_method_ref_self_stays_shared() {
        let mut contract = test_contract();
        let record = blittable_point_record();
        contract.catalog.insert_record(record.clone());
        let lowerer = lowerer_for_contract(&contract);

        let method = MethodDef {
            id: MethodId::new("magnitude"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        };

        let plan = lowerer.lower_value_type_method(&record, &method);

        assert_eq!(plan.params[0].mutability, Mutability::Shared);
        assert!(matches!(
            plan.kind,
            CallPlanKind::Sync {
                returns: ReturnPlan::Void
            }
        ));
    }

    fn c_style_enum_with_method() -> EnumDef {
        EnumDef {
            id: EnumId::new("Direction"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    CStyleVariant {
                        name: VariantName::new("North"),
                        discriminant: 0,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("South"),
                        discriminant: 1,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("opposite"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            }],
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn enum_method_lowered_via_method_host() {
        let mut contract = test_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());

        let lowerer = lowerer_for_contract(&contract);
        let abi = lowerer.to_abi_contract();

        let enum_methods: Vec<_> = abi
            .calls
            .iter()
            .filter(|c| matches!(&c.id, CallId::EnumMethod { .. }))
            .collect();

        assert_eq!(enum_methods.len(), 1);
        assert!(matches!(
            &enum_methods[0].id,
            CallId::EnumMethod { enum_id, method_id }
                if enum_id.as_str() == "Direction" && method_id.as_str() == "opposite"
        ));
    }

    #[test]
    fn c_style_enum_self_is_scalar() {
        let mut contract = test_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());

        let lowerer = lowerer_for_contract(&contract);
        let enum_def = contract.catalog.all_enums().next().unwrap();
        let method = &enum_def.methods[0];
        let plan = lowerer.lower_value_type_method(enum_def, method);

        let self_param = plan.params.first().unwrap();
        assert_eq!(self_param.name.as_str(), "self");
        assert!(
            matches!(
                self_param.transport,
                Transport::Scalar(ScalarOrigin::CStyleEnum { .. })
            ),
            "c-style enum self should be Scalar, got: {:?}",
            self_param.transport
        );
    }

    #[test]
    fn vec_c_style_enum_uses_wire_encoded_transport() {
        let mut contract = test_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());

        let lowerer = lowerer_for_contract(&contract);
        let strategy = lowerer.classify_type(&TypeExpr::Vec(Box::new(TypeExpr::Enum(
            EnumId::new("Direction"),
        ))));

        assert!(matches!(
            strategy,
            Transport::Span(SpanContent::Encoded(CodecPlan::Vec { .. }))
        ));
    }

    #[test]
    fn callback_param_c_style_enum_uses_scalar_transport() {
        let mut contract = test_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("DirectionMapper"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("map_direction"),
                params: vec![ParamDef {
                    name: ParamName::new("direction"),
                    type_expr: TypeExpr::Enum(EnumId::new("Direction")),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::Enum(EnumId::new("Direction"))),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let lowerer = lowerer_for_contract(&contract);
        let abi = lowerer.to_abi_contract();
        let callback = abi
            .callbacks
            .iter()
            .find(|callback| callback.callback_id.as_str() == "DirectionMapper")
            .expect("callback should be lowered");
        let param = callback.methods[0]
            .params
            .iter()
            .find(|param| param.name.as_str() == "direction")
            .expect("callback param should be present");

        assert!(matches!(
            &param.role,
            ParamRole::Input {
                transport: Transport::Scalar(ScalarOrigin::CStyleEnum { .. }),
                ..
            }
        ));
        assert_eq!(param.abi_type, AbiType::I32);
    }
}
