use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::naming::{
    self, snake_to_camel as camel_case, to_upper_camel_case as pascal_case,
};
use boltffi_ffi_rules::transport::{
    EncodedReturnStrategy, ErrorReturnStrategy, ReturnInvocationContext, ReturnPlatform,
    ScalarReturnStrategy, ValueReturnMethod, ValueReturnStrategy,
};
use heck::ToLowerCamelCase;

use std::collections::HashMap;

use super::emit;
use super::plan::{
    CompositeFieldMapping, DirectBufferCompositeMapping, SwiftAsyncConversion, SwiftAsyncResult,
    SwiftCallMode, SwiftCallback, SwiftCallbackMethod, SwiftCallbackParam, SwiftClass,
    SwiftClosureTrampoline, SwiftClosureTrampolineParam, SwiftConstructor, SwiftConversion,
    SwiftCustomType, SwiftEnum, SwiftEnumStyle, SwiftField, SwiftFunction, SwiftMethod,
    SwiftModule, SwiftNativeConversion, SwiftNativeMapping, SwiftParam, SwiftRecord, SwiftReturn,
    SwiftStream, SwiftStreamItemDelivery, SwiftStreamMode, SwiftVariant, SwiftVariantPayload,
    ValueSelfParam,
};
use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnShape, StreamItemTransport,
};
use crate::ir::codec::CodecPlan;
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, ConstructorDef, DefaultValue, EnumRepr, MethodDef, ParamDef,
    Receiver, ReturnDef, StreamDef, StreamMode,
};
use crate::ir::ids::{CallbackId, ClassId, EnumId, FieldName, ParamName, RecordId};
use crate::ir::ops::{
    FieldReadOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq,
    remap_root_in_seq,
};
use crate::ir::plan::{
    AbiType, CallbackStyle, CompositeLayout, Mutability, ScalarOrigin, SpanContent, Transport,
};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::{TypeConversion, TypeMappings};

struct AbiIndex {
    calls: HashMap<CallId, usize>,
    callbacks: HashMap<CallbackId, usize>,
    records: HashMap<RecordId, usize>,
    enums: HashMap<EnumId, usize>,
}

impl AbiIndex {
    fn new(contract: &AbiContract) -> Self {
        let calls = contract
            .calls
            .iter()
            .enumerate()
            .map(|(index, call)| (call.id.clone(), index))
            .collect();
        let callbacks = contract
            .callbacks
            .iter()
            .enumerate()
            .map(|(index, callback)| (callback.callback_id.clone(), index))
            .collect();
        let records = contract
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.id.clone(), index))
            .collect();
        let enums = contract
            .enums
            .iter()
            .enumerate()
            .map(|(index, enumeration)| (enumeration.id.clone(), index))
            .collect();

        Self {
            calls,
            callbacks,
            records,
            enums,
        }
    }

    fn call<'a>(&self, contract: &'a AbiContract, id: &CallId) -> &'a AbiCall {
        let index = self.calls.get(id).expect("abi call should exist");
        &contract.calls[*index]
    }

    fn callback<'a>(
        &self,
        contract: &'a AbiContract,
        id: &CallbackId,
    ) -> &'a AbiCallbackInvocation {
        let index = self.callbacks.get(id).expect("abi callback should exist");
        &contract.callbacks[*index]
    }

    fn record<'a>(&self, contract: &'a AbiContract, id: &RecordId) -> &'a AbiRecord {
        let index = self.records.get(id).expect("record should exist");
        &contract.records[*index]
    }

    fn enumeration<'a>(&self, contract: &'a AbiContract, id: &EnumId) -> &'a AbiEnum {
        let index = self.enums.get(id).expect("enum should exist");
        &contract.enums[*index]
    }
}

pub struct SwiftLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    abi_index: AbiIndex,
    type_mappings: TypeMappings,
}

// ─────────────────────────────────────────────────────────────────────────────
// Construction
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    pub fn new(contract: &'a FfiContract, abi: &'a AbiContract) -> Self {
        Self {
            contract,
            abi,
            abi_index: AbiIndex::new(abi),
            type_mappings: TypeMappings::new(),
        }
    }

    pub fn with_type_mappings(mut self, mappings: TypeMappings) -> Self {
        self.type_mappings = mappings;
        self
    }

    pub fn lower(self) -> SwiftModule {
        let custom_types = self.lower_custom_types();
        let records = self.lower_records();
        let enums = self.lower_enums();
        let classes = self.lower_classes();
        let callbacks = self.lower_callbacks();
        let functions = self.lower_functions();

        SwiftModule {
            imports: vec!["Foundation".to_string()],
            custom_types,
            records,
            enums,
            classes,
            callbacks,
            functions,
        }
    }

    fn resolve_swift_type(&self, type_expr: &TypeExpr) -> String {
        match type_expr {
            TypeExpr::Custom(id) => self.swift_named_custom_type(id.as_str()),
            TypeExpr::Option(inner) => format!("{}?", self.resolve_swift_type(inner)),
            TypeExpr::Vec(inner) => format!("[{}]", self.resolve_swift_type(inner)),
            TypeExpr::Result { ok, err } => self.swift_result_type(ok, err),
            _ => emit::swift_type(type_expr),
        }
    }

    fn swift_named_custom_type(&self, id: &str) -> String {
        if let Some(mapping) = self.type_mappings.get(id) {
            mapping.native_type.clone()
        } else if let Some(native_builtin) = self.swift_named_builtin(id) {
            native_builtin
        } else {
            pascal_case(id)
        }
    }

    fn swift_named_builtin(&self, id: &str) -> Option<String> {
        match id {
            "Duration" => Some("TimeInterval".to_string()),
            "SystemTime" => Some("Date".to_string()),
            "Uuid" => Some("UUID".to_string()),
            "Url" => Some("URL".to_string()),
            _ => None,
        }
    }

    fn swift_result_type(&self, ok: &TypeExpr, err: &TypeExpr) -> String {
        format!(
            "Result<{}, {}>",
            self.resolve_swift_type(ok),
            self.swift_result_error_type(err)
        )
    }

    fn swift_result_error_type(&self, err: &TypeExpr) -> String {
        match err {
            TypeExpr::String => "FfiError".to_string(),
            _ => self.resolve_swift_type(err),
        }
    }

    fn is_swift_error_type(&self, type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::Enum(enum_id) => self
                .contract
                .catalog
                .resolve_enum(enum_id)
                .map(|enum_def| enum_def.is_error)
                .unwrap_or(false),
            _ => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom Types
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_custom_types(&self) -> Vec<SwiftCustomType> {
        self.contract
            .catalog
            .all_custom_types()
            .map(|def| {
                let alias_name = pascal_case(def.id.as_str());
                let target_type = emit::swift_type(&def.repr);
                let native_mapping = self
                    .type_mappings
                    .get(def.id.as_str())
                    .map(|mapping| self.build_native_mapping(mapping, &target_type));
                SwiftCustomType {
                    alias_name,
                    target_type,
                    native_mapping,
                }
            })
            .collect()
    }

    fn build_native_mapping(
        &self,
        mapping: &crate::render::TypeMapping,
        _repr_type: &str,
    ) -> SwiftNativeMapping {
        let (decode_expr, encode_expr) = match mapping.conversion {
            TypeConversion::UuidString => (
                "UUID(uuidString: $0)!".to_string(),
                "$0.uuidString".to_string(),
            ),
            TypeConversion::UrlString => (
                "URL(string: $0)!".to_string(),
                "$0.absoluteString".to_string(),
            ),
        };

        SwiftNativeMapping {
            native_type: mapping.native_type.clone(),
            decode_expr,
            encode_expr,
        }
    }

    fn native_conversion_for_type(&self, type_expr: &TypeExpr) -> Option<SwiftNativeConversion> {
        match type_expr {
            TypeExpr::Custom(id) => self.type_mappings.get(id.as_str()).map(|mapping| {
                let (decode_wrapper, encode_wrapper) = match mapping.conversion {
                    TypeConversion::UuidString => (
                        "UUID(uuidString: $0)!".to_string(),
                        "$0.uuidString".to_string(),
                    ),
                    TypeConversion::UrlString => (
                        "URL(string: $0)!".to_string(),
                        "$0.absoluteString".to_string(),
                    ),
                };
                SwiftNativeConversion {
                    decode_wrapper,
                    encode_wrapper,
                }
            }),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Records
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_records(&self) -> Vec<SwiftRecord> {
        self.contract
            .catalog
            .all_records()
            .map(|def| {
                let abi_record = self.abi_index.record(self.abi, &def.id);
                let decode_fields = self.record_decode_fields(abi_record);
                let encode_fields = self.record_encode_fields(abi_record);
                let fields =
                    def.fields
                        .iter()
                        .map(|field| {
                            let swift_name = camel_case(field.name.as_str());
                            let decode =
                                decode_fields.get(&field.name).cloned().unwrap_or_else(|| {
                                    ReadSeq {
                                        size: SizeExpr::Fixed(0),
                                        ops: vec![],
                                        shape: WireShape::Value,
                                    }
                                });
                            let encode =
                                encode_fields.get(&field.name).cloned().unwrap_or_else(|| {
                                    WriteSeq {
                                        size: SizeExpr::Fixed(0),
                                        ops: vec![],
                                        shape: WireShape::Value,
                                    }
                                });
                            let c_offset = if abi_record.is_blittable {
                                decode_fields
                                    .get(&field.name)
                                    .and_then(|seq| self.record_field_offset(seq))
                            } else {
                                None
                            };
                            let native_conversion =
                                self.native_conversion_for_type(&field.type_expr);
                            SwiftField {
                                swift_name,
                                swift_type: self.swift_type(&field.type_expr),
                                default_expr: field.default.as_ref().map(swift_default_literal),
                                decode,
                                encode,
                                doc: field.doc.clone(),
                                c_offset,
                                native_conversion,
                            }
                        })
                        .collect();

                let constructors = def
                    .constructor_calls()
                    .filter_map(|(call_id, ctor)| {
                        let call = self.resolve_abi_call(&call_id)?;
                        Some(self.lower_value_type_constructor(ctor, call))
                    })
                    .collect();
                let methods = def
                    .method_calls()
                    .filter(|(_, method)| !method.is_async())
                    .filter_map(|(call_id, method)| {
                        let call = self.resolve_abi_call(&call_id)?;
                        Some(self.lower_value_type_method(method, call, &abi_record.encode_ops))
                    })
                    .collect();

                SwiftRecord {
                    class_name: self.swift_name_for_record(&def.id),
                    fields,
                    is_blittable: abi_record.is_blittable,
                    is_error: def.is_error,
                    blittable_size: abi_record.size,
                    constructors,
                    methods,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn resolve_abi_call(&self, call_id: &CallId) -> Option<&AbiCall> {
        self.abi_index
            .calls
            .get(call_id)
            .map(|i| &self.abi.calls[*i])
    }

    fn lower_value_type_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
    ) -> SwiftConstructor {
        let throw_decode_expr = self.constructor_throw_decode_expr(call);
        match ctor {
            ConstructorDef::Default {
                is_fallible,
                is_optional,
                doc,
                ..
            } => SwiftConstructor::Designated {
                ffi_symbol: call.symbol.as_str().to_string(),
                closure_return_type: self.value_type_constructor_closure_return_type(call),
                params: ctor
                    .params()
                    .into_iter()
                    .map(|p| self.lower_param(p, call))
                    .collect(),
                is_fallible: *is_fallible,
                is_optional: *is_optional,
                throw_decode_expr: throw_decode_expr.clone(),
                doc: doc.clone(),
            },
            ConstructorDef::NamedFactory {
                name,
                is_fallible,
                is_optional,
                doc,
                ..
            } => SwiftConstructor::Factory {
                name: camel_case(name.as_str()),
                ffi_symbol: call.symbol.as_str().to_string(),
                is_fallible: *is_fallible,
                is_optional: *is_optional,
                throw_decode_expr: throw_decode_expr.clone(),
                doc: doc.clone(),
            },
            ConstructorDef::NamedInit {
                name,
                first_param,
                rest_params,
                is_fallible,
                is_optional,
                doc,
                ..
            } => {
                let label = camel_case(name.as_str());
                let mut first = self.lower_param(first_param, call);
                first.label = Some(label.clone());
                let rest = rest_params.iter().map(|p| self.lower_param(p, call));
                SwiftConstructor::Convenience {
                    name: label,
                    ffi_symbol: call.symbol.as_str().to_string(),
                    closure_return_type: self.value_type_constructor_closure_return_type(call),
                    params: std::iter::once(first).chain(rest).collect(),
                    is_fallible: *is_fallible,
                    is_optional: *is_optional,
                    throw_decode_expr,
                    doc: doc.clone(),
                }
            }
        }
    }

    fn value_type_constructor_closure_return_type(&self, call: &AbiCall) -> String {
        match (&call.error, &call.returns.transport) {
            (ErrorTransport::Encoded { .. }, _) => "FfiBuf_u8".to_string(),
            (ErrorTransport::StatusCode, None) => "FfiStatus".to_string(),
            (_, None) => "Void".to_string(),
            (_, Some(Transport::Scalar(origin))) => {
                self.abi_to_swift(&AbiType::from(origin.primitive()))
            }
            (_, Some(Transport::Composite(layout))) => format!("___{}", layout.record_id.as_str()),
            (_, Some(Transport::Span(_))) => "FfiBuf_u8".to_string(),
            (_, Some(Transport::Handle { .. })) => "OpaquePointer?".to_string(),
            (_, Some(Transport::Callback { .. })) => "BoltFFICallbackHandle".to_string(),
        }
    }

    fn constructor_throw_decode_expr(&self, call: &AbiCall) -> Option<String> {
        match &call.error {
            ErrorTransport::Encoded { decode_ops, .. } => {
                let decode_expr = emit::emit_reader_read(decode_ops);
                let throw_expr = match decode_ops.ops.first() {
                    Some(ReadOp::String { .. }) => {
                        format!("FfiError(message: {})", decode_expr)
                    }
                    _ => decode_expr,
                };
                Some(throw_expr)
            }
            ErrorTransport::None | ErrorTransport::StatusCode => None,
        }
    }

    fn lower_value_type_method(
        &self,
        method: &MethodDef,
        call: &AbiCall,
        encode_ops: &WriteSeq,
    ) -> SwiftMethod {
        let value_self = if method.receiver != Receiver::Static {
            Some(Self::build_value_self_param(
                call,
                encode_ops,
                method.receiver == Receiver::RefMutSelf,
            ))
        } else {
            None
        };

        let mut returns = self.swift_return_from_abi(&call.returns, &call.error, &method.returns);

        let mutating_void =
            method.receiver == Receiver::RefMutSelf && matches!(method.returns, ReturnDef::Void);

        if mutating_void && let Some(Transport::Composite(layout)) = &call.returns.transport {
            returns.set_composite_swift_type(self.swift_name_for_record(&layout.record_id));
        }

        SwiftMethod {
            name: camel_case(method.id.as_str()),
            mode: SwiftCallMode::Sync {
                symbol: call.symbol.as_str().to_string(),
            },
            params: method
                .params
                .iter()
                .map(|p| self.lower_param(p, call))
                .collect(),
            returns,
            is_static: method.callable_form() == CallableForm::StaticMethod,
            value_self,
            mutating_void,
            doc: method.doc.clone(),
        }
    }

    fn build_value_self_param(
        call: &AbiCall,
        encode_ops: &WriteSeq,
        is_mutating: bool,
    ) -> ValueSelfParam {
        let self_abi_param = call
            .params
            .iter()
            .find(|p| p.name.as_str() == "self")
            .expect("value type instance method must have self param");

        match &self_abi_param.role {
            ParamRole::Input {
                transport: Transport::Composite(layout),
                ..
            } => {
                let c_type = format!("___{}", layout.record_id.as_str());
                let field_inits: Vec<String> = layout
                    .fields
                    .iter()
                    .map(|f| format!("{}: self.{}", f.name.as_str(), camel_case(f.name.as_str())))
                    .collect();
                ValueSelfParam {
                    ffi_args: vec![format!("{}({})", c_type, field_inits.join(", "))],
                    wrapper_code: None,
                    is_mutating,
                }
            }
            ParamRole::Input {
                transport: Transport::Scalar(ScalarOrigin::CStyleEnum { .. }),
                ..
            } => ValueSelfParam {
                ffi_args: vec!["self.rawValue".to_string()],
                wrapper_code: None,
                is_mutating,
            },
            _ => {
                let writer_body = emit::emit_writer_write(encode_ops);
                ValueSelfParam {
                    ffi_args: vec!["selfBytes".to_string(), "UInt(selfBytes.count)".to_string()],
                    wrapper_code: Some(format!(
                        "let selfBytes = boltffiEncode {{ writer in {} }}",
                        writer_body
                    )),
                    is_mutating,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_enums(&self) -> Vec<SwiftEnum> {
        self.contract
            .catalog
            .all_enums()
            .map(|def| {
                let abi_enum = self.abi_index.enumeration(self.abi, &def.id);
                let variant_docs = def.variant_docs();
                let variants = abi_enum
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, variant)| SwiftVariant {
                        swift_name: emit::escape_swift_keyword(
                            &variant.name.as_str().to_lower_camel_case(),
                        ),
                        discriminant: variant.discriminant,
                        payload: self.lower_variant_payload(variant),
                        doc: variant_docs.get(i).cloned().flatten(),
                    })
                    .collect();

                let style = if abi_enum.is_c_style {
                    SwiftEnumStyle::CStyle
                } else {
                    SwiftEnumStyle::Data
                };
                let constructors = def
                    .constructor_calls()
                    .filter_map(|(call_id, ctor)| {
                        let call = self.resolve_abi_call(&call_id)?;
                        Some(self.lower_value_type_constructor(ctor, call))
                    })
                    .collect();
                let methods = def
                    .method_calls()
                    .filter(|(_, method)| !method.is_async())
                    .filter_map(|(call_id, method)| {
                        let call = self.resolve_abi_call(&call_id)?;
                        Some(self.lower_value_type_method(method, call, &abi_enum.encode_ops))
                    })
                    .collect();

                SwiftEnum {
                    name: self.swift_name_for_enum(&def.id),
                    variants,
                    style,
                    c_style_tag_type: match &def.repr {
                        EnumRepr::CStyle { tag_type, .. } => Some(*tag_type),
                        _ => None,
                    },
                    is_error: def.is_error,
                    constructors,
                    methods,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_variant_payload(&self, variant: &AbiEnumVariant) -> SwiftVariantPayload {
        match &variant.payload {
            AbiEnumPayload::Unit => SwiftVariantPayload::Unit,
            // we match tuple variants as case .foo(let value) so we get one
            // binding for everything and need to remap all fields to encode
            // from value instead of individual names
            AbiEnumPayload::Tuple(fields) => SwiftVariantPayload::Tuple(
                fields
                    .iter()
                    .map(|field| {
                        let lowered = self.lower_enum_field(field);
                        let encode =
                            remap_root_in_seq(&lowered.encode, ValueExpr::Var("value".into()));
                        SwiftField { encode, ..lowered }
                    })
                    .collect(),
            ),
            // if the field name is just 0 or 1 we dont have a real name to
            // bind in the switch so we treat it as value same as tuples
            AbiEnumPayload::Struct(fields) => SwiftVariantPayload::Struct(
                fields
                    .iter()
                    .map(|field| {
                        let lowered = self.lower_enum_field(field);
                        if lowered.swift_name.chars().all(|c| c.is_ascii_digit()) {
                            let encode =
                                remap_root_in_seq(&lowered.encode, ValueExpr::Var("value".into()));
                            SwiftField { encode, ..lowered }
                        } else {
                            lowered
                        }
                    })
                    .collect(),
            ),
        }
    }

    fn lower_enum_field(&self, field: &AbiEnumField) -> SwiftField {
        let swift_name = camel_case(field.name.as_str());
        let encode = field.encode.clone();
        let native_conversion = self.native_conversion_for_type(&field.type_expr);
        SwiftField {
            swift_name,
            swift_type: self.swift_type(&field.type_expr),
            default_expr: None,
            decode: field.decode.clone(),
            encode,
            doc: None,
            c_offset: None,
            native_conversion,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Classes
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_classes(&self) -> Vec<SwiftClass> {
        self.contract
            .catalog
            .all_classes()
            .map(|def| {
                let class_name = self.swift_name_for_class(&def.id);
                let ffi_free = naming::class_ffi_free(def.id.as_str()).to_string();

                let constructors = def
                    .constructors
                    .iter()
                    .enumerate()
                    .map(|(idx, ctor)| {
                        let call = self.abi_call(&CallId::Constructor {
                            class_id: def.id.clone(),
                            index: idx,
                        });
                        let throw_decode_expr = self.constructor_throw_decode_expr(call);

                        match ctor {
                            ConstructorDef::Default {
                                is_fallible,
                                is_optional,
                                doc,
                                ..
                            } => SwiftConstructor::Designated {
                                ffi_symbol: call.symbol.as_str().to_string(),
                                closure_return_type: "OpaquePointer?".to_string(),
                                params: ctor
                                    .params()
                                    .into_iter()
                                    .map(|p| self.lower_param(p, call))
                                    .collect(),
                                is_fallible: *is_fallible,
                                is_optional: *is_optional,
                                throw_decode_expr: throw_decode_expr.clone(),
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedFactory {
                                name,
                                is_fallible,
                                is_optional,
                                doc,
                                ..
                            } => SwiftConstructor::Factory {
                                name: camel_case(name.as_str()),
                                ffi_symbol: call.symbol.as_str().to_string(),
                                is_fallible: *is_fallible,
                                is_optional: *is_optional,
                                throw_decode_expr: throw_decode_expr.clone(),
                                doc: doc.clone(),
                            },
                            ConstructorDef::NamedInit {
                                name,
                                first_param,
                                rest_params,
                                is_fallible,
                                is_optional,
                                doc,
                                ..
                            } => {
                                let label = camel_case(name.as_str());
                                let mut first = self.lower_param(first_param, call);
                                first.label = Some(label.clone());
                                let rest = rest_params.iter().map(|p| self.lower_param(p, call));
                                SwiftConstructor::Convenience {
                                    name: label,
                                    ffi_symbol: call.symbol.as_str().to_string(),
                                    closure_return_type: "OpaquePointer?".to_string(),
                                    params: std::iter::once(first).chain(rest).collect(),
                                    is_fallible: *is_fallible,
                                    is_optional: *is_optional,
                                    throw_decode_expr,
                                    doc: doc.clone(),
                                }
                            }
                        }
                    })
                    .collect();

                let methods =
                    def.methods
                        .iter()
                        .map(|method| {
                            let call = self.abi_call(&CallId::Method {
                                class_id: def.id.clone(),
                                method_id: method.id.clone(),
                            });
                            let mode = self.lower_call_mode(call, &method.returns);
                            // we get the actual value back through poll/complete for async
                            // so all we set up here is the error wrapping
                            let returns = match &call.mode {
                                CallMode::Async(async_call) => self
                                    .lower_return_def_for_async(&async_call.error, &method.returns),
                                CallMode::Sync => self.swift_return_from_abi(
                                    &call.returns,
                                    &call.error,
                                    &method.returns,
                                ),
                            };

                            SwiftMethod {
                                name: camel_case(method.id.as_str()),
                                mode,
                                params: method
                                    .params
                                    .iter()
                                    .map(|p| self.lower_param(p, call))
                                    .collect(),
                                returns,
                                is_static: method.callable_form() == CallableForm::StaticMethod,
                                value_self: None,
                                mutating_void: false,
                                doc: method.doc.clone(),
                            }
                        })
                        .collect();

                let streams = def
                    .streams
                    .iter()
                    .map(|stream_def| {
                        let abi_stream = self
                            .abi
                            .streams
                            .iter()
                            .find(|stream| {
                                stream.class_id == def.id && stream.stream_id == stream_def.id
                            })
                            .expect("abi stream");
                        self.lower_stream(stream_def, abi_stream, &class_name)
                    })
                    .collect();

                SwiftClass {
                    name: class_name,
                    ffi_free,
                    constructors,
                    methods,
                    streams,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_stream(
        &self,
        stream_def: &StreamDef,
        stream: &AbiStream,
        class_name: &str,
    ) -> SwiftStream {
        let method_name_pascal = pascal_case(stream.stream_id.as_str());

        let mode = match stream.mode {
            StreamMode::Async => SwiftStreamMode::Async,
            StreamMode::Batch => SwiftStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => SwiftStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };

        SwiftStream {
            name: camel_case(stream.stream_id.as_str()),
            mode,
            item_type: self.swift_type(&stream_def.item_type),
            item_delivery: self.lower_stream_item_delivery(stream),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
            free_buf: self.abi.free_buf.to_string(),
            atomic_cas: self.abi.atomic_cas.to_string(),
        }
    }

    fn lower_stream_item_delivery(&self, stream: &AbiStream) -> SwiftStreamItemDelivery {
        match &stream.item_transport {
            Transport::Scalar(origin) => SwiftStreamItemDelivery::Direct {
                c_element_type: self.abi_to_swift(&AbiType::from(origin.primitive())),
                item_expr_template: match origin {
                    ScalarOrigin::Primitive(_) => "$0".to_string(),
                    ScalarOrigin::CStyleEnum { enum_id, .. } => {
                        format!("{}(fromC: $0)", self.swift_name_for_enum(enum_id))
                    }
                },
            },
            Transport::Composite(layout) => SwiftStreamItemDelivery::Direct {
                c_element_type: format!("___{}", layout.record_id.as_str()),
                item_expr_template: self.swift_composite_value_expr(layout, "$0"),
            },
            _ => {
                let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
                SwiftStreamItemDelivery::WireEncoded {
                    item_decode: self.rebase_read_seq(decode_ops, "pos", "0"),
                }
            }
        }
    }

    fn swift_composite_value_expr(&self, layout: &CompositeLayout, raw_value_expr: &str) -> String {
        let record_name = self.swift_name_for_record(&layout.record_id);
        let field_exprs = self
            .composite_field_mappings(layout)
            .into_iter()
            .map(|field| format!("{}: {}.{}", field.swift_name, raw_value_expr, field.c_name))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{record_name}({field_exprs})")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Callbacks
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_callbacks(&self) -> Vec<SwiftCallback> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|def| def.kind == CallbackKind::Trait)
            .map(|def| {
                let protocol_name = pascal_case(def.id.as_str());
                let vtable_var = format!("{}VTableInstance", lower_first_char(&protocol_name));
                let plan = self.abi_index.callback(self.abi, &def.id);

                let methods = def
                    .methods
                    .iter()
                    .map(|method_def| {
                        let abi_method = plan
                            .methods
                            .iter()
                            .find(|m| m.id == method_def.id)
                            .expect("callback method");
                        // IR generates encode ops with self as root but we capture the
                        // callback return as result in the template so we need to rewrite
                        // the root before handing it off
                        let returns = Self::rebase_return_encode(
                            self.swift_return_from_abi_with_context(
                                &abi_method.returns,
                                &abi_method.error,
                                &method_def.returns,
                                ReturnInvocationContext::CallbackVtable,
                            ),
                            "result",
                        );
                        let has_out_param =
                            abi_method.execution_kind() == ExecutionKind::Sync && !returns.is_void();
                        let param_map = method_def
                            .params
                            .iter()
                            .map(|param| (param.name.clone(), param))
                            .collect::<HashMap<_, _>>();
                        let params = abi_method
                            .params
                            .iter()
                            .filter(|param| matches!(
                                param.role,
                                ParamRole::Input {
                                    transport: Transport::Scalar(_) | Transport::Span(SpanContent::Encoded(_)),
                                    ..
                                }
                            ))
                            .map(|param| {
                                let def = param_map.get(&param.name).unwrap_or_else(|| {
                                    unreachable!(
                                        "param def not found: callback={}, method={}, param={}, role={:?}",
                                        plan.callback_id.as_str(),
                                        abi_method.id.as_str(),
                                        param.name.as_str(),
                                        param.role,
                                    )
                                });
                                self.lower_callback_param(def, param)
                            })
                            .collect();

                        SwiftCallbackMethod {
                            swift_name: camel_case(abi_method.id.as_str()),
                            ffi_name: abi_method.vtable_field.as_str().to_string(),
                            params,
                            returns,
                            execution_kind: abi_method.execution_kind(),
                            has_out_param,
                            doc: method_def.doc.clone(),
                        }
                    })
                    .collect();

                SwiftCallback {
                    protocol_name: protocol_name.clone(),
                    wrapper_class: format!("{}Wrapper", protocol_name),
                    vtable_var,
                    vtable_type: plan.vtable_type.as_str().to_string(),
                    bridge_name: format!("{}Bridge", protocol_name),
                    register_fn: plan.register_fn.as_str().to_string(),
                    create_fn: plan.create_fn.as_str().to_string(),
                    supports_foreign_wrap: self.callback_is_returned(&def.id),
                    methods,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }

    fn lower_callback_param(&self, def: &ParamDef, param: &AbiParam) -> SwiftCallbackParam {
        let label = camel_case(param.name.as_str());
        let (
            swift_type,
            ffi_args,
            proxy_ffi_args,
            proxy_wrapper_code,
            proxy_closure_wrap_open,
            proxy_closure_wrap_close,
            decode_prelude,
        ) = match &param.role {
            ParamRole::Input {
                transport: Transport::Scalar(ScalarOrigin::CStyleEnum { .. }),
                ..
            } => {
                let raw_label = format!("raw{}", pascal_case(param.name.as_str()));
                let swift_type = self.swift_type(&def.type_expr);
                (
                    swift_type.clone(),
                    vec![raw_label.clone()],
                    vec![format!("{}.rawValue", label)],
                    None,
                    None,
                    None,
                    Some(format!(
                        "let {} = {}(rawValue: {})!",
                        label, swift_type, raw_label
                    )),
                )
            }
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => (
                self.swift_type(&def.type_expr),
                vec![label.clone()],
                vec![label.clone()],
                None,
                None,
                None,
                None,
            ),
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Encoded(_)),
                encode_ops: Some(encode_ops),
                decode_ops: Some(decode_ops),
                ..
            } => {
                let len_name = format!("{}Len", label);
                let reader_decode = emit::emit_reader_read(decode_ops);
                let writer_encode = emit::emit_writer_write(encode_ops);
                (
                    self.swift_type(&def.type_expr),
                    vec![label.clone(), len_name.clone()],
                    vec![
                        format!("{}Buf.baseAddress!", label),
                        format!("UInt({}Buf.count)", label),
                    ],
                    Some(format!(
                        "let {label}Bytes = boltffiEncode {{ writer in {writer_encode} }}",
                    )),
                    Some(format!(
                        "{label}Bytes.withUnsafeBufferPointer {{ {label}Buf in",
                    )),
                    Some("}".to_string()),
                    Some(format!(
                        "let {} = {{ var reader = WireReader(ptr: {}!, len: Int({})); return {} }}()",
                        label, label, len_name, reader_decode
                    )),
                )
            }
            _ => unreachable!(
                "unsupported ABI param role for Swift callback: {:?}",
                param.role
            ),
        };

        SwiftCallbackParam {
            label: label.clone(),
            swift_type,
            call_arg: label,
            ffi_args,
            proxy_ffi_args,
            proxy_wrapper_code,
            proxy_closure_wrap_open,
            proxy_closure_wrap_close,
            decode_prelude,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_functions(&self) -> Vec<SwiftFunction> {
        self.contract
            .functions
            .iter()
            .map(|def| {
                let call = self.abi_call(&CallId::Function(def.id.clone()));
                let mode = self.lower_call_mode(call, &def.returns);
                let returns = match &call.mode {
                    CallMode::Async(async_call) => {
                        self.lower_return_def_for_async(&async_call.error, &def.returns)
                    }
                    CallMode::Sync => {
                        self.swift_return_from_abi(&call.returns, &call.error, &def.returns)
                    }
                };

                SwiftFunction {
                    name: camel_case(def.id.as_str()),
                    mode,
                    params: def
                        .params
                        .iter()
                        .map(|p| self.lower_param(p, call))
                        .collect(),
                    returns,
                    doc: def.doc.clone(),
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Params
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn lower_param(&self, param: &ParamDef, call: &AbiCall) -> SwiftParam {
        let abi_param = self.abi_param_for_semantic(call, &param.name);
        let swift_name = camel_case(param.name.as_str());

        let (swift_type, conversion) = match &abi_param.role {
            ParamRole::Input {
                transport: Transport::Scalar(origin),
                ..
            } => match origin {
                ScalarOrigin::CStyleEnum { enum_id, .. } => {
                    let swift_enum = self.swift_name_for_enum(enum_id);
                    (swift_enum, SwiftConversion::CStyleEnumRawValue)
                }
                ScalarOrigin::Primitive(p) => (
                    self.abi_to_swift(&AbiType::from(*p)),
                    SwiftConversion::Direct,
                ),
            },
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Scalar(origin)),
                mutability,
                ..
            } => match origin {
                ScalarOrigin::Primitive(primitive) => {
                    let element_abi = AbiType::from(*primitive);
                    let element_type = self.abi_to_swift(&element_abi);
                    if element_abi == AbiType::U8 && *mutability == Mutability::Shared {
                        ("Data".to_string(), SwiftConversion::ToData)
                    } else {
                        let conversion = match mutability {
                            Mutability::Mutable => SwiftConversion::MutableBuffer {
                                element_type: element_type.clone(),
                            },
                            Mutability::Shared => SwiftConversion::PrimitiveBuffer {
                                element_type: element_type.clone(),
                            },
                        };
                        (format!("[{}]", element_type), conversion)
                    }
                }
                ScalarOrigin::CStyleEnum { .. } => {
                    unreachable!("c-style enum buffers must be wire-encoded")
                }
            },
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Utf8),
                ..
            } => ("String".to_string(), SwiftConversion::ToString),
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Composite(layout)),
                ..
            } => {
                let c_type = format!("___{}", layout.record_id.as_str());
                let fields = self.composite_field_mappings(layout);
                (
                    format!("[{}]", self.swift_name_for_record(&layout.record_id)),
                    SwiftConversion::ToCompositeBuffer {
                        c_type,
                        fields,
                        length_unit: abi_param
                            .span_length_unit()
                            .expect("composite span input must carry span length unit"),
                    },
                )
            }
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Encoded(codec)),
                encode_ops: Some(encode_ops),
                ..
            } => (
                self.swift_type_from_codec(codec),
                SwiftConversion::ToWireBuffer {
                    encode: encode_ops.clone(),
                },
            ),
            ParamRole::Input {
                transport: Transport::Span(_),
                mutability: Mutability::Mutable,
                ..
            } => {
                let element_type = self.buffer_element_swift_type(&param.type_expr);
                (
                    format!("[{}]", element_type),
                    SwiftConversion::MutableBuffer {
                        element_type: element_type.clone(),
                    },
                )
            }
            ParamRole::Input {
                transport: Transport::Handle { class_id, nullable },
                ..
            } => {
                let class_name = self.swift_name_for_class(class_id);
                let swift_type = if *nullable {
                    format!("{}?", class_name)
                } else {
                    class_name.clone()
                };
                (
                    swift_type,
                    SwiftConversion::PassHandle {
                        class_name,
                        nullable: *nullable,
                    },
                )
            }
            ParamRole::Input {
                transport:
                    Transport::Callback {
                        callback_id,
                        nullable,
                        style,
                    },
                ..
            } => match style {
                CallbackStyle::BoxedDyn => {
                    let protocol = pascal_case(callback_id.as_str());
                    let swift_type = if *nullable {
                        format!("(any {})?", protocol)
                    } else {
                        format!("any {}", protocol)
                    };
                    (
                        swift_type,
                        SwiftConversion::WrapCallback {
                            protocol,
                            nullable: *nullable,
                        },
                    )
                }
                CallbackStyle::ImplTrait => {
                    let closure_plan = self.build_closure_trampoline(callback_id, &swift_name);
                    let swift_type = format!("@escaping {}", closure_plan.swift_type);
                    (
                        swift_type,
                        SwiftConversion::InlineClosure {
                            closure: Box::new(closure_plan),
                        },
                    )
                }
            },
            ParamRole::Input {
                transport: Transport::Composite(layout),
                ..
            } => {
                let c_type = format!("___{}", layout.record_id.as_str());
                let fields = self.composite_field_mappings(layout);
                (
                    self.swift_name_for_record(&layout.record_id),
                    SwiftConversion::ToComposite { c_type, fields },
                )
            }
            _ => unreachable!("unsupported param role for Swift: {:?}", abi_param.role),
        };

        SwiftParam {
            label: None,
            name: swift_name,
            swift_type,
            conversion,
        }
    }

    fn swift_type_from_codec(&self, codec: &CodecPlan) -> String {
        match codec {
            CodecPlan::Record { id, .. } => self.swift_name_for_record(id),
            CodecPlan::Enum { id, .. } => self.swift_name_for_enum(id),
            CodecPlan::Vec { element, .. } => format!("[{}]", self.swift_type_from_codec(element)),
            CodecPlan::Option(inner) => format!("{}?", self.swift_type_from_codec(inner)),
            CodecPlan::Result { ok, err } => format!(
                "Result<{}, {}>",
                self.swift_type_from_codec(ok),
                self.swift_codec_result_error_type(err)
            ),
            CodecPlan::String => "String".to_string(),
            CodecPlan::Primitive(p) => self.abi_to_swift(&AbiType::from(*p)),
            CodecPlan::Void => "Void".to_string(),
            CodecPlan::Builtin(id) => emit::swift_builtin(id),
            CodecPlan::Custom { id, .. } => self.swift_named_custom_type(id.as_str()),
        }
    }

    fn swift_codec_result_error_type(&self, codec: &CodecPlan) -> String {
        match codec {
            CodecPlan::String => "FfiError".to_string(),
            _ => self.swift_type_from_codec(codec),
        }
    }

    fn abi_param_for_semantic<'b>(&self, call: &'b AbiCall, name: &ParamName) -> &'b AbiParam {
        call.params
            .iter()
            .find(|param| param.name.as_str() == name.as_str() && param.transport().is_some())
            .expect("ABI param should exist")
    }

    fn composite_field_mappings(&self, layout: &CompositeLayout) -> Vec<CompositeFieldMapping> {
        layout
            .fields
            .iter()
            .map(|field| CompositeFieldMapping {
                swift_name: camel_case(field.name.as_str()),
                c_name: field.name.as_str().to_string(),
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Returns
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn swift_return_from_abi(
        &self,
        return_shape: &ReturnShape,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftReturn {
        self.swift_return_from_abi_with_context(
            return_shape,
            error,
            returns,
            ReturnInvocationContext::SyncExport,
        )
    }

    fn swift_return_from_abi_with_context(
        &self,
        return_shape: &ReturnShape,
        error: &ErrorTransport,
        returns: &ReturnDef,
        context: ReturnInvocationContext,
    ) -> SwiftReturn {
        let strategy = return_shape.value_return_strategy();
        let method =
            strategy.return_method(error.return_strategy(), context, ReturnPlatform::Native);

        let base = match strategy {
            ValueReturnStrategy::Void => SwiftReturn::Void,
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue) => {
                SwiftReturn::Direct {
                    swift_type: self.swift_return_value_type(returns),
                }
            }
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag) => {
                let Some(Transport::Scalar(ScalarOrigin::CStyleEnum { enum_id, .. })) =
                    &return_shape.transport
                else {
                    unreachable!("c-style enum return strategy requires scalar enum transport");
                };
                SwiftReturn::CStyleEnumFromRawValue {
                    swift_type: self.swift_name_for_enum(enum_id),
                }
            }
            _ if matches!(method, ValueReturnMethod::WriteToOutBufferParts) => {
                self.wire_buffer_return(return_shape, returns)
            }
            ValueReturnStrategy::CompositeValue => {
                let Some(Transport::Composite(layout)) = &return_shape.transport else {
                    unreachable!("composite return strategy requires composite transport");
                };
                let c_type = format!("___{}", layout.record_id.as_str());
                let fields = self.composite_field_mappings(layout);
                SwiftReturn::FromComposite {
                    swift_type: self.swift_return_value_type(returns),
                    c_type,
                    fields,
                }
            }
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec) => match &return_shape
                .transport
            {
                Some(Transport::Span(SpanContent::Scalar(origin))) => {
                    let primitive = origin.primitive();
                    let element_swift_type = self.abi_to_swift(&AbiType::from(primitive));
                    let enum_mapping = match origin {
                        ScalarOrigin::CStyleEnum { enum_id, .. } => {
                            Some(self.swift_name_for_enum(enum_id))
                        }
                        ScalarOrigin::Primitive(_) => None,
                    };
                    SwiftReturn::FromDirectBuffer {
                        swift_type: self.swift_return_value_type(returns),
                        element_swift_type,
                        composite_mapping: None,
                        enum_mapping,
                    }
                }
                Some(Transport::Span(SpanContent::Composite(layout))) => {
                    let c_struct = format!("___{}", layout.record_id.as_str());
                    let swift_record = self.swift_name_for_record(&layout.record_id);
                    let fields = self.composite_field_mappings(layout);
                    SwiftReturn::FromDirectBuffer {
                        swift_type: self.swift_return_value_type(returns),
                        element_swift_type: c_struct,
                        composite_mapping: Some(DirectBufferCompositeMapping {
                            swift_record_type: swift_record,
                            fields,
                        }),
                        enum_mapping: None,
                    }
                }
                _ => unreachable!("direct buffer return strategy requires direct span transport"),
            },
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::ResultScalar)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded) => {
                SwiftReturn::FromWireBuffer {
                    swift_type: self.swift_return_value_type(returns),
                    decode: return_shape.decode_ops.clone().unwrap_or_else(|| ReadSeq {
                        size: SizeExpr::Fixed(0),
                        ops: vec![],
                        shape: WireShape::Value,
                    }),
                    encode: return_shape.encode_ops.clone().unwrap_or_else(|| WriteSeq {
                        size: SizeExpr::Fixed(0),
                        ops: vec![],
                        shape: WireShape::Value,
                    }),
                }
            }
            ValueReturnStrategy::ObjectHandle => {
                let Some(Transport::Handle { class_id, nullable }) = &return_shape.transport else {
                    unreachable!("object handle return strategy requires handle transport");
                };
                let class_name = self.swift_name_for_class(class_id);
                SwiftReturn::Handle {
                    class_name,
                    nullable: *nullable,
                }
            }
            ValueReturnStrategy::CallbackHandle => {
                let Some(Transport::Callback {
                    callback_id,
                    nullable,
                    ..
                }) = &return_shape.transport
                else {
                    unreachable!("callback handle return strategy requires callback transport");
                };
                SwiftReturn::Callback {
                    protocol_name: pascal_case(callback_id.as_str()),
                    nullable: *nullable,
                }
            }
        };

        match error.return_strategy() {
            ErrorReturnStrategy::None => base,
            ErrorReturnStrategy::Encoded => {
                let ErrorTransport::Encoded {
                    decode_ops,
                    encode_ops,
                } = error
                else {
                    unreachable!("encoded error strategy requires encoded error transport");
                };
                let result_decode = return_shape.decode_ops.clone().unwrap_or_else(|| ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                });
                let ok_variant = match returns {
                    ReturnDef::Result {
                        ok: TypeExpr::Void, ..
                    } => SwiftReturn::Void,
                    _ if self.is_c_style_enum_return(returns) => {
                        SwiftReturn::CStyleEnumFromRawValue {
                            swift_type: self.swift_return_value_type(returns),
                        }
                    }
                    _ => base,
                };
                SwiftReturn::Throws {
                    ok: Box::new(ok_variant),
                    err_type: self.swift_error_type(returns),
                    result_decode,
                    err_decode: decode_ops.clone(),
                    err_is_string: self.error_is_string(returns),
                    err_encode: encode_ops.clone(),
                }
            }
            ErrorReturnStrategy::StatusCode => SwiftReturn::Throws {
                ok: Box::new(base),
                err_type: "FfiError".to_string(),
                result_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_is_string: false,
                err_encode: None,
            },
        }
    }

    fn wire_buffer_return(&self, return_shape: &ReturnShape, returns: &ReturnDef) -> SwiftReturn {
        SwiftReturn::FromWireBuffer {
            swift_type: self.swift_return_value_type(returns),
            decode: return_shape.decode_ops.clone().unwrap_or_else(|| ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            }),
            encode: return_shape.encode_ops.clone().unwrap_or_else(|| WriteSeq {
                size: SizeExpr::Fixed(0),
                ops: vec![],
                shape: WireShape::Value,
            }),
        }
    }

    fn callback_is_returned(&self, callback_id: &CallbackId) -> bool {
        self.abi.calls.iter().any(|call| {
            self.return_shape_references_callback(&call.returns, callback_id)
                || match &call.mode {
                    CallMode::Sync => false,
                    CallMode::Async(async_call) => {
                        self.return_shape_references_callback(&async_call.result, callback_id)
                    }
                }
        })
    }

    fn return_shape_references_callback(
        &self,
        return_shape: &ReturnShape,
        callback_id: &CallbackId,
    ) -> bool {
        matches!(
            &return_shape.transport,
            Some(Transport::Callback {
                callback_id: returned_callback,
                ..
            }) if returned_callback == callback_id
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type Helpers
// ─────────────────────────────────────────────────────────────────────────────

impl<'a> SwiftLowerer<'a> {
    fn abi_call(&self, id: &CallId) -> &AbiCall {
        self.abi_index.call(self.abi, id)
    }

    fn swift_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Handle(id) => self.swift_name_for_class(id),
            TypeExpr::Callback(id) => format!("any {}", pascal_case(id.as_str())),
            TypeExpr::Custom(id) => self.swift_named_custom_type(id.as_str()),
            TypeExpr::Option(inner) => match inner.as_ref() {
                TypeExpr::Handle(id) => format!("{}?", self.swift_name_for_class(id)),
                TypeExpr::Callback(id) => format!("(any {})?", pascal_case(id.as_str())),
                TypeExpr::Custom(id) => format!("{}?", self.swift_named_custom_type(id.as_str())),
                _ => self.resolve_swift_type(ty),
            },
            TypeExpr::Vec(inner) => {
                if matches!(inner.as_ref(), TypeExpr::Primitive(PrimitiveType::U8)) {
                    "Data".to_string()
                } else {
                    format!("[{}]", self.swift_type(inner))
                }
            }
            TypeExpr::Result { ok, err } => self.swift_result_type(ok, err),
            _ => emit::swift_type(ty),
        }
    }

    fn swift_return_value_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(ty) => self.swift_type(ty),
            ReturnDef::Result { ok, .. } => self.swift_type(ok),
        }
    }

    fn is_c_style_enum_return(&self, returns: &ReturnDef) -> bool {
        let enum_id = match returns {
            ReturnDef::Value(TypeExpr::Enum(id))
            | ReturnDef::Result {
                ok: TypeExpr::Enum(id),
                ..
            } => id,
            _ => return false,
        };
        self.contract
            .catalog
            .resolve_enum(enum_id)
            .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
            .unwrap_or(false)
    }

    fn is_c_style_enum_type_expr(&self, type_expr: &TypeExpr) -> bool {
        let TypeExpr::Enum(enum_id) = type_expr else {
            return false;
        };
        self.contract
            .catalog
            .resolve_enum(enum_id)
            .map(|enumeration| matches!(enumeration.repr, EnumRepr::CStyle { .. }))
            .unwrap_or(false)
    }

    fn swift_error_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Result { err, .. } => self.swift_type(err),
            _ => "FfiError".to_string(),
        }
    }

    fn error_is_string(&self, returns: &ReturnDef) -> bool {
        matches!(returns, ReturnDef::Result { err, .. } if matches!(err, TypeExpr::String))
    }

    fn record_decode_fields(&self, record: &AbiRecord) -> HashMap<FieldName, ReadSeq> {
        record
            .decode_ops
            .ops
            .iter()
            .find_map(|op| match op {
                ReadOp::Record { fields, .. } => Some(fields),
                _ => None,
            })
            .into_iter()
            .flat_map(|fields| {
                fields
                    .iter()
                    .map(|field| (field.name.clone(), field.seq.clone()))
            })
            .collect()
    }

    fn record_encode_fields(&self, record: &AbiRecord) -> HashMap<FieldName, WriteSeq> {
        record
            .encode_ops
            .ops
            .iter()
            .find_map(|op| match op {
                WriteOp::Record { fields, .. } => Some(fields),
                _ => None,
            })
            .into_iter()
            .flat_map(|fields| {
                fields
                    .iter()
                    .map(|field| (field.name.clone(), field.seq.clone()))
            })
            .collect()
    }

    fn record_field_offset(&self, seq: &ReadSeq) -> Option<usize> {
        seq.ops.first().and_then(|op| match op {
            ReadOp::Primitive { offset, .. } => match offset {
                OffsetExpr::Fixed(value) => Some(*value),
                OffsetExpr::Base => Some(0),
                OffsetExpr::BasePlus(add) => Some(*add),
                OffsetExpr::Var(_) | OffsetExpr::VarPlus(_, _) => None,
            },
            _ => None,
        })
    }

    fn rebase_read_seq(&self, seq: &ReadSeq, old_base: &str, new_base: &str) -> ReadSeq {
        ReadSeq {
            size: seq.size.clone(),
            ops: seq
                .ops
                .iter()
                .map(|op| self.rebase_read_op(op, old_base, new_base))
                .collect(),
            shape: seq.shape,
        }
    }

    fn rebase_read_op(&self, op: &ReadOp, old_base: &str, new_base: &str) -> ReadOp {
        match op {
            ReadOp::Primitive { primitive, offset } => ReadOp::Primitive {
                primitive: *primitive,
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::String { offset } => ReadOp::String {
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                some: Box::new(self.rebase_read_seq(some, old_base, new_base)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: self.rebase_offset_expr(len_offset, old_base, new_base),
                element_type: element_type.clone(),
                element: Box::new(self.rebase_read_seq(element, old_base, new_base)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                fields: fields
                    .iter()
                    .map(|field| {
                        let seq = self.rebase_read_seq(&field.seq, old_base, new_base);
                        FieldReadOp {
                            name: field.name.clone(),
                            seq,
                        }
                    })
                    .collect(),
            },
            ReadOp::Enum { id, offset, layout } => ReadOp::Enum {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
                layout: layout.clone(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: self.rebase_offset_expr(tag_offset, old_base, new_base),
                ok: Box::new(self.rebase_read_seq(ok, old_base, new_base)),
                err: Box::new(self.rebase_read_seq(err, old_base, new_base)),
            },
            ReadOp::Builtin { id, offset } => ReadOp::Builtin {
                id: id.clone(),
                offset: self.rebase_offset_expr(offset, old_base, new_base),
            },
            ReadOp::Custom { id, underlying } => ReadOp::Custom {
                id: id.clone(),
                underlying: Box::new(self.rebase_read_seq(underlying, old_base, new_base)),
            },
        }
    }

    fn rebase_offset_expr(
        &self,
        offset: &OffsetExpr,
        old_base: &str,
        new_base: &str,
    ) -> OffsetExpr {
        match offset {
            OffsetExpr::Fixed(value) => OffsetExpr::Fixed(*value),
            OffsetExpr::Base => OffsetExpr::Base,
            OffsetExpr::BasePlus(add) => OffsetExpr::BasePlus(*add),
            OffsetExpr::Var(name) => {
                if name == old_base {
                    OffsetExpr::Var(new_base.to_string())
                } else {
                    OffsetExpr::Var(name.clone())
                }
            }
            OffsetExpr::VarPlus(name, add) => {
                if name == old_base {
                    OffsetExpr::VarPlus(new_base.to_string(), *add)
                } else {
                    OffsetExpr::VarPlus(name.clone(), *add)
                }
            }
        }
    }

    fn rebase_return_encode(returns: SwiftReturn, new_base: &str) -> SwiftReturn {
        match returns {
            SwiftReturn::FromWireBuffer {
                swift_type,
                decode,
                encode,
            } => SwiftReturn::FromWireBuffer {
                swift_type,
                decode,
                encode: remap_root_in_seq(&encode, ValueExpr::Var(new_base.to_string())),
            },
            SwiftReturn::Throws {
                ok,
                err_type,
                result_decode,
                err_decode,
                err_is_string,
                err_encode,
            } => SwiftReturn::Throws {
                ok: Box::new(Self::rebase_return_encode(*ok, new_base)),
                err_type,
                result_decode,
                err_decode,
                err_is_string,
                err_encode: err_encode
                    .map(|seq| remap_root_in_seq(&seq, ValueExpr::Var("error".to_string()))),
            },
            other => other,
        }
    }

    fn buffer_element_swift_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Vec(inner) => self.swift_type(inner),
            _ => "UInt8".to_string(),
        }
    }

    fn build_closure_trampoline(
        &self,
        callback_id: &CallbackId,
        param_name: &str,
    ) -> SwiftClosureTrampoline {
        let callback_def = self
            .contract
            .catalog
            .resolve_callback(callback_id)
            .expect("closure callback should exist");
        let method = &callback_def.methods[0];
        let abi_callback = self.abi_index.callback(self.abi, callback_id);
        let abi_method = abi_callback
            .methods
            .iter()
            .find(|m| m.id == method.id)
            .expect("closure callback method");
        let param_types: Vec<String> = method
            .params
            .iter()
            .map(|p| self.swift_type(&p.type_expr))
            .collect();
        let return_type = match &method.returns {
            ReturnDef::Void => "Void".to_string(),
            ReturnDef::Value(ty) => self.swift_type(ty),
            ReturnDef::Result { ok, err } => self.swift_result_type(ok, err),
        };

        let swift_type = if param_types.is_empty() {
            format!("() -> {}", return_type)
        } else {
            format!("({}) -> {}", param_types.join(", "), return_type)
        };

        let upper_name = pascal_case(param_name);
        let type_alias = format!("{}CallbackFn", upper_name);
        let box_class = format!("{}CallbackBox", upper_name);

        let abi_params: Vec<&AbiParam> = abi_method
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.role,
                    ParamRole::Input {
                        transport: Transport::Scalar(_) | Transport::Span(SpanContent::Encoded(_)),
                        ..
                    }
                )
            })
            .collect();
        let trampoline_params: Vec<SwiftClosureTrampolineParam> = method
            .params
            .iter()
            .zip(abi_params.iter())
            .enumerate()
            .map(|(idx, (param_def, abi_param))| {
                self.build_closure_trampoline_param(idx, param_def, abi_param)
            })
            .collect();
        let returns = self.inline_closure_swift_return(abi_method, method);
        let (c_return_type, value_return_strategy) =
            self.inline_closure_return_signature(&method.returns);

        SwiftClosureTrampoline {
            type_alias,
            swift_type,
            box_class,
            box_var: format!("{}Box", param_name),
            ptr_var: format!("{}Ptr", param_name),
            trampoline_var: format!("{}Trampoline", param_name),
            param_name: param_name.to_string(),
            trampoline_params,
            c_return_type,
            value_return_strategy,
            returns,
        }
    }

    fn inline_closure_return_signature(
        &self,
        returns: &ReturnDef,
    ) -> (String, ValueReturnStrategy) {
        match returns {
            ReturnDef::Void => ("Void".to_string(), ValueReturnStrategy::Void),
            ReturnDef::Result { .. } => (
                "FfiBuf_u8".to_string(),
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
            ),
            ReturnDef::Value(type_expr) => self.inline_closure_value_return_signature(type_expr),
        }
    }

    fn inline_closure_swift_return(
        &self,
        abi_method: &AbiCallbackMethod,
        method: &CallbackMethodDef,
    ) -> SwiftReturn {
        match &method.returns {
            ReturnDef::Void => SwiftReturn::Void,
            ReturnDef::Value(TypeExpr::Primitive(_)) => SwiftReturn::Direct {
                swift_type: self.swift_return_value_type(&method.returns),
            },
            ReturnDef::Value(TypeExpr::Enum(enum_id)) => self
                .contract
                .catalog
                .resolve_enum(enum_id)
                .and_then(|enum_def| match enum_def.repr {
                    EnumRepr::CStyle { .. } => Some(SwiftReturn::CStyleEnumFromRawValue {
                        swift_type: self.swift_name_for_enum(enum_id),
                    }),
                    EnumRepr::Data { .. } => None,
                })
                .unwrap_or_else(|| {
                    Self::rebase_return_encode(
                        self.swift_return_from_abi(
                            &abi_method.returns,
                            &abi_method.error,
                            &method.returns,
                        ),
                        "result",
                    )
                }),
            ReturnDef::Value(TypeExpr::Record(record_id)) => self
                .abi
                .records
                .iter()
                .find(|record| record.id.as_str() == record_id.as_str() && record.is_blittable)
                .map(|_| SwiftReturn::FromComposite {
                    swift_type: self.swift_name_for_record(record_id),
                    c_type: format!("___{}", record_id.as_str()),
                    fields: self.record_field_mappings(record_id),
                })
                .unwrap_or_else(|| {
                    Self::rebase_return_encode(
                        self.swift_return_from_abi(
                            &abi_method.returns,
                            &abi_method.error,
                            &method.returns,
                        ),
                        "result",
                    )
                }),
            _ => Self::rebase_return_encode(
                self.swift_return_from_abi(&abi_method.returns, &abi_method.error, &method.returns),
                "result",
            ),
        }
    }

    fn inline_closure_value_return_signature(
        &self,
        type_expr: &TypeExpr,
    ) -> (String, ValueReturnStrategy) {
        match type_expr {
            TypeExpr::Primitive(primitive) => (
                self.abi_to_swift(&AbiType::from(*primitive)),
                ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue),
            ),
            TypeExpr::Enum(enum_id) => self
                .contract
                .catalog
                .resolve_enum(enum_id)
                .and_then(|enum_def| match &enum_def.repr {
                    EnumRepr::CStyle { tag_type, .. } => Some((
                        self.abi_to_swift(&AbiType::from(*tag_type)),
                        ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag),
                    )),
                    EnumRepr::Data { .. } => None,
                })
                .unwrap_or_else(|| {
                    (
                        "FfiBuf_u8".to_string(),
                        ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
                    )
                }),
            TypeExpr::Record(record_id) => self
                .abi
                .records
                .iter()
                .find(|record| record.id == *record_id && record.is_blittable)
                .map(|_| {
                    (
                        format!("___{}", record_id.as_str()),
                        ValueReturnStrategy::CompositeValue,
                    )
                })
                .unwrap_or_else(|| {
                    (
                        "FfiBuf_u8".to_string(),
                        ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
                    )
                }),
            _ => (
                "FfiBuf_u8".to_string(),
                ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
            ),
        }
    }

    fn record_field_mappings(&self, record_id: &RecordId) -> Vec<CompositeFieldMapping> {
        self.contract
            .catalog
            .resolve_record(record_id)
            .map(|record| {
                record
                    .fields
                    .iter()
                    .map(|field| CompositeFieldMapping {
                        swift_name: camel_case(field.name.as_str()),
                        c_name: field.name.as_str().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn build_closure_trampoline_param(
        &self,
        idx: usize,
        param_def: &ParamDef,
        abi_param: &AbiParam,
    ) -> SwiftClosureTrampolineParam {
        match &abi_param.role {
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Encoded(_)),
                decode_ops: Some(decode_ops),
                ..
            } => {
                let ptr_name = format!("ptr{}", idx);
                let len_name = format!("len{}", idx);
                let reader_decode = emit::emit_reader_read(decode_ops);
                let decode_expr = format!(
                    "{{ var reader = WireReader(ptr: {}!, len: Int({})); return {} }}()",
                    ptr_name, len_name, reader_decode
                );
                SwiftClosureTrampolineParam {
                    name: format!("{}, {}", ptr_name, len_name),
                    c_type: "UnsafePointer<UInt8>?, UInt".to_string(),
                    decode_expr,
                }
            }
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => {
                let arg_name = format!("arg{}", idx);
                let decode_expr = if self.is_c_style_enum_type_expr(&param_def.type_expr) {
                    format!(
                        "{}(rawValue: {})!",
                        self.swift_type(&param_def.type_expr),
                        arg_name
                    )
                } else {
                    arg_name.clone()
                };
                SwiftClosureTrampolineParam {
                    name: arg_name.clone(),
                    c_type: self.abi_to_swift(&abi_param.abi_type),
                    decode_expr,
                }
            }
            _ => unreachable!(
                "unsupported closure param role for {}",
                param_def.name.as_str()
            ),
        }
    }

    fn abi_to_swift(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Void => "Void".to_string(),
            AbiType::Bool => "Bool".to_string(),
            AbiType::I8 => "Int8".to_string(),
            AbiType::U8 => "UInt8".to_string(),
            AbiType::I16 => "Int16".to_string(),
            AbiType::U16 => "UInt16".to_string(),
            AbiType::I32 => "Int32".to_string(),
            AbiType::U32 => "UInt32".to_string(),
            AbiType::I64 => "Int64".to_string(),
            AbiType::U64 => "UInt64".to_string(),
            AbiType::ISize => "Int".to_string(),
            AbiType::USize => "UInt".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer(_)
            | AbiType::OwnedBuffer
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_) => "OpaquePointer".to_string(),
            AbiType::CallbackHandle => "BoltFFICallbackHandle".to_string(),
            AbiType::Struct(id) => format!("___{}", id.as_str()),
        }
    }

    fn swift_name_for_record(&self, id: &RecordId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_enum(&self, id: &EnumId) -> String {
        pascal_case(id.as_str())
    }

    fn swift_name_for_class(&self, id: &ClassId) -> String {
        pascal_case(id.as_str())
    }

    fn lower_call_mode(&self, call: &AbiCall, returns: &ReturnDef) -> SwiftCallMode {
        match &call.mode {
            CallMode::Sync => SwiftCallMode::Sync {
                symbol: call.symbol.as_str().to_string(),
            },
            CallMode::Async(async_call) => SwiftCallMode::Async {
                start: call.symbol.as_str().to_string(),
                poll: async_call.poll.as_str().to_string(),
                complete: async_call.complete.as_str().to_string(),
                cancel: async_call.cancel.as_str().to_string(),
                free: async_call.free.as_str().to_string(),
                result: Box::new(self.lower_async_result(
                    &async_call.result,
                    &async_call.error,
                    returns,
                )),
            },
        }
    }

    fn lower_async_result(
        &self,
        result_shape: &ReturnShape,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftAsyncResult {
        let returns_is_result = matches!(returns, ReturnDef::Result { .. });
        let throws = returns_is_result || matches!(error, ErrorTransport::Encoded { .. });

        match &result_shape.transport {
            None => SwiftAsyncResult::Void,
            Some(Transport::Scalar(origin)) => SwiftAsyncResult::Direct {
                swift_type: self.abi_to_swift(&AbiType::from(origin.primitive())),
                conversion: SwiftAsyncConversion::None,
            },
            Some(Transport::Span(SpanContent::Scalar(origin))) => {
                let primitive = origin.primitive();
                let element_swift_type = self.abi_to_swift(&AbiType::from(primitive));
                let enum_mapping = match origin {
                    ScalarOrigin::CStyleEnum { enum_id, .. } => {
                        Some(self.swift_name_for_enum(enum_id))
                    }
                    ScalarOrigin::Primitive(_) => None,
                };
                SwiftAsyncResult::DirectBuffer {
                    swift_type: self.swift_return_value_type(returns),
                    element_swift_type,
                    composite_mapping: None,
                    enum_mapping,
                }
            }
            Some(Transport::Span(SpanContent::Composite(layout))) => {
                let c_struct = format!("___{}", layout.record_id.as_str());
                let swift_record = self.swift_name_for_record(&layout.record_id);
                let fields = self.composite_field_mappings(layout);
                SwiftAsyncResult::DirectBuffer {
                    swift_type: self.swift_return_value_type(returns),
                    element_swift_type: c_struct,
                    composite_mapping: Some(DirectBufferCompositeMapping {
                        swift_record_type: swift_record,
                        fields,
                    }),
                    enum_mapping: None,
                }
            }
            Some(Transport::Span(_)) => {
                let decode_ops = result_shape.decode_ops.clone().unwrap_or_else(|| ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                });
                self.encoded_async_result(decode_ops, throws, error, returns)
            }
            Some(Transport::Composite(layout)) => SwiftAsyncResult::Direct {
                swift_type: self.swift_return_value_type(returns),
                conversion: SwiftAsyncConversion::Composite(DirectBufferCompositeMapping {
                    swift_record_type: self.swift_name_for_record(&layout.record_id),
                    fields: self.composite_field_mappings(layout),
                }),
            },
            Some(Transport::Handle { class_id, nullable }) => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("{}?", self.swift_name_for_class(class_id))
                } else {
                    self.swift_name_for_class(class_id)
                },
                conversion: SwiftAsyncConversion::Handle {
                    class_name: self.swift_name_for_class(class_id),
                    nullable: *nullable,
                },
            },
            Some(Transport::Callback {
                callback_id,
                nullable,
                ..
            }) => SwiftAsyncResult::Direct {
                swift_type: if *nullable {
                    format!("(any {})?", pascal_case(callback_id.as_str()))
                } else {
                    format!("any {}", pascal_case(callback_id.as_str()))
                },
                conversion: SwiftAsyncConversion::Callback {
                    protocol: pascal_case(callback_id.as_str()),
                    nullable: *nullable,
                },
            },
        }
    }

    fn encoded_async_result(
        &self,
        decode_ops: ReadSeq,
        throws: bool,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftAsyncResult {
        let ok_type = if throws {
            match returns {
                ReturnDef::Result { ok, .. } => Some(self.swift_type(ok)),
                _ => None,
            }
        } else {
            None
        };
        let (swift_type, err_is_string) = match returns {
            ReturnDef::Result { ok, err } => (self.swift_type(ok), matches!(err, TypeExpr::String)),
            ReturnDef::Value(ty) => (self.swift_type(ty), false),
            ReturnDef::Void => ("Void".to_string(), false),
        };
        let err_decode = match error {
            ErrorTransport::Encoded { decode_ops, .. } => decode_ops.clone(),
            ErrorTransport::None | ErrorTransport::StatusCode => {
                self.error_decode_from_result_read(&decode_ops)
            }
        };
        SwiftAsyncResult::Encoded {
            swift_type,
            ok_type,
            decode: decode_ops,
            throws,
            err_decode,
            err_is_string,
        }
    }

    fn error_decode_from_result_read(&self, decode_ops: &ReadSeq) -> ReadSeq {
        match decode_ops.ops.first() {
            Some(ReadOp::Result { err, .. }) => err.as_ref().clone(),
            _ => ReadSeq {
                size: SizeExpr::Fixed(0),
                ops: Vec::new(),
                shape: WireShape::Value,
            },
        }
    }

    fn lower_return_def_for_async(
        &self,
        error: &ErrorTransport,
        returns: &ReturnDef,
    ) -> SwiftReturn {
        match error {
            ErrorTransport::None => SwiftReturn::Void,
            ErrorTransport::StatusCode => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: "FfiError".to_string(),
                result_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_decode: ReadSeq {
                    size: SizeExpr::Fixed(0),
                    ops: vec![],
                    shape: WireShape::Value,
                },
                err_is_string: false,
                err_encode: None,
            },
            ErrorTransport::Encoded {
                decode_ops,
                encode_ops,
            } => SwiftReturn::Throws {
                ok: Box::new(SwiftReturn::Void),
                err_type: self.swift_error_type(returns),
                result_decode: decode_ops.clone(),
                err_decode: decode_ops.clone(),
                err_is_string: self.error_is_string(returns),
                err_encode: encode_ops.clone(),
            },
        }
    }
}

fn swift_default_literal(default: &DefaultValue) -> String {
    match default {
        DefaultValue::Bool(true) => "true".to_string(),
        DefaultValue::Bool(false) => "false".to_string(),
        DefaultValue::Integer(v) => v.to_string(),
        DefaultValue::Float(v) => format!("{}", v),
        DefaultValue::String(v) => format!("\"{}\"", v),
        DefaultValue::EnumVariant { variant_name, .. } => {
            format!(".{}", variant_name.to_lower_camel_case())
        }
        DefaultValue::Null => "nil".to_string(),
    }
}

fn lower_first_char(name: &str) -> String {
    name.chars()
        .enumerate()
        .map(|(index, ch)| {
            if index == 0 {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef,
        DataVariant, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, ParamPassing,
        Receiver, RecordDef, ReturnDef, VariantPayload,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, VariantName,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};

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

    fn lower_contract(contract: &FfiContract) -> SwiftModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        SwiftLowerer::new(contract, &abi).lower()
    }

    #[test]
    fn blittable_record_is_detected() {
        let mut contract = empty_contract();
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

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            record.is_blittable,
            "Point should be blittable (primitives only)"
        );
        assert_eq!(record.blittable_size, Some(16));
    }

    #[test]
    fn non_blittable_record_with_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("User"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("id"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            !record.is_blittable,
            "User should NOT be blittable (has String)"
        );
        assert_eq!(record.blittable_size, None);
    }

    #[test]
    fn non_blittable_record_with_vec() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Scores"),
            fields: vec![FieldDef {
                name: FieldName::new("values"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.records.len(), 1);
        let record = &module.records[0];
        assert!(
            !record.is_blittable,
            "Scores should NOT be blittable (has Vec)"
        );
    }

    #[test]
    fn field_names_are_camel_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Config"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("max_connections"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("timeout_ms"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        let record = &module.records[0];
        assert_eq!(record.fields[0].swift_name, "maxConnections");
        assert_eq!(record.fields[1].swift_name, "timeoutMs");
    }

    #[test]
    fn primitive_types_map_correctly() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("AllPrimitives"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("a"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::Bool),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("b"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I8),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("c"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U8),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("d"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I16),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("e"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U16),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("f"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("g"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("h"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("i"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U64),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("j"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F32),
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("k"),
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

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "Bool");
        assert_eq!(record.fields[1].swift_type, "Int8");
        assert_eq!(record.fields[2].swift_type, "UInt8");
        assert_eq!(record.fields[3].swift_type, "Int16");
        assert_eq!(record.fields[4].swift_type, "UInt16");
        assert_eq!(record.fields[5].swift_type, "Int32");
        assert_eq!(record.fields[6].swift_type, "UInt32");
        assert_eq!(record.fields[7].swift_type, "Int64");
        assert_eq!(record.fields[8].swift_type, "UInt64");
        assert_eq!(record.fields[9].swift_type, "Float");
        assert_eq!(record.fields[10].swift_type, "Double");
    }

    #[test]
    fn c_style_enum_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    CStyleVariant {
                        name: VariantName::new("Active"),
                        discriminant: 0,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("Inactive"),
                        discriminant: 1,
                        doc: None,
                    },
                    CStyleVariant {
                        name: VariantName::new("Pending"),
                        discriminant: 2,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.name, "Status");
        assert!(e.is_c_style());
        assert_eq!(e.variants.len(), 3);
        assert_eq!(e.variants[0].swift_name, "active");
        assert_eq!(e.variants[1].swift_name, "inactive");
        assert_eq!(e.variants[2].swift_name, "pending");
    }

    #[test]
    fn data_enum_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    DataVariant {
                        name: VariantName::new("Int"),
                        discriminant: 0,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::I64,
                        )]),
                        doc: None,
                    },
                    DataVariant {
                        name: VariantName::new("Text"),
                        discriminant: 1,
                        payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.name, "Value");
        assert!(!e.is_c_style());
        assert_eq!(e.variants.len(), 2);
    }

    #[test]
    fn blittable_struct_has_c_offsets() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Aligned"),
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

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert!(record.is_blittable);
        assert_eq!(record.fields[0].c_offset, Some(0));
        assert_eq!(record.fields[1].c_offset, Some(4));
        assert_eq!(record.fields[2].c_offset, Some(8));
    }

    #[test]
    fn option_type_maps_to_swift_optional() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("MaybeValue"),
            fields: vec![FieldDef {
                name: FieldName::new("value"),
                type_expr: TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "Int32?");
    }

    #[test]
    fn vec_type_maps_to_swift_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Numbers"),
            fields: vec![FieldDef {
                name: FieldName::new("items"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].swift_type, "[Int32]");
    }

    #[test]
    fn nested_record_type_maps_correctly() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Inner"),
            fields: vec![FieldDef {
                name: FieldName::new("value"),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Outer"),
            fields: vec![FieldDef {
                name: FieldName::new("inner"),
                type_expr: TypeExpr::Record(RecordId::new("Inner")),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);

        let outer = module
            .records
            .iter()
            .find(|r| r.class_name == "Outer")
            .unwrap();
        assert_eq!(outer.fields[0].swift_type, "Inner");
    }

    #[test]
    fn callback_with_encoded_param_does_not_panic() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Logger"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("log"),
                params: vec![ParamDef {
                    name: ParamName::new("message"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Void,
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower_contract(&contract);

        assert_eq!(module.callbacks.len(), 1);
        let cb = &module.callbacks[0];
        assert_eq!(cb.protocol_name, "Logger");
        assert_eq!(cb.methods.len(), 1);
        assert_eq!(cb.methods[0].params.len(), 1);
    }

    #[test]
    fn result_unit_class_method_decodes_ok_as_void() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(ClassDef {
            id: ClassId::new("Counter"),
            constructors: vec![ConstructorDef::Default {
                params: vec![],
                is_fallible: false,
                is_optional: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![MethodDef {
                id: MethodId::new("try_reset_if_positive"),
                receiver: Receiver::RefSelf,
                params: vec![],
                returns: ReturnDef::Result {
                    ok: TypeExpr::Void,
                    err: TypeExpr::String,
                },
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let method = module.classes[0]
            .methods
            .iter()
            .find(|method| method.name == "tryResetIfPositive")
            .expect("result unit class method should lower");
        let decode_expr = method
            .returns
            .reader_decode_expr()
            .expect("result unit class method should decode from reader");

        assert!(decode_expr.contains("if tag == 0 { return () }"));
        assert!(!decode_expr.contains("return reader.readU8()"));
    }

    #[test]
    fn result_unit_async_function_decodes_ok_as_void() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("try_reset_async"),
            params: vec![],
            returns: ReturnDef::Result {
                ok: TypeExpr::Void,
                err: TypeExpr::String,
            },
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let SwiftCallMode::Async { result, .. } = &module.functions[0].mode else {
            panic!("result unit async function should lower as async");
        };
        let decode_expr = result
            .reader_decode_expr()
            .expect("result unit async function should decode from reader");

        assert!(decode_expr.contains("if tag == 0 { return () }"));
        assert!(!decode_expr.contains("return reader.readU8()"));
    }

    #[test]
    fn nullary_inline_closure_returns_primitive_value() {
        use crate::ir::definitions::FunctionDef;
        use crate::ir::ids::FunctionId;

        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("NullaryI32"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract.functions.push(FunctionDef {
            id: FunctionId::new("apply"),
            params: vec![ParamDef {
                name: ParamName::new("callback"),
                type_expr: TypeExpr::Callback(CallbackId::new("NullaryI32")),
                passing: ParamPassing::ImplTrait,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let closure_wrapper = module.functions[0].params[0]
            .wrapper_code()
            .expect("inline closure should render a wrapper");

        assert!(
            closure_wrapper.contains("@convention(c) (UnsafeMutableRawPointer?) -> Int32"),
            "nullary closure should not emit a trailing comma and should return Int32: {}",
            closure_wrapper
        );
        assert!(
            closure_wrapper.contains(
                "return Unmanaged<CallbackCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_()"
            ),
            "nullary closure should return the closure result directly: {}",
            closure_wrapper
        );
    }

    #[test]
    fn inline_string_closure_returns_owned_buffer() {
        use crate::ir::definitions::FunctionDef;
        use crate::ir::ids::FunctionId;

        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("StringMapper"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params: vec![ParamDef {
                    name: ParamName::new("value"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract.functions.push(FunctionDef {
            id: FunctionId::new("apply_string"),
            params: vec![
                ParamDef {
                    name: ParamName::new("callback"),
                    type_expr: TypeExpr::Callback(CallbackId::new("StringMapper")),
                    passing: ParamPassing::ImplTrait,
                    doc: None,
                },
                ParamDef {
                    name: ParamName::new("value"),
                    type_expr: TypeExpr::String,
                    passing: ParamPassing::Value,
                    doc: None,
                },
            ],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let closure_wrapper = module.functions[0].params[0]
            .wrapper_code()
            .expect("inline closure should render a wrapper");

        assert!(
            closure_wrapper.contains(
                "@convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt) -> FfiBuf_u8"
            ),
            "string closure should return an owned buffer: {}",
            closure_wrapper
        );
        assert!(
            closure_wrapper.contains("let encoded = ({ var writer = WireWriter(); writer.writeString(result); return writer.finalize() })()"),
            "string closure should encode the Swift result before returning it: {}",
            closure_wrapper
        );
        assert!(
            closure_wrapper.contains("return FfiBuf_u8(ptr: allocated"),
            "string closure should allocate an owned buffer result: {}",
            closure_wrapper
        );
    }

    #[test]
    fn inline_blittable_record_closure_returns_c_struct() {
        use crate::ir::definitions::FunctionDef;
        use crate::ir::ids::FunctionId;

        let mut contract = contract_with_blittable_point();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("PointMapper"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("call"),
                params: vec![ParamDef {
                    name: ParamName::new("point"),
                    type_expr: TypeExpr::Record(RecordId::new("Point")),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract.functions.push(FunctionDef {
            id: FunctionId::new("apply_point"),
            params: vec![
                ParamDef {
                    name: ParamName::new("callback"),
                    type_expr: TypeExpr::Callback(CallbackId::new("PointMapper")),
                    passing: ParamPassing::ImplTrait,
                    doc: None,
                },
                ParamDef {
                    name: ParamName::new("point"),
                    type_expr: TypeExpr::Record(RecordId::new("Point")),
                    passing: ParamPassing::Value,
                    doc: None,
                },
            ],
            returns: ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let closure_wrapper = module.functions[0].params[0]
            .wrapper_code()
            .expect("inline closure should render a wrapper");

        assert!(
            closure_wrapper.contains(
                "@convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt) -> ___Point"
            ),
            "blittable record closure should return the C struct directly: {}",
            closure_wrapper
        );
        assert!(
            closure_wrapper.contains("return ___Point(x: result.x, y: result.y)"),
            "blittable record closure should pack Swift fields into the C struct: {}",
            closure_wrapper
        );
    }

    #[test]
    fn swift_default_literal_bool() {
        assert_eq!(swift_default_literal(&DefaultValue::Bool(true)), "true");
        assert_eq!(swift_default_literal(&DefaultValue::Bool(false)), "false");
    }

    #[test]
    fn swift_default_literal_integer() {
        assert_eq!(swift_default_literal(&DefaultValue::Integer(42)), "42");
        assert_eq!(swift_default_literal(&DefaultValue::Integer(-1)), "-1");
    }

    #[test]
    fn swift_default_literal_float() {
        assert_eq!(swift_default_literal(&DefaultValue::Float(2.5)), "2.5");
    }

    #[test]
    fn swift_default_literal_string() {
        assert_eq!(
            swift_default_literal(&DefaultValue::String("hello".to_string())),
            "\"hello\""
        );
    }

    #[test]
    fn swift_default_literal_enum_variant() {
        assert_eq!(
            swift_default_literal(&DefaultValue::EnumVariant {
                enum_name: "Direction".to_string(),
                variant_name: "North".to_string(),
            }),
            ".north"
        );
    }

    #[test]
    fn swift_default_literal_null() {
        assert_eq!(swift_default_literal(&DefaultValue::Null), "nil");
    }

    #[test]
    fn record_field_default_expr_propagates() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Config"),
            fields: vec![
                FieldDef {
                    name: FieldName::new("name"),
                    type_expr: TypeExpr::String,
                    doc: None,
                    default: None,
                },
                FieldDef {
                    name: FieldName::new("retries"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                    doc: None,
                    default: Some(DefaultValue::Integer(3)),
                },
                FieldDef {
                    name: FieldName::new("label"),
                    type_expr: TypeExpr::Option(Box::new(TypeExpr::String)),
                    doc: None,
                    default: Some(DefaultValue::Null),
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert!(record.fields[0].default_expr.is_none());
        assert_eq!(record.fields[1].default_expr.as_deref(), Some("3"));
        assert_eq!(record.fields[2].default_expr.as_deref(), Some("nil"));
    }

    fn contract_with_blittable_point() -> FfiContract {
        let mut contract = empty_contract();
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
        contract
    }

    #[test]
    fn blittable_param_uses_to_composite_conversion() {
        use crate::ir::definitions::FunctionDef;
        use crate::ir::ids::FunctionId;

        let mut contract = contract_with_blittable_point();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("translate"),
            params: vec![ParamDef {
                name: ParamName::new("point"),
                type_expr: TypeExpr::Record(RecordId::new("Point")),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let func = &module.functions[0];
        let param = &func.params[0];

        assert!(
            matches!(param.conversion, SwiftConversion::ToComposite { .. }),
            "blittable record param should use ToComposite, got: {:?}",
            param.conversion
        );

        let ffi_arg = param.ffi_arg();
        assert!(
            ffi_arg.contains("___Point"),
            "ffi_arg should construct ___Point, got: {}",
            ffi_arg
        );
        assert!(
            ffi_arg.contains("x: point.x") && ffi_arg.contains("y: point.y"),
            "ffi_arg should map fields, got: {}",
            ffi_arg
        );
    }

    #[test]
    fn blittable_return_uses_from_composite() {
        use crate::ir::definitions::FunctionDef;
        use crate::ir::ids::FunctionId;

        let mut contract = contract_with_blittable_point();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("get_origin"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Record(RecordId::new("Point"))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let module = lower_contract(&contract);
        let func = &module.functions[0];

        assert!(
            func.returns.is_composite(),
            "blittable return should be FromComposite, got: {:?}",
            func.returns
        );

        let convert = func.returns.composite_convert_expr("_raw").unwrap();
        assert!(
            convert.contains("Point(")
                && convert.contains("x: _raw.x")
                && convert.contains("y: _raw.y"),
            "convert expr should construct Point from C fields, got: {}",
            convert
        );
    }

    fn blittable_point_with_methods() -> RecordDef {
        RecordDef {
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
            constructors: vec![ConstructorDef::Default {
                params: vec![
                    ParamDef {
                        name: ParamName::new("x"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                        passing: ParamPassing::Value,
                        doc: None,
                    },
                    ParamDef {
                        name: ParamName::new("y"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                        passing: ParamPassing::Value,
                        doc: None,
                    },
                ],
                is_fallible: false,
                is_optional: false,
                doc: None,
                deprecated: None,
            }],
            methods: vec![
                MethodDef {
                    id: MethodId::new("magnitude"),
                    receiver: Receiver::RefSelf,
                    params: vec![],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                    execution_kind: ExecutionKind::Sync,
                    doc: None,
                    deprecated: None,
                },
                MethodDef {
                    id: MethodId::new("normalize"),
                    receiver: Receiver::RefMutSelf,
                    params: vec![],
                    returns: ReturnDef::Void,
                    execution_kind: ExecutionKind::Sync,
                    doc: None,
                    deprecated: None,
                },
                MethodDef {
                    id: MethodId::new("origin"),
                    receiver: Receiver::Static,
                    params: vec![],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                    execution_kind: ExecutionKind::Sync,
                    doc: None,
                    deprecated: None,
                },
            ],
            doc: None,
            deprecated: None,
        }
    }

    #[test]
    fn record_with_methods_lowers_constructors_and_methods() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(blittable_point_with_methods());

        let module = lower_contract(&contract);
        let record = &module.records[0];

        assert_eq!(record.constructors.len(), 1);
        assert_eq!(record.methods.len(), 3);
    }

    #[test]
    fn record_instance_method_has_value_self() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(blittable_point_with_methods());

        let module = lower_contract(&contract);
        let record = &module.records[0];

        let magnitude = record
            .methods
            .iter()
            .find(|m| m.name == "magnitude")
            .unwrap();
        assert!(!magnitude.is_static);
        assert!(magnitude.value_self.is_some());
        assert!(!magnitude.is_mutating());

        let rs = magnitude.value_self.as_ref().unwrap();
        assert_eq!(rs.ffi_args.len(), 1);
        assert!(rs.ffi_args[0].contains("___Point("));
        assert!(rs.wrapper_code.is_none());
    }

    #[test]
    fn record_mutating_method_is_marked() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(blittable_point_with_methods());

        let module = lower_contract(&contract);
        let record = &module.records[0];

        let normalize = record
            .methods
            .iter()
            .find(|m| m.name == "normalize")
            .unwrap();
        assert!(normalize.is_mutating());
        assert!(normalize.value_self.as_ref().unwrap().is_mutating);
    }

    #[test]
    fn record_static_method_has_no_value_self() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(blittable_point_with_methods());

        let module = lower_contract(&contract);
        let record = &module.records[0];

        let origin = record.methods.iter().find(|m| m.name == "origin").unwrap();
        assert!(origin.is_static);
        assert!(origin.value_self.is_none());
    }

    #[test]
    fn record_method_sync_call_expr_includes_self() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(blittable_point_with_methods());

        let module = lower_contract(&contract);
        let record = &module.records[0];

        let magnitude = record
            .methods
            .iter()
            .find(|m| m.name == "magnitude")
            .unwrap();
        let call = magnitude.sync_call_expr();
        assert!(
            call.contains("___Point("),
            "sync_call_expr should include self conversion: {}",
            call
        );
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
    fn c_style_enum_method_lowers_with_raw_value_self() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());

        let module = lower_contract(&contract);
        let e = &module.enums[0];

        assert_eq!(e.methods.len(), 1);
        let method = &e.methods[0];
        assert_eq!(method.name, "opposite");
        assert!(!method.is_static);
        assert!(method.value_self.is_some());

        let vs = method.value_self.as_ref().unwrap();
        assert_eq!(vs.ffi_args, vec!["self.rawValue"]);
        assert!(vs.wrapper_code.is_none());
    }

    #[test]
    fn c_style_enum_method_sync_call_includes_raw_value() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(c_style_enum_with_method());

        let module = lower_contract(&contract);
        let method = &module.enums[0].methods[0];
        let call = method.sync_call_expr();
        assert!(
            call.contains("self.rawValue"),
            "sync_call_expr should pass self.rawValue: {}",
            call
        );
    }

    #[test]
    fn static_method_returning_optional_record_is_wire_encoded() {
        let mut record = blittable_point_with_methods();
        record.methods.push(MethodDef {
            id: MethodId::new("checked_unit"),
            receiver: Receiver::Static,
            params: vec![
                ParamDef {
                    name: ParamName::new("x"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    passing: ParamPassing::Value,
                    doc: None,
                },
                ParamDef {
                    name: ParamName::new("y"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                    passing: ParamPassing::Value,
                    doc: None,
                },
            ],
            returns: ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Record(RecordId::new(
                "Point",
            ))))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let mut contract = empty_contract();
        contract.catalog.insert_record(record);
        let module = lower_contract(&contract);
        let swift_record = &module.records[0];

        let method = swift_record
            .methods
            .iter()
            .find(|m| m.name == "checkedUnit")
            .expect("checkedUnit not found in lowered methods");

        assert!(method.is_static);
        assert!(
            method.returns.is_wire_encoded(),
            "Option<Record> return should be wire-encoded, got: {:?}",
            method.returns
        );
    }
}
