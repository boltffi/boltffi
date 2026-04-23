use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::transport::{
    EncodedReturnStrategy, EnumTagStrategy, ReturnInvocationContext, ReturnPlatform,
    ScalarReturnStrategy, ValueReturnMethod, ValueReturnStrategy,
};

use super::JavaOptions;
use super::mappings;
use super::names::NamingConvention;
use super::plan::{
    JavaAsyncCall, JavaAsyncCallbackInvoker, JavaAsyncCallbackMethod, JavaAsyncMode,
    JavaBlittableField, JavaBlittableLayout, JavaBridgeParam, JavaBridgeReturn,
    JavaCallbackErrorCapture, JavaCallbackProxyAsyncMethod, JavaCallbackProxySyncMethod,
    JavaCallbackTrait, JavaClass, JavaClassMethod, JavaClosureInterface, JavaConstructor,
    JavaConstructorKind, JavaDirectCompositeInput, JavaEnum, JavaEnumField, JavaEnumKind,
    JavaEnumVariant, JavaFunction, JavaInputBindings, JavaModule, JavaNativeParam, JavaParam,
    JavaRecord, JavaRecordDefaultConstructor, JavaRecordDefaultConstructorParam, JavaRecordField,
    JavaRecordShape, JavaResultBridgeReturn, JavaReturnPlan, JavaReturnRender, JavaStream,
    JavaStreamMode, JavaSyncCallbackMethod, JavaValueBridgeRender, JavaValueBridgeReturn,
    JavaValueTypeConstructor, JavaValueTypeMethod, JavaWireWriter,
};
use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnShape, StreamItemTransport,
};
use crate::ir::contract::FfiContract;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, CustomTypeDef,
    DefaultValue, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, Receiver,
    RecordDef, ReturnDef, StreamDef, StreamMode, VariantPayload,
};
use crate::ir::ids::{CallbackId, CustomTypeId, EnumId, FieldName, RecordId};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq,
    remap_root_in_seq,
};
use crate::ir::plan::{AbiType, ScalarOrigin, SpanContent, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};

pub struct JavaLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: JavaOptions,
    supported_types: HashSet<String>,
}

#[derive(Clone, Copy)]
enum JavaValueTypeDef<'a> {
    Record(&'a RecordDef),
    Enum(&'a EnumDef),
}

impl<'a> JavaValueTypeDef<'a> {
    fn type_expr(self) -> TypeExpr {
        match self {
            Self::Record(record) => TypeExpr::Record(record.id.clone()),
            Self::Enum(enumeration) => TypeExpr::Enum(enumeration.id.clone()),
        }
    }

    fn type_name(self) -> &'a str {
        match self {
            Self::Record(record) => record.id.as_str(),
            Self::Enum(enumeration) => enumeration.id.as_str(),
        }
    }

    fn constructors(self) -> &'a [ConstructorDef] {
        match self {
            Self::Record(record) => &record.constructors,
            Self::Enum(enumeration) => &enumeration.constructors,
        }
    }

    fn methods(self) -> &'a [MethodDef] {
        match self {
            Self::Record(record) => &record.methods,
            Self::Enum(enumeration) => &enumeration.methods,
        }
    }

    fn constructor_call_id(self, index: usize) -> CallId {
        match self {
            Self::Record(record) => CallId::RecordConstructor {
                record_id: record.id.clone(),
                index,
            },
            Self::Enum(enumeration) => CallId::EnumConstructor {
                enum_id: enumeration.id.clone(),
                index,
            },
        }
    }

    fn method_call_id(self, method: &MethodDef) -> CallId {
        match self {
            Self::Record(record) => CallId::RecordMethod {
                record_id: record.id.clone(),
                method_id: method.id.clone(),
            },
            Self::Enum(enumeration) => CallId::EnumMethod {
                enum_id: enumeration.id.clone(),
                method_id: method.id.clone(),
            },
        }
    }
}

impl<'a> JavaLowerer<'a> {
    pub fn new(
        ffi: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: JavaOptions,
    ) -> Self {
        let supported_types = Self::compute_supported_types(ffi, abi);
        Self {
            ffi,
            abi,
            package_name,
            module_name,
            options,
            supported_types,
        }
    }

    fn is_leaf_supported(ffi: &FfiContract, ty: &TypeExpr, supported: &HashSet<String>) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Bytes | TypeExpr::Void => true,
            TypeExpr::Record(id) => supported.contains(id.as_str()),
            TypeExpr::Enum(id) => supported.contains(id.as_str()),
            TypeExpr::Custom(id) => ffi
                .catalog
                .resolve_custom(id)
                .is_some_and(|custom| Self::is_leaf_supported(ffi, &custom.repr, supported)),
            TypeExpr::Option(inner) => Self::is_leaf_supported(ffi, inner, supported),
            TypeExpr::Vec(inner) => Self::is_leaf_supported(ffi, inner, supported),
            TypeExpr::Callback(_) => true,
            TypeExpr::Handle(_) => true,
            _ => false,
        }
    }

    fn compute_supported_types(ffi: &FfiContract, abi: &AbiContract) -> HashSet<String> {
        let mut supported = HashSet::new();
        let mut changed = true;

        while changed {
            changed = false;

            for record in ffi.catalog.all_records() {
                let id = record.id.as_str();
                if supported.contains(id) {
                    continue;
                }
                let all_ok = record
                    .fields
                    .iter()
                    .all(|f| Self::is_leaf_supported(ffi, &f.type_expr, &supported));
                if all_ok {
                    supported.insert(id.to_string());
                    changed = true;
                }
            }

            for enumeration in ffi.catalog.all_enums() {
                let id = enumeration.id.as_str();
                if supported.contains(id) {
                    continue;
                }
                let abi_enum = abi
                    .enums
                    .iter()
                    .find(|ae| ae.id == enumeration.id)
                    .expect("abi enum missing");

                let all_ok = if abi_enum.is_c_style {
                    true
                } else {
                    abi_enum
                        .variants
                        .iter()
                        .all(|variant| match &variant.payload {
                            AbiEnumPayload::Unit => true,
                            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                                fields
                                    .iter()
                                    .all(|f| Self::is_leaf_supported(ffi, &f.type_expr, &supported))
                            }
                        })
                };

                if all_ok {
                    supported.insert(id.to_string());
                    changed = true;
                }
            }
        }

        supported
    }

    pub fn module(&self) -> JavaModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| naming::library_name(&self.module_name));

        let prefix = boltffi_ffi_rules::naming::ffi_prefix().to_string();

        let enums: Vec<JavaEnum> = self
            .ffi
            .catalog
            .all_enums()
            .filter(|e| self.supported_types.contains(e.id.as_str()))
            .map(|e| self.lower_enum(e))
            .collect();

        let records: Vec<JavaRecord> = self
            .ffi
            .catalog
            .all_records()
            .filter(|r| self.supported_types.contains(r.id.as_str()))
            .map(|r| self.lower_record(r))
            .collect();

        let functions: Vec<JavaFunction> = self
            .ffi
            .functions
            .iter()
            .filter(|f| self.is_supported_function(f))
            .map(|f| self.lower_function(f))
            .collect();

        let closures: Vec<JavaClosureInterface> = self
            .ffi
            .catalog
            .all_callbacks()
            .filter(|cb| matches!(cb.kind, CallbackKind::Closure))
            .map(|cb| self.lower_closure(cb))
            .collect();

        let callbacks: Vec<JavaCallbackTrait> = self
            .ffi
            .catalog
            .all_callbacks()
            .filter(|cb| matches!(cb.kind, CallbackKind::Trait))
            .filter(|cb| !cb.methods.is_empty())
            .map(|cb| self.lower_callback_trait(cb))
            .collect();

        let async_callback_invokers = self.collect_async_callback_invokers(&callbacks);

        let classes: Vec<JavaClass> = self
            .ffi
            .catalog
            .all_classes()
            .map(|c| self.lower_class(c))
            .collect();

        JavaModule {
            package_name: self.package_name.clone(),
            class_name: NamingConvention::class_name(&self.module_name),
            lib_name,
            desktop_loader: self.options.desktop_loader,
            java_version: self.options.min_java_version,
            async_mode: JavaAsyncMode::from_version(self.options.min_java_version),
            prefix,
            records,
            enums,
            closures,
            callbacks,
            async_callback_invokers,
            functions,
            classes,
        }
    }

    fn is_supported_function(&self, func: &FunctionDef) -> bool {
        let params_ok = func
            .params
            .iter()
            .all(|p| self.is_supported_type(&p.type_expr));
        let return_ok = match &func.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_type(ty),
            ReturnDef::Result { ok, err } => {
                self.is_supported_result_type(ok) && self.is_supported_result_type(err)
            }
        };
        params_ok && return_ok
    }

    fn is_supported_result_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Void => true,
            TypeExpr::Handle(_) => false,
            _ => self.is_supported_type(ty),
        }
    }

    fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        Self::is_leaf_supported(self.ffi, ty, &self.supported_types)
    }

    fn resolve_custom_type(&self, id: &CustomTypeId) -> &CustomTypeDef {
        self.ffi
            .catalog
            .resolve_custom(id)
            .unwrap_or_else(|| panic!("custom type should be resolved: {:?}", id))
    }

    fn custom_repr_type(&self, id: &CustomTypeId) -> &TypeExpr {
        &self.resolve_custom_type(id).repr
    }

    fn normalize_custom_type_expr(&self, ty: &TypeExpr) -> TypeExpr {
        match ty {
            TypeExpr::Custom(id) => self.normalize_custom_type_expr(self.custom_repr_type(id)),
            TypeExpr::Option(inner) => {
                TypeExpr::Option(Box::new(self.normalize_custom_type_expr(inner)))
            }
            TypeExpr::Vec(inner) => TypeExpr::Vec(Box::new(self.normalize_custom_type_expr(inner))),
            TypeExpr::Result { ok, err } => TypeExpr::Result {
                ok: Box::new(self.normalize_custom_type_expr(ok)),
                err: Box::new(self.normalize_custom_type_expr(err)),
            },
            _ => ty.clone(),
        }
    }

    fn normalize_custom_size_expr(size: &SizeExpr) -> SizeExpr {
        match size {
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: value.clone(),
                inner: Box::new(Self::normalize_custom_size_expr(inner)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: value.clone(),
                inner: Box::new(Self::normalize_custom_size_expr(inner)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: value.clone(),
                ok: Box::new(Self::normalize_custom_size_expr(ok)),
                err: Box::new(Self::normalize_custom_size_expr(err)),
            },
            SizeExpr::Sum(parts) => {
                SizeExpr::Sum(parts.iter().map(Self::normalize_custom_size_expr).collect())
            }
            _ => size.clone(),
        }
    }

    fn normalize_custom_read_seq(&self, seq: &ReadSeq) -> ReadSeq {
        if let Some(ReadOp::Custom { underlying, .. }) = seq.ops.first() {
            return self.normalize_custom_read_seq(underlying);
        }

        ReadSeq {
            size: Self::normalize_custom_size_expr(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.normalize_custom_read_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn normalize_custom_read_op(&self, op: &ReadOp) -> ReadOp {
        match op {
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: tag_offset.clone(),
                some: Box::new(self.normalize_custom_read_seq(some)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: len_offset.clone(),
                element_type: self.normalize_custom_type_expr(element_type),
                element: Box::new(self.normalize_custom_read_seq(element)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: offset.clone(),
                fields: fields
                    .iter()
                    .map(|field| FieldReadOp {
                        name: field.name.clone(),
                        seq: self.normalize_custom_read_seq(&field.seq),
                    })
                    .collect(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: tag_offset.clone(),
                ok: Box::new(self.normalize_custom_read_seq(ok)),
                err: Box::new(self.normalize_custom_read_seq(err)),
            },
            ReadOp::Custom { underlying, .. } => self
                .normalize_custom_read_seq(underlying)
                .ops
                .into_iter()
                .next()
                .expect("normalized custom read op should not be empty"),
            _ => op.clone(),
        }
    }

    fn normalize_custom_write_seq(&self, seq: &WriteSeq) -> WriteSeq {
        if let Some(WriteOp::Custom { underlying, .. }) = seq.ops.first() {
            return self.normalize_custom_write_seq(underlying);
        }

        WriteSeq {
            size: Self::normalize_custom_size_expr(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.normalize_custom_write_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn normalize_custom_write_op(&self, op: &WriteOp) -> WriteOp {
        match op {
            WriteOp::Option { value, some } => WriteOp::Option {
                value: value.clone(),
                some: Box::new(self.normalize_custom_write_seq(some)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: value.clone(),
                element_type: self.normalize_custom_type_expr(element_type),
                element: Box::new(self.normalize_custom_write_seq(element)),
                layout: layout.clone(),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: value.clone(),
                fields: fields
                    .iter()
                    .map(|field| crate::ir::ops::FieldWriteOp {
                        name: field.name.clone(),
                        accessor: field.accessor.clone(),
                        seq: self.normalize_custom_write_seq(&field.seq),
                    })
                    .collect(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: value.clone(),
                ok: Box::new(self.normalize_custom_write_seq(ok)),
                err: Box::new(self.normalize_custom_write_seq(err)),
            },
            WriteOp::Custom { underlying, .. } => self
                .normalize_custom_write_seq(underlying)
                .ops
                .into_iter()
                .next()
                .expect("normalized custom write op should not be empty"),
            _ => op.clone(),
        }
    }

    fn emit_reader_read(&self, seq: &ReadSeq) -> String {
        let normalized = self.normalize_custom_read_seq(seq);
        super::emit::emit_reader_read(&normalized)
    }

    fn emit_write_expr(&self, seq: &WriteSeq, writer_name: &str) -> String {
        let normalized = self.normalize_custom_write_seq(seq);
        super::emit::emit_write_expr(&normalized, writer_name)
    }

    fn emit_size_expr(&self, size: &SizeExpr) -> String {
        let normalized = Self::normalize_custom_size_expr(size);
        super::emit::emit_size_expr(&normalized)
    }

    fn lower_record(&self, record: &RecordDef) -> JavaRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(&record.id, field))
            .collect();
        let blittable_layout = self.lower_blittable_layout(&record.id);
        let shape = if self.can_use_native_record_syntax(record) {
            JavaRecordShape::NativeRecord
        } else {
            JavaRecordShape::ClassicClass
        };
        let value_type = JavaValueTypeDef::Record(record);
        JavaRecord {
            doc: record.doc.clone(),
            shape,
            is_error: record.is_error,
            class_name,
            fields,
            default_constructors: self.lower_record_default_constructors(record),
            blittable_layout,
            constructors: self.lower_value_type_constructors(value_type),
            methods: self.lower_value_type_methods(value_type),
        }
    }

    fn can_use_native_record_syntax(&self, record: &RecordDef) -> bool {
        !record.is_error
            && self.options.min_java_version.supports_records()
            && !record
                .fields
                .iter()
                .any(|field| self.contains_primitive_array_component(&field.type_expr))
    }

    fn contains_primitive_array_component(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Bytes => true,
            TypeExpr::Vec(inner) => {
                matches!(inner.as_ref(), TypeExpr::Primitive(_))
                    || self.contains_primitive_array_component(inner)
            }
            TypeExpr::Option(inner) => self.contains_primitive_array_component(inner),
            TypeExpr::Result { ok, err } => {
                self.contains_primitive_array_component(ok)
                    || self.contains_primitive_array_component(err)
            }
            TypeExpr::Custom(id) => {
                self.contains_primitive_array_component(self.custom_repr_type(id))
            }
            _ => false,
        }
    }

    fn lower_blittable_layout(&self, record_id: &RecordId) -> Option<JavaBlittableLayout> {
        let abi_record = self.abi_record_for(record_id)?;
        if !abi_record.is_blittable {
            return None;
        }
        let struct_size = abi_record.size?;
        let fields = match abi_record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(Self::lower_blittable_field)
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        Some(JavaBlittableLayout {
            struct_size,
            fields,
        })
    }

    fn lower_blittable_field(field: &FieldReadOp) -> Option<JavaBlittableField> {
        let (primitive, offset) = match field.seq.ops.first() {
            Some(ReadOp::Primitive { primitive, offset }) => (*primitive, offset),
            _ => return None,
        };
        let offset = match offset {
            OffsetExpr::Base => 0,
            OffsetExpr::BasePlus(offset) => *offset,
            _ => return None,
        };
        let name = NamingConvention::field_name(field.name.as_str());
        let const_name = NamingConvention::enum_constant_name(field.name.as_str());
        Some(JavaBlittableField {
            name: name.clone(),
            const_name: const_name.clone(),
            offset,
            decode_expr: java_blittable_decode_expr(primitive, &const_name),
            encode_expr: java_blittable_encode_expr(primitive, &const_name, &name),
        })
    }

    fn lower_record_field(&self, record_id: &RecordId, field: &FieldDef) -> JavaRecordField {
        let decode_seq = self
            .record_field_read_seq(record_id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(record_id, &field.name)
            .expect("record field encode ops");
        JavaRecordField {
            doc: field.doc.clone(),
            name: NamingConvention::field_name(field.name.as_str()),
            java_type: self.java_type(&field.type_expr),
            default_value: field
                .default
                .as_ref()
                .map(|default| self.java_default_literal(default, &field.type_expr)),
            wire_decode_expr: self.emit_reader_read(&decode_seq),
            wire_size_expr: self.emit_size_expr(&encode_seq.size),
            wire_encode_expr: self.emit_write_expr(&encode_seq, "wire"),
            equals_expr: self.record_field_equals_expr(&field.type_expr, field.name.as_str()),
            hash_expr: self.record_field_hash_expr(&field.type_expr, field.name.as_str()),
        }
    }

    fn lower_record_default_constructors(
        &self,
        record: &RecordDef,
    ) -> Vec<JavaRecordDefaultConstructor> {
        let trailing_default_count = record
            .fields
            .iter()
            .rev()
            .take_while(|field| field.default.is_some())
            .count();

        (1..=trailing_default_count)
            .map(|omitted_count| {
                let included_count = record.fields.len() - omitted_count;
                let params = record
                    .fields
                    .iter()
                    .take(included_count)
                    .map(|field| JavaRecordDefaultConstructorParam {
                        name: NamingConvention::field_name(field.name.as_str()),
                        java_type: self.java_type(&field.type_expr),
                    })
                    .collect();
                let arguments = record
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        if index < included_count {
                            NamingConvention::field_name(field.name.as_str())
                        } else {
                            self.java_default_literal(
                                field
                                    .default
                                    .as_ref()
                                    .expect("trailing default field must have a default"),
                                &field.type_expr,
                            )
                        }
                    })
                    .collect();

                JavaRecordDefaultConstructor { params, arguments }
            })
            .collect()
    }

    fn lower_value_type_constructors(
        &self,
        owner: JavaValueTypeDef<'_>,
    ) -> Vec<JavaValueTypeConstructor> {
        owner
            .constructors()
            .iter()
            .enumerate()
            .filter(|(_, constructor)| constructor.name().is_some())
            .map(|(index, constructor)| {
                let call = self.find_abi_call(&owner.constructor_call_id(index));
                JavaValueTypeConstructor::lower(self, owner, constructor, call)
            })
            .collect()
    }

    fn java_default_literal(&self, default: &DefaultValue, ty: &TypeExpr) -> String {
        if let TypeExpr::Option(inner) = ty {
            return match default {
                DefaultValue::Null => "java.util.Optional.empty()".to_string(),
                _ => format!(
                    "java.util.Optional.of({})",
                    self.java_default_literal(default, inner)
                ),
            };
        }

        match default {
            DefaultValue::Bool(value) => value.to_string(),
            DefaultValue::Integer(value) => match ty {
                TypeExpr::Primitive(
                    PrimitiveType::I64
                    | PrimitiveType::U64
                    | PrimitiveType::ISize
                    | PrimitiveType::USize,
                ) => format!("{value}L"),
                _ => value.to_string(),
            },
            DefaultValue::Float(value) => match ty {
                TypeExpr::Primitive(PrimitiveType::F32) => format!("{value}f"),
                _ => value.to_string(),
            },
            DefaultValue::String(value) => {
                format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
            }
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => self.java_enum_default_literal(enum_name, variant_name),
            DefaultValue::Null => "null".to_string(),
        }
    }

    fn java_enum_default_literal(&self, enum_name: &str, variant_name: &str) -> String {
        let enum_id = EnumId::new(enum_name);
        let enum_def = self
            .ffi
            .catalog
            .resolve_enum(&enum_id)
            .expect("enum default should reference a known enum");
        let class_name = NamingConvention::class_name(enum_name);
        let variant_class_name = NamingConvention::class_name(variant_name);
        match &enum_def.repr {
            EnumRepr::CStyle { .. } if enum_def.is_error => {
                format!(
                    "{}.{}",
                    class_name,
                    NamingConvention::enum_constant_name(variant_name)
                )
            }
            EnumRepr::CStyle { .. } => {
                format!(
                    "{}.{}",
                    class_name,
                    NamingConvention::enum_constant_name(variant_name)
                )
            }
            EnumRepr::Data { variants, .. } => {
                let variant = variants
                    .iter()
                    .find(|candidate| candidate.name.as_str() == variant_name)
                    .expect("enum default should reference a known variant");
                match variant.payload {
                    VariantPayload::Unit => {
                        if self.options.min_java_version.supports_sealed() {
                            format!("new {}.{}()", class_name, variant_class_name)
                        } else {
                            format!("{}.{}.INSTANCE", class_name, variant_class_name)
                        }
                    }
                    _ => panic!("enum defaults only support unit variants in Java"),
                }
            }
        }
    }

    fn lower_value_type_methods(&self, owner: JavaValueTypeDef<'_>) -> Vec<JavaValueTypeMethod> {
        owner
            .methods()
            .iter()
            .filter(|method| self.is_supported_method(method))
            .map(|method| {
                let call = self.find_abi_call(&owner.method_call_id(method));
                JavaValueTypeMethod::lower(self, owner, method, call)
            })
            .collect()
    }

    fn find_abi_call(&self, call_id: &CallId) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| &call.id == call_id)
            .expect("abi call not found")
    }

    fn strip_value_self_param(call: &AbiCall) -> AbiCall {
        AbiCall {
            params: call
                .params
                .iter()
                .filter(|param| param.name.as_str() != "self")
                .cloned()
                .collect(),
            ..call.clone()
        }
    }

    fn lower_self_input_bindings(&self, call: &AbiCall) -> JavaInputBindings {
        let Some(self_param) = call
            .params
            .iter()
            .find(|param| param.name.as_str() == "self")
        else {
            return JavaInputBindings::default();
        };
        match &self_param.role {
            ParamRole::Input {
                transport: Transport::Composite(_),
                ..
            } => JavaInputBindings {
                direct_composites: vec![JavaDirectCompositeInput {
                    binding_name: "_direct_self".to_string(),
                    param_name: "self".to_string(),
                    declaration_expr: "this.encodeBlittableInput()".to_string(),
                }],
                wire_writers: Vec::new(),
            },
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => {
                let remapped = remap_root_in_seq(encode_ops, ValueExpr::Var("this".into()));
                JavaInputBindings {
                    direct_composites: Vec::new(),
                    wire_writers: vec![JavaWireWriter {
                        binding_name: "_wire_self".to_string(),
                        param_name: "self".to_string(),
                        size_expr: self.emit_size_expr(&remapped.size),
                        encode_expr: self.emit_write_expr(&remapped, "_wire_self"),
                    }],
                }
            }
            _ => JavaInputBindings::default(),
        }
    }

    fn lower_self_native_param(&self, call: &AbiCall) -> Option<JavaNativeParam> {
        let self_param = call
            .params
            .iter()
            .find(|param| param.name.as_str() == "self")?;
        let transport = match &self_param.role {
            ParamRole::Input { transport, .. } => transport,
            _ => return None,
        };

        Some(match transport {
            Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. }) => JavaNativeParam {
                name: "selfValue".to_string(),
                native_type: mappings::java_type(*tag_type).to_string(),
                expr: "this.nativeValue()".to_string(),
            },
            Transport::Scalar(origin) => JavaNativeParam {
                name: "selfValue".to_string(),
                native_type: mappings::java_type(origin.primitive()).to_string(),
                expr: "this".to_string(),
            },
            Transport::Composite(_) => JavaNativeParam {
                name: "selfBuffer".to_string(),
                native_type: "ByteBuffer".to_string(),
                expr: "_direct_self".to_string(),
            },
            Transport::Span(_) => JavaNativeParam {
                name: "selfBuffer".to_string(),
                native_type: "ByteBuffer".to_string(),
                expr: "_wire_self.toBuffer()".to_string(),
            },
            Transport::Handle { .. } | Transport::Callback { .. } => return None,
        })
    }

    fn record_field_equals_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        let left = format!("this.{field}");
        let right = format!("other.{field}");
        self.value_equals_expr(ty, &left, &right)
    }

    fn record_field_hash_expr(&self, ty: &TypeExpr, field_name: &str) -> String {
        let field = NamingConvention::field_name(field_name);
        self.value_hash_expr(ty, &field)
    }

    fn value_equals_expr(&self, ty: &TypeExpr, left: &str, right: &str) -> String {
        self.value_equals_expr_with_depth(ty, left, right, 0)
    }

    fn value_equals_expr_with_depth(
        &self,
        ty: &TypeExpr,
        left: &str,
        right: &str,
        depth: usize,
    ) -> String {
        match ty {
            TypeExpr::Custom(id) => {
                self.value_equals_expr_with_depth(self.custom_repr_type(id), left, right, depth)
            }
            TypeExpr::Primitive(PrimitiveType::F32) => {
                format!("Float.compare({left}, {right}) == 0")
            }
            TypeExpr::Primitive(PrimitiveType::F64) => {
                format!("Double.compare({left}, {right}) == 0")
            }
            TypeExpr::Primitive(_) => format!("{left} == {right}"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("java.util.Objects.equals({left}, {right})")
            }
            TypeExpr::Bytes => format!("java.util.Arrays.equals({left}, {right})"),
            TypeExpr::Option(inner) => {
                let left_is_null = format!("({left}) == null");
                let right_is_null = format!("({right}) == null");
                let left_present = format!("({left}).isPresent()");
                let right_present = format!("({right}).isPresent()");
                let left_value = format!("({left}).get()");
                let right_value = format!("({right}).get()");
                let inner_equals =
                    self.value_equals_expr_with_depth(inner, &left_value, &right_value, depth);
                format!(
                    "({left_is_null} ? {right_is_null} : (!({right_is_null}) && {left_present} == {right_present} && (!({left_present}) || {inner_equals})))"
                )
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => format!("java.util.Arrays.equals({left}, {right})"),
                _ => {
                    let left_item = format!("leftItem{depth}");
                    let right_item = format!("rightItem{depth}");
                    let inner_equals = self.value_equals_expr_with_depth(
                        inner,
                        &left_item,
                        &right_item,
                        depth + 1,
                    );
                    format!(
                        "WireWriter.listEquals({left}, {right}, ({left_item}, {right_item}) -> {inner_equals})"
                    )
                }
            },
            _ => panic!("unsupported Java equality type: {:?}", ty),
        }
    }

    fn value_hash_expr(&self, ty: &TypeExpr, value: &str) -> String {
        self.value_hash_expr_with_depth(ty, value, 0)
    }

    fn value_hash_expr_with_depth(&self, ty: &TypeExpr, value: &str, depth: usize) -> String {
        match ty {
            TypeExpr::Custom(id) => {
                self.value_hash_expr_with_depth(self.custom_repr_type(id), value, depth)
            }
            TypeExpr::Primitive(PrimitiveType::Bool) => format!("Boolean.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::I8) | TypeExpr::Primitive(PrimitiveType::U8) => {
                format!("Byte.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I16) | TypeExpr::Primitive(PrimitiveType::U16) => {
                format!("Short.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I32) | TypeExpr::Primitive(PrimitiveType::U32) => {
                format!("Integer.hashCode({value})")
            }
            TypeExpr::Primitive(PrimitiveType::I64)
            | TypeExpr::Primitive(PrimitiveType::U64)
            | TypeExpr::Primitive(PrimitiveType::ISize)
            | TypeExpr::Primitive(PrimitiveType::USize) => format!("Long.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::F32) => format!("Float.hashCode({value})"),
            TypeExpr::Primitive(PrimitiveType::F64) => format!("Double.hashCode({value})"),
            TypeExpr::String | TypeExpr::Record(_) | TypeExpr::Enum(_) => {
                format!("java.util.Objects.hashCode({value})")
            }
            TypeExpr::Bytes => format!("java.util.Arrays.hashCode({value})"),
            TypeExpr::Option(inner) => {
                let inner_value = format!("({value}).get()");
                let inner_hash = self.value_hash_expr_with_depth(inner, &inner_value, depth);
                format!(
                    "(({value}) == null ? 0 : (({value}).isPresent() ? (31 + ({inner_hash})) : 0))"
                )
            }
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(_) => format!("java.util.Arrays.hashCode({value})"),
                _ => {
                    let item = format!("item{depth}");
                    let inner_hash = self.value_hash_expr_with_depth(inner, &item, depth + 1);
                    format!("WireWriter.listHash({value}, {item} -> {inner_hash})")
                }
            },
            _ => panic!("unsupported Java hash type: {:?}", ty),
        }
    }

    fn record_field_read_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.decode_ops.ops.first() {
                Some(ReadOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn record_field_write_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.encode_ops.ops.first() {
                Some(WriteOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn lower_function(&self, func: &FunctionDef) -> JavaFunction {
        let call = self.abi_call_for_function(func);

        let input_bindings = self.input_bindings_for_params(call);

        let params: Vec<JavaParam> = func
            .params
            .iter()
            .map(|parameter| {
                self.lower_param(
                    parameter.name.as_str(),
                    &parameter.type_expr,
                    call,
                    &input_bindings,
                )
            })
            .collect();

        let async_call = self.async_call_from_mode(call, &func.returns);
        let return_plan = match &async_call {
            Some(async_call) => async_call.complete_return_plan.clone(),
            None => self.return_plan(&func.returns, call),
        };

        JavaFunction {
            doc: func.doc.clone(),
            name: NamingConvention::method_name(func.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            params,
            return_type: self.return_java_type(&func.returns),
            return_plan,
            input_bindings,
            async_call,
        }
    }

    fn lower_param(
        &self,
        name: &str,
        ty: &TypeExpr,
        call: &AbiCall,
        input_bindings: &JavaInputBindings,
    ) -> JavaParam {
        let field_name = NamingConvention::field_name(name);
        let abi_transport = call
            .params
            .iter()
            .find(|p| p.name.as_str() == name)
            .and_then(|p| p.transport());
        self.lower_native_param(name, ty, &field_name, abi_transport, input_bindings)
    }

    fn lower_native_param(
        &self,
        source_name: &str,
        ty: &TypeExpr,
        field_name: &str,
        abi_transport: Option<&Transport>,
        input_bindings: &JavaInputBindings,
    ) -> JavaParam {
        let java_type = self.java_type(ty);

        if let Some((native_type, native_expr)) =
            self.direct_handle_or_callback_param_expr(ty, field_name, abi_transport)
        {
            return JavaParam {
                name: field_name.to_string(),
                java_type,
                native_type,
                native_expr,
            };
        }

        let (native_type, native_expr) = match ty {
            TypeExpr::String => {
                if let Some(binding_name) = input_bindings.binding_name_for(source_name) {
                    (
                        "ByteBuffer".to_string(),
                        format!("{}.toBuffer()", binding_name),
                    )
                } else {
                    (
                        "byte[]".to_string(),
                        format!(
                            "{}.getBytes(java.nio.charset.StandardCharsets.UTF_8)",
                            field_name
                        ),
                    )
                }
            }
            TypeExpr::Bytes => {
                if let Some(binding_name) = input_bindings.binding_name_for(source_name) {
                    (
                        "ByteBuffer".to_string(),
                        format!("{}.toBuffer()", binding_name),
                    )
                } else {
                    ("byte[]".to_string(), field_name.to_string())
                }
            }
            TypeExpr::Record(record_id) if matches!(abi_transport, Some(Transport::Composite(layout)) if &layout.record_id == record_id) =>
            {
                let binding_name = input_bindings
                    .binding_name_for(source_name)
                    .expect("direct composite input binding must exist");
                return JavaParam {
                    name: field_name.to_string(),
                    java_type,
                    native_type: "ByteBuffer".to_string(),
                    native_expr: binding_name.to_string(),
                };
            }
            TypeExpr::Enum(_)
                if matches!(
                    abi_transport,
                    Some(Transport::Scalar(ScalarOrigin::CStyleEnum { .. }))
                ) =>
            {
                let tag_type = match abi_transport {
                    Some(Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. })) => *tag_type,
                    _ => unreachable!(),
                };
                (
                    mappings::java_type(tag_type).to_string(),
                    format!("{}.value", field_name),
                )
            }
            TypeExpr::Vec(inner) => {
                if let Some((native_type, native_expr)) =
                    self.direct_vec_native_param(abi_transport, inner, field_name, source_name)
                {
                    (native_type, native_expr)
                } else if let Some(binding_name) = input_bindings.binding_name_for(source_name) {
                    (
                        "ByteBuffer".to_string(),
                        format!("{}.toBuffer()", binding_name),
                    )
                } else {
                    (java_type.clone(), field_name.to_string())
                }
            }
            TypeExpr::Record(_) | TypeExpr::Enum(_) | TypeExpr::Option(_) | TypeExpr::Custom(_) => {
                let binding_name = input_bindings
                    .binding_name_for(source_name)
                    .expect("encoded input binding must exist");
                (
                    "ByteBuffer".to_string(),
                    format!("{}.toBuffer()", binding_name),
                )
            }
            _ => (java_type.clone(), field_name.to_string()),
        };

        JavaParam {
            name: field_name.to_string(),
            java_type,
            native_type,
            native_expr,
        }
    }

    fn direct_handle_or_callback_param_expr(
        &self,
        ty: &TypeExpr,
        field_name: &str,
        abi_transport: Option<&Transport>,
    ) -> Option<(String, String)> {
        match (ty, abi_transport) {
            (TypeExpr::Handle(_), Some(Transport::Handle { .. })) => {
                Some(("long".to_string(), format!("{}.rawHandle()", field_name)))
            }
            (TypeExpr::Callback(callback_id), Some(Transport::Callback { .. })) => {
                let bridge_name = self.callback_bridge_name(callback_id);
                Some((
                    "long".to_string(),
                    format!("{}.create({})", bridge_name, field_name),
                ))
            }
            (TypeExpr::Option(inner), Some(Transport::Handle { nullable: true, .. }))
                if matches!(inner.as_ref(), TypeExpr::Handle(_)) =>
            {
                Some((
                    "long".to_string(),
                    format!("{}.map(value -> value.rawHandle()).orElse(0L)", field_name),
                ))
            }
            (
                TypeExpr::Option(inner),
                Some(Transport::Callback {
                    callback_id,
                    nullable: true,
                    ..
                }),
            ) if matches!(inner.as_ref(), TypeExpr::Callback(_)) => {
                let bridge_name = self.callback_bridge_name(callback_id);
                Some((
                    "long".to_string(),
                    format!(
                        "{}.map(value -> {}.create(value)).orElse(0L)",
                        field_name, bridge_name
                    ),
                ))
            }
            _ => None,
        }
    }

    fn direct_vec_native_param(
        &self,
        abi_transport: Option<&Transport>,
        inner: &TypeExpr,
        field_name: &str,
        source_name: &str,
    ) -> Option<(String, String)> {
        if let Some(Transport::Span(SpanContent::Composite(layout))) = abi_transport
            && let TypeExpr::Record(record_id) = inner
            && record_id == &layout.record_id
        {
            return Some((
                "ByteBuffer".to_string(),
                vec_blittable_record_param_encode_expr(record_id, source_name),
            ));
        }

        if let Some(Transport::Span(SpanContent::Scalar(ScalarOrigin::CStyleEnum {
            tag_type,
            ..
        }))) = abi_transport
        {
            return Some((
                mappings::java_primitive_array_type(*tag_type).to_string(),
                vec_c_style_enum_param_encode_expr(field_name, *tag_type),
            ));
        }

        None
    }

    fn input_bindings_for_params(&self, call: &AbiCall) -> JavaInputBindings {
        let direct_composites = call
            .params
            .iter()
            .filter_map(|param| self.direct_composite_input(param))
            .collect();
        let wire_writers = call
            .params
            .iter()
            .filter_map(|param| self.wire_writer_for_param(param))
            .collect();

        JavaInputBindings {
            direct_composites,
            wire_writers,
        }
    }

    fn direct_composite_input(&self, param: &AbiParam) -> Option<JavaDirectCompositeInput> {
        match &param.role {
            ParamRole::Input {
                transport: Transport::Composite(_),
                ..
            } => Some(JavaDirectCompositeInput {
                binding_name: format!("_direct_{}", param.name.as_str()),
                param_name: param.name.as_str().to_string(),
                declaration_expr: format!(
                    "{}.encodeBlittableInput()",
                    NamingConvention::field_name(param.name.as_str())
                ),
            }),
            _ => None,
        }
    }

    fn wire_writer_for_param(&self, param: &AbiParam) -> Option<JavaWireWriter> {
        self.input_write_ops(param).map(|encode_ops| {
            let param_name = param.name.as_str().to_string();
            let binding_name = format!("_wire_{}", param.name.as_str());
            let encode_expr = self.emit_write_expr(&encode_ops, &binding_name);
            JavaWireWriter {
                binding_name,
                param_name,
                size_expr: self.emit_size_expr(&encode_ops.size),
                encode_expr,
            }
        })
    }

    fn input_write_ops(&self, param: &AbiParam) -> Option<WriteSeq> {
        match &param.role {
            ParamRole::Input {
                transport: Transport::Composite(_),
                ..
            } => None,
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Composite(_)),
                ..
            } => None,
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => Some(encode_ops.clone()),
            _ => None,
        }
    }

    fn return_java_type(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "void".to_string(),
            ReturnDef::Value(TypeExpr::Void) => "void".to_string(),
            ReturnDef::Value(ty) => self.java_type(ty),
            ReturnDef::Result {
                ok: TypeExpr::Void, ..
            } => "void".to_string(),
            ReturnDef::Result { ok, .. } => self.java_type(ok),
        }
    }

    fn result_return_plan(&self, returns: &ReturnDef, ret_shape: &ReturnShape) -> JavaReturnPlan {
        let decode_ops = ret_shape
            .decode_ops
            .as_ref()
            .expect("Result return should have decode ops");
        let (ok_seq, err_seq) = match decode_ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            _ => panic!("expected ReadOp::Result in decode ops"),
        };
        let ok_decode_expr = self.emit_reader_read(ok_seq);
        let err_decode_expr = self.emit_reader_read(err_seq);
        let err_is_string =
            matches!(returns, ReturnDef::Result { err, .. } if matches!(err, TypeExpr::String));
        let (err_exception_class, err_throw_direct) = match returns {
            ReturnDef::Result {
                err: TypeExpr::Enum(id),
                ..
            } if self.is_flat_error_enum(id) => (
                Some(format!(
                    "{}.Exception",
                    NamingConvention::class_name(id.as_str())
                )),
                false,
            ),
            ReturnDef::Result {
                err: TypeExpr::Enum(id),
                ..
            } if self.is_error_enum(id) => (None, true),
            ReturnDef::Result {
                err: TypeExpr::Record(id),
                ..
            } if self.is_error_record(id) => (None, true),
            _ => (None, false),
        };
        JavaReturnPlan {
            native_return_type: "byte[]".to_string(),
            render: JavaReturnRender::Result {
                ok_decode_expr,
                err_decode_expr,
                err_is_string,
                err_exception_class,
                err_throw_direct,
            },
        }
    }

    fn return_plan(&self, returns: &ReturnDef, call: &AbiCall) -> JavaReturnPlan {
        self.return_plan_for_shape(returns, &call.returns, &call.error)
    }

    fn return_plan_for_shape(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
        error: &ErrorTransport,
    ) -> JavaReturnPlan {
        match returns {
            ReturnDef::Void => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
            ReturnDef::Result { .. } => self.result_return_plan(returns, ret_shape),
            ReturnDef::Value(ty) => self.java_return_plan_for_value_shape(ty, ret_shape, error),
        }
    }

    fn java_return_plan_for_value_shape(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
        _error: &ErrorTransport,
    ) -> JavaReturnPlan {
        let value_return_strategy = ret_shape.value_return_strategy();
        let value_return_method = ret_shape
            .value_return_method(ReturnInvocationContext::HostCall, ReturnPlatform::Native);
        match ty {
            TypeExpr::Void => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
            _ => match (value_return_strategy, value_return_method) {
                (ValueReturnStrategy::Void, _) => JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
                (
                    ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue),
                    ValueReturnMethod::DirectReturn,
                ) => JavaReturnPlan {
                    native_return_type: self.java_type(ty),
                    render: JavaReturnRender::Direct,
                },
                (
                    ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag),
                    ValueReturnMethod::DirectReturn,
                ) => self.java_c_style_enum_return_plan(ty, ret_shape),
                (
                    ValueReturnStrategy::CompositeValue
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::ResultScalar)
                    | ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded),
                    ValueReturnMethod::DirectReturn | ValueReturnMethod::WriteToOutBufferParts,
                ) => self.java_decode_return_plan(ty, ret_shape),
                (
                    ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec),
                    ValueReturnMethod::DirectReturn | ValueReturnMethod::WriteToOutBufferParts,
                ) => self.java_direct_vec_return_plan(ty, ret_shape),
                (ValueReturnStrategy::ObjectHandle, ValueReturnMethod::DirectReturn) => {
                    self.java_handle_return_plan(ret_shape)
                }
                (ValueReturnStrategy::CallbackHandle, ValueReturnMethod::DirectReturn) => {
                    self.java_callback_return_plan(ret_shape)
                }
                _ => JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                },
            },
        }
    }

    fn java_return_plan_for_value(&self, ty: &TypeExpr, call: &AbiCall) -> JavaReturnPlan {
        self.java_return_plan_for_value_shape(ty, &call.returns, &call.error)
    }

    fn java_c_style_enum_return_plan(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
    ) -> JavaReturnPlan {
        match (ty, ret_shape.transport.as_ref()) {
            (
                TypeExpr::Enum(enum_id),
                Some(Transport::Scalar(ScalarOrigin::CStyleEnum { tag_type, .. })),
            ) => JavaReturnPlan {
                native_return_type: mappings::java_type(*tag_type).to_string(),
                render: JavaReturnRender::CStyleEnum {
                    class_name: NamingConvention::class_name(enum_id.as_str()),
                },
            },
            _ => JavaReturnPlan {
                native_return_type: self.java_type(ty),
                render: JavaReturnRender::Direct,
            },
        }
    }

    fn java_decode_return_plan(&self, ty: &TypeExpr, ret_shape: &ReturnShape) -> JavaReturnPlan {
        let decode_expr = match ty {
            TypeExpr::String => "reader.readString()".to_string(),
            TypeExpr::Custom(_) => match &ret_shape.decode_ops {
                Some(decode_seq) => self.emit_reader_read(decode_seq),
                None => {
                    return JavaReturnPlan {
                        native_return_type: "void".to_string(),
                        render: JavaReturnRender::Void,
                    };
                }
            },
            TypeExpr::Option(_) => match &ret_shape.decode_ops {
                Some(decode_seq) => self.emit_reader_read(decode_seq),
                None => panic!(
                    "unsupported direct Option return transport for Java backend: {:?}",
                    ty
                ),
            },
            TypeExpr::Record(id) => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            TypeExpr::Enum(id) => {
                format!(
                    "{}.decode(reader)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            TypeExpr::Vec(_) => match &ret_shape.decode_ops {
                Some(decode_seq) => self.emit_reader_read(decode_seq),
                None => {
                    return JavaReturnPlan {
                        native_return_type: "void".to_string(),
                        render: JavaReturnRender::Void,
                    };
                }
            },
            TypeExpr::Bytes => "_buf != null ? _buf : new byte[0]".to_string(),
            _ => {
                return JavaReturnPlan {
                    native_return_type: "void".to_string(),
                    render: JavaReturnRender::Void,
                };
            }
        };
        JavaReturnPlan {
            native_return_type: "byte[]".to_string(),
            render: JavaReturnRender::Decode { decode_expr },
        }
    }

    fn java_direct_vec_return_plan(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
    ) -> JavaReturnPlan {
        match ty {
            TypeExpr::Bytes => JavaReturnPlan {
                native_return_type: "byte[]".to_string(),
                render: JavaReturnRender::Decode {
                    decode_expr: "_buf != null ? _buf : new byte[0]".to_string(),
                },
            },
            TypeExpr::Vec(inner) => JavaReturnPlan {
                native_return_type: "byte[]".to_string(),
                render: JavaReturnRender::Decode {
                    decode_expr: self.vec_buffer_decode_expr(inner, ret_shape.transport.as_ref()),
                },
            },
            _ => self.java_decode_return_plan(ty, ret_shape),
        }
    }

    fn java_handle_return_plan(&self, ret_shape: &ReturnShape) -> JavaReturnPlan {
        match ret_shape.transport.as_ref() {
            Some(Transport::Handle { class_id, nullable }) => JavaReturnPlan {
                native_return_type: "long".to_string(),
                render: JavaReturnRender::Handle {
                    class_name: NamingConvention::class_name(class_id.as_str()),
                    nullable: *nullable,
                },
            },
            _ => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
        }
    }

    fn java_callback_return_plan(&self, ret_shape: &ReturnShape) -> JavaReturnPlan {
        match ret_shape.transport.as_ref() {
            Some(Transport::Callback {
                callback_id,
                nullable,
                ..
            }) => JavaReturnPlan {
                native_return_type: "long".to_string(),
                render: JavaReturnRender::Callback {
                    callbacks_class_name: self.callback_bridge_name(callback_id),
                    nullable: *nullable,
                },
            },
            _ => JavaReturnPlan {
                native_return_type: "void".to_string(),
                render: JavaReturnRender::Void,
            },
        }
    }

    fn async_call_from_mode(&self, call: &AbiCall, returns: &ReturnDef) -> Option<JavaAsyncCall> {
        let async_abi = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => return None,
        };
        let result_call = AbiCall {
            returns: async_abi.result.clone(),
            ..call.clone()
        };
        let complete_return_plan =
            self.return_plan_for_shape(returns, &result_call.returns, &result_call.error);
        Some(JavaAsyncCall {
            poll: async_abi.poll.as_str().to_string(),
            complete: async_abi.complete.as_str().to_string(),
            cancel: async_abi.cancel.as_str().to_string(),
            free: async_abi.free.as_str().to_string(),
            complete_return_plan,
        })
    }

    fn abi_call_for_function(&self, func: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| matches!(&c.id, CallId::Function(id) if id == &func.id))
            .expect("abi call not found for function")
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, index: usize) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| {
                c.id == CallId::Constructor {
                    class_id: class.id.clone(),
                    index,
                }
            })
            .expect("abi call not found for constructor")
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|c| {
                c.id == CallId::Method {
                    class_id: class.id.clone(),
                    method_id: method.id.clone(),
                }
            })
            .expect("abi call not found for method")
    }

    fn strip_receiver(call: &AbiCall) -> AbiCall {
        AbiCall {
            params: call
                .params
                .iter()
                .filter(|p| !Self::is_instance_receiver(p))
                .cloned()
                .collect(),
            ..call.clone()
        }
    }

    fn is_instance_receiver(param: &AbiParam) -> bool {
        param.name.as_str() == "self"
            && matches!(
                param.role,
                ParamRole::Input {
                    transport: Transport::Handle { .. },
                    ..
                }
            )
    }

    fn lower_class(&self, class: &ClassDef) -> JavaClass {
        let class_name = NamingConvention::class_name(class.id.as_str());
        let ffi_free = boltffi_ffi_rules::naming::class_ffi_free(class.id.as_str()).to_string();

        let constructors = self.resolve_constructor_collisions(
            class
                .constructors
                .iter()
                .enumerate()
                .map(|(index, ctor)| self.lower_constructor(class, ctor, index))
                .collect(),
        );

        let methods = class
            .methods
            .iter()
            .filter(|m| self.is_supported_method(m))
            .map(|m| self.lower_class_method(class, m))
            .collect();

        let streams = class
            .streams
            .iter()
            .map(|stream| self.lower_stream(class, stream))
            .collect();

        JavaClass {
            doc: class.doc.clone(),
            class_name,
            ffi_free,
            constructors,
            methods,
            streams,
        }
    }

    fn resolve_constructor_collisions(
        &self,
        constructors: Vec<JavaConstructor>,
    ) -> Vec<JavaConstructor> {
        let preferred_indices = constructors
            .iter()
            .enumerate()
            .filter(|(_, constructor)| !constructor.is_factory())
            .fold(
                HashMap::<Vec<String>, usize>::new(),
                |mut indices, (index, constructor)| {
                    let signature = Self::constructor_signature(constructor);
                    match indices.get(&signature).copied() {
                        Some(existing_index)
                            if Self::prefers_constructor_surface(
                                constructor,
                                &constructors[existing_index],
                            ) =>
                        {
                            indices.insert(signature, index);
                        }
                        None => {
                            indices.insert(signature, index);
                        }
                        Some(_) => {}
                    }
                    indices
                },
            );

        constructors
            .into_iter()
            .enumerate()
            .map(|(index, mut constructor)| {
                if constructor.is_factory() {
                    return constructor;
                }

                let signature = Self::constructor_signature(&constructor);
                if preferred_indices.get(&signature).copied() != Some(index) {
                    constructor.kind = JavaConstructorKind::Factory;
                }
                constructor
            })
            .collect()
    }

    fn constructor_signature(constructor: &JavaConstructor) -> Vec<String> {
        constructor
            .params
            .iter()
            .map(|param| param.java_type.clone())
            .collect()
    }

    fn prefers_constructor_surface(candidate: &JavaConstructor, current: &JavaConstructor) -> bool {
        Self::constructor_priority(candidate) < Self::constructor_priority(current)
    }

    fn constructor_priority(constructor: &JavaConstructor) -> usize {
        match (constructor.kind, constructor.is_fallible) {
            (JavaConstructorKind::Primary, _) => 0,
            (JavaConstructorKind::Secondary, false) => 1,
            (JavaConstructorKind::Secondary, true) => 2,
            (JavaConstructorKind::Factory, _) => 3,
        }
    }

    fn is_supported_method(&self, method: &MethodDef) -> bool {
        let params_ok = method
            .params
            .iter()
            .all(|p| self.is_supported_type(&p.type_expr));
        let return_ok = match &method.returns {
            ReturnDef::Void => true,
            ReturnDef::Value(ty) => self.is_supported_method_return_type(ty),
            ReturnDef::Result { ok, err } => {
                self.is_supported_result_type(ok) && self.is_supported_result_type(err)
            }
        };
        params_ok && return_ok
    }

    fn is_supported_method_return_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Handle(_) => true,
            _ => self.is_supported_type(ty),
        }
    }

    fn is_error_record(&self, id: &RecordId) -> bool {
        self.ffi
            .catalog
            .resolve_record(id)
            .is_some_and(|record| record.is_error)
    }

    fn is_error_enum(&self, id: &EnumId) -> bool {
        self.ffi
            .catalog
            .resolve_enum(id)
            .is_some_and(|enumeration| enumeration.is_error)
    }

    fn is_flat_error_enum(&self, id: &EnumId) -> bool {
        self.ffi
            .catalog
            .resolve_enum(id)
            .is_some_and(|enumeration| self.uses_flat_error_enum_template(enumeration))
    }

    fn uses_flat_error_enum_template(&self, enumeration: &EnumDef) -> bool {
        if !enumeration.is_error {
            return false;
        }

        match &enumeration.repr {
            EnumRepr::CStyle { .. } => true,
            EnumRepr::Data { variants, .. } => variants
                .iter()
                .all(|variant| matches!(variant.payload, VariantPayload::Unit)),
        }
    }

    fn lower_constructor(
        &self,
        class: &ClassDef,
        ctor: &ConstructorDef,
        index: usize,
    ) -> JavaConstructor {
        let call = self.abi_call_for_constructor(class, index);
        let input_bindings = self.input_bindings_for_params(call);

        let (kind, name) = match ctor {
            ConstructorDef::Default { .. } => (JavaConstructorKind::Primary, String::new()),
            ConstructorDef::NamedFactory { name, .. } => (
                JavaConstructorKind::Factory,
                NamingConvention::method_name(name.as_str()),
            ),
            ConstructorDef::NamedInit { name, .. } => (
                JavaConstructorKind::Secondary,
                NamingConvention::method_name(name.as_str()),
            ),
        };

        let params: Vec<JavaParam> = ctor
            .params()
            .iter()
            .map(|param_def| {
                self.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    call,
                    &input_bindings,
                )
            })
            .collect();

        JavaConstructor {
            doc: ctor.doc().map(str::to_string),
            kind,
            name,
            is_fallible: ctor.is_fallible(),
            params,
            ffi_name: call.symbol.as_str().to_string(),
            input_bindings,
        }
    }

    fn lower_class_method(&self, class: &ClassDef, method: &MethodDef) -> JavaClassMethod {
        let raw_call = self.abi_call_for_method(class, method);
        let call = Self::strip_receiver(raw_call);
        let input_bindings = self.input_bindings_for_params(&call);
        let is_static = method.callable_form() == CallableForm::StaticMethod;

        let params: Vec<JavaParam> = method
            .params
            .iter()
            .map(|param_def| {
                self.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    &call,
                    &input_bindings,
                )
            })
            .collect();

        let async_call = self.async_call_from_mode(&call, &method.returns);
        let return_plan = match &async_call {
            Some(async_call) => async_call.complete_return_plan.clone(),
            None => self.return_plan(&method.returns, &call),
        };

        JavaClassMethod {
            doc: method.doc.clone(),
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            is_static,
            params,
            return_type: self.return_java_type(&method.returns),
            return_plan,
            input_bindings,
            async_call,
        }
    }

    fn lower_stream(&self, class: &ClassDef, stream_def: &StreamDef) -> JavaStream {
        let abi_stream = self.abi_stream(class, stream_def);
        let mode = match stream_def.mode {
            StreamMode::Async => JavaStreamMode::Async,
            StreamMode::Batch => JavaStreamMode::Batch,
            StreamMode::Callback => JavaStreamMode::Callback,
        };
        JavaStream {
            doc: stream_def.doc.clone(),
            name: NamingConvention::method_name(stream_def.id.as_str()),
            item_type: self.java_boxed_type(&stream_def.item_type),
            pop_batch_items_expr: self.stream_pop_batch_items_expr(abi_stream),
            subscribe: abi_stream.subscribe.to_string(),
            poll: abi_stream.poll.to_string(),
            pop_batch: abi_stream.pop_batch.to_string(),
            wait: abi_stream.wait.to_string(),
            unsubscribe: abi_stream.unsubscribe.to_string(),
            free: abi_stream.free.to_string(),
            mode,
        }
    }

    fn stream_pop_batch_items_expr(&self, stream: &AbiStream) -> String {
        match &stream.item_transport {
            Transport::Scalar(origin) => self.scalar_stream_items_expr(origin),
            Transport::Composite(layout) => {
                let class_name = NamingConvention::class_name(layout.record_id.as_str());
                format!("{}.decodeBlittableVecFromRawBuffer(_bytes)", class_name)
            }
            _ => {
                let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
                let item_decode = self.emit_reader_read(decode_ops);
                format!("WireReader.readList(_bytes, _i -> {})", item_decode)
            }
        }
    }

    fn scalar_stream_items_expr(&self, origin: &ScalarOrigin) -> String {
        match origin {
            ScalarOrigin::Primitive(primitive) => match primitive {
                PrimitiveType::Bool => "WireReader.readPackedBools(_bytes)".to_string(),
                PrimitiveType::I8 | PrimitiveType::U8 => {
                    "WireReader.readPackedBytes(_bytes)".to_string()
                }
                PrimitiveType::I16 | PrimitiveType::U16 => {
                    "WireReader.readPackedShorts(_bytes)".to_string()
                }
                PrimitiveType::I32 | PrimitiveType::U32 => {
                    "WireReader.readPackedInts(_bytes)".to_string()
                }
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => "WireReader.readPackedLongs(_bytes)".to_string(),
                PrimitiveType::F32 => "WireReader.readPackedFloats(_bytes)".to_string(),
                PrimitiveType::F64 => "WireReader.readPackedDoubles(_bytes)".to_string(),
            },
            ScalarOrigin::CStyleEnum { enum_id, tag_type } => {
                let class_name = NamingConvention::class_name(enum_id.as_str());
                let read_method = match tag_type {
                    PrimitiveType::I8 | PrimitiveType::U8 => "readPackedBytes",
                    PrimitiveType::I16 | PrimitiveType::U16 => "readPackedShorts",
                    PrimitiveType::I32 | PrimitiveType::U32 => "readPackedInts",
                    _ => "readPackedLongs",
                };
                format!(
                    "WireReader.{}(_bytes).stream().map(v -> {}.fromValue(v)).collect(java.util.stream.Collectors.toList())",
                    read_method, class_name
                )
            }
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|s| s.class_id == class.id && s.stream_id == stream.id)
            .expect("abi stream not found")
    }

    fn vec_buffer_decode_expr(&self, inner: &TypeExpr, transport: Option<&Transport>) -> String {
        match transport {
            Some(Transport::Span(SpanContent::Scalar(origin))) => match origin {
                ScalarOrigin::Primitive(primitive) => vec_primitive_buffer_decode(*primitive),
                ScalarOrigin::CStyleEnum { enum_id, tag_type } => {
                    vec_c_style_enum_buffer_decode(enum_id.as_str(), *tag_type)
                }
            },
            Some(Transport::Span(SpanContent::Composite(layout))) => {
                format!(
                    "{}.decodeBlittableVecFromRawBuffer(_buf)",
                    NamingConvention::class_name(layout.record_id.as_str()),
                )
            }
            _ => match inner {
                TypeExpr::Primitive(primitive) => vec_primitive_buffer_decode(*primitive),
                TypeExpr::Record(id) => match self.ffi.catalog.resolve_record(id) {
                    Some(record) if record.is_blittable() => format!(
                        "{}.decodeBlittableVecFromRawBuffer(_buf)",
                        NamingConvention::class_name(id.as_str()),
                    ),
                    _ => panic!(
                        "unsupported direct Vec<Record> return transport for non-blittable record: {:?}",
                        id
                    ),
                },
                _ => panic!(
                    "unsupported direct Vec return transport for Java backend: {:?}",
                    inner
                ),
            },
        }
    }

    fn java_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Bytes => "byte[]".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Custom(id) => self.java_type(self.custom_repr_type(id)),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Callback(id) => self.callback_java_type(id),
            TypeExpr::Option(inner) => {
                format!("java.util.Optional<{}>", self.java_boxed_type(inner))
            }
            TypeExpr::Vec(inner) => self.java_vec_type(inner),
            _ => "Object".to_string(),
        }
    }

    fn java_vec_type(&self, inner: &TypeExpr) -> String {
        match inner {
            TypeExpr::Primitive(p) => mappings::java_primitive_array_type(*p).to_string(),
            TypeExpr::Custom(id) => self.java_vec_type(self.custom_repr_type(id)),
            _ => format!("java.util.List<{}>", self.java_boxed_type(inner)),
        }
    }

    fn java_boxed_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => mappings::java_boxed_type(*p).to_string(),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Bytes => "byte[]".to_string(),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Custom(id) => self.java_boxed_type(self.custom_repr_type(id)),
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Handle(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Callback(id) => self.callback_java_type(id),
            TypeExpr::Option(inner) => {
                format!("java.util.Optional<{}>", self.java_boxed_type(inner))
            }
            TypeExpr::Vec(inner) => self.java_vec_type(inner),
            _ => "Object".to_string(),
        }
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> JavaEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let kind = if enumeration.is_error {
            if self.uses_flat_error_enum_template(enumeration) {
                JavaEnumKind::Error
            } else {
                JavaEnumKind::ErrorAbstractClass
            }
        } else if abi_enum.is_c_style {
            JavaEnumKind::CStyle
        } else if self.options.min_java_version.supports_sealed()
            && !self.requires_manual_enum_value_semantics(abi_enum)
        {
            JavaEnumKind::SealedInterface
        } else {
            JavaEnumKind::AbstractClass
        };
        let value_type = match &enumeration.repr {
            EnumRepr::CStyle { tag_type, .. } | EnumRepr::Data { tag_type, .. } => {
                mappings::java_type(*tag_type).to_string()
            }
        };
        let variant_names: HashSet<String> = abi_enum
            .variants
            .iter()
            .map(|v| NamingConvention::class_name(v.name.as_str()))
            .collect();
        let variant_docs = enumeration.variant_docs();
        let variants = abi_enum
            .variants
            .iter()
            .enumerate()
            .map(|(ordinal, variant)| {
                self.lower_enum_variant(
                    abi_enum,
                    variant,
                    variant_docs.get(ordinal).cloned().flatten(),
                    ordinal,
                    kind,
                    &variant_names,
                )
            })
            .collect();
        let owner = JavaValueTypeDef::Enum(enumeration);
        let mut methods = self.lower_value_type_methods(owner);
        if matches!(enumeration.repr, EnumRepr::Data { .. }) {
            for method in &mut methods {
                self.qualify_collisions_in_type(&mut method.return_type, &variant_names);
                if let JavaReturnRender::Decode { decode_expr } = &mut method.return_plan.render {
                    self.qualify_colliding_names(decode_expr, &variant_names);
                    self.qualify_collisions_in_type(decode_expr, &variant_names);
                }
            }
        }
        JavaEnum {
            doc: enumeration.doc.clone(),
            class_name,
            kind,
            value_type,
            variants,
            constructors: self.lower_value_type_constructors(owner),
            methods,
        }
    }

    fn requires_manual_enum_value_semantics(&self, enumeration: &AbiEnum) -> bool {
        enumeration
            .variants
            .iter()
            .any(|variant| match &variant.payload {
                AbiEnumPayload::Unit => false,
                AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                    .iter()
                    .any(|field| self.contains_primitive_array_component(&field.type_expr)),
            })
    }

    fn lower_enum_variant(
        &self,
        abi_enum: &AbiEnum,
        variant: &AbiEnumVariant,
        doc: Option<String>,
        ordinal: usize,
        kind: JavaEnumKind,
        sibling_names: &HashSet<String>,
    ) -> JavaEnumVariant {
        let name = match kind {
            JavaEnumKind::CStyle | JavaEnumKind::Error => {
                NamingConvention::enum_constant_name(variant.name.as_str())
            }
            JavaEnumKind::SealedInterface
            | JavaEnumKind::AbstractClass
            | JavaEnumKind::ErrorAbstractClass => {
                NamingConvention::class_name(variant.name.as_str())
            }
        };
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|field| self.lower_enum_field(field, sibling_names))
                .collect(),
        };
        JavaEnumVariant {
            doc,
            name,
            tag: self.java_enum_variant_tag(abi_enum, kind, ordinal, variant.discriminant),
            fields,
        }
    }

    fn java_enum_variant_tag(
        &self,
        abi_enum: &AbiEnum,
        kind: JavaEnumKind,
        ordinal: usize,
        discriminant: i128,
    ) -> i128 {
        match kind {
            JavaEnumKind::CStyle => discriminant,
            JavaEnumKind::Error
            | JavaEnumKind::SealedInterface
            | JavaEnumKind::AbstractClass
            | JavaEnumKind::ErrorAbstractClass => match abi_enum.codec_tag_strategy {
                EnumTagStrategy::Discriminant => discriminant,
                EnumTagStrategy::OrdinalIndex => abi_enum.resolve_codec_tag(ordinal, discriminant),
            },
        }
    }

    fn lower_enum_field(
        &self,
        field: &AbiEnumField,
        sibling_names: &HashSet<String>,
    ) -> JavaEnumField {
        let field_name = NamingConvention::field_name(field.name.as_str());
        let equals_expr = self.value_equals_expr(
            &field.type_expr,
            &format!("this.{field_name}"),
            &format!("other.{field_name}"),
        );
        let hash_expr = self.value_hash_expr(&field.type_expr, field_name.as_str());
        let prefixed = Self::prefix_write_seq(&field.encode, "_v");
        let mut java_type = self.java_type(&field.type_expr);
        let mut decode_expr = self.emit_reader_read(&field.decode);
        let mut size_expr = self.emit_size_expr(&prefixed.size);
        let mut encode_expr = self.emit_write_expr(&prefixed, "wire");
        self.qualify_collisions_in_type(&mut java_type, sibling_names);
        self.qualify_colliding_names(&mut decode_expr, sibling_names);
        self.qualify_colliding_names(&mut size_expr, sibling_names);
        self.qualify_colliding_names(&mut encode_expr, sibling_names);
        JavaEnumField {
            doc: None,
            name: field_name,
            java_type,
            wire_decode_expr: decode_expr,
            wire_size_expr: size_expr,
            wire_encode_expr: encode_expr,
            equals_expr,
            hash_expr,
        }
    }

    fn qualify_colliding_names(&self, expr: &mut String, sibling_names: &HashSet<String>) {
        let static_call_suffixes = [
            ".decode(",
            ".wireEncodeTo(",
            ".wireEncodedSize(",
            ".decodeBlittableVec(",
            ".encodeBlittableVec(",
        ];
        for name in sibling_names {
            for suffix in static_call_suffixes {
                let pattern = format!("{name}{suffix}");
                if expr.contains(&pattern) {
                    let qualified = format!("{}.{name}{suffix}", self.package_name);
                    *expr = expr.replace(&pattern, &qualified);
                }
            }
        }
    }

    /// Rewrites a Java type string so any bare occurrence of a shadowed
    /// sibling name becomes fully qualified. Handles bare `Point`,
    /// `Optional<Point>`, `List<Point>`, `Map<Point, Foo>`, etc. Boundary
    /// detection uses type-syntax delimiters: a sibling name is
    /// "bare" when neither neighbor is a Java identifier character or
    /// `.`.
    fn qualify_collisions_in_type(&self, ty: &mut String, sibling_names: &HashSet<String>) {
        for name in sibling_names {
            let qualified = format!("{}.{name}", self.package_name);
            let mut search_from = 0;
            while let Some(idx_in_rest) = ty[search_from..].find(name.as_str()) {
                let idx = search_from + idx_in_rest;
                let end = idx + name.len();
                let left_ok = match ty[..idx].chars().next_back() {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_' && c != '.',
                };
                let right_ok = match ty[end..].chars().next() {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_',
                };
                if left_ok && right_ok {
                    ty.replace_range(idx..end, &qualified);
                    search_from = idx + qualified.len();
                } else {
                    search_from = end;
                }
            }
        }
    }

    fn prefix_value(value: &ValueExpr, binding: &str) -> ValueExpr {
        match value {
            ValueExpr::Instance => ValueExpr::Var(binding.to_string()),
            ValueExpr::Named(name) => ValueExpr::Field(
                Box::new(ValueExpr::Var(binding.to_string())),
                FieldName::new(name),
            ),
            ValueExpr::Var(_) => value.clone(),
            ValueExpr::Field(parent, field) => {
                ValueExpr::Field(Box::new(Self::prefix_value(parent, binding)), field.clone())
            }
        }
    }

    fn prefix_write_op(op: &WriteOp, binding: &str) -> WriteOp {
        match op {
            WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
                primitive: *primitive,
                value: Self::prefix_value(value, binding),
            },
            WriteOp::String { value } => WriteOp::String {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Bytes { value } => WriteOp::Bytes {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Option { value, some } => WriteOp::Option {
                value: Self::prefix_value(value, binding),
                some: Box::new(Self::prefix_write_seq(some, binding)),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                fields: fields
                    .iter()
                    .map(|f| FieldWriteOp {
                        name: f.name.clone(),
                        accessor: Self::prefix_value(&f.accessor, binding),
                        seq: Self::prefix_write_seq(&f.seq, binding),
                    })
                    .collect(),
            },
            WriteOp::Enum { id, value, layout } => WriteOp::Enum {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                layout: layout.clone(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: Self::prefix_value(value, binding),
                ok: Box::new(Self::prefix_write_seq(ok, binding)),
                err: Box::new(Self::prefix_write_seq(err, binding)),
            },
            WriteOp::Builtin { id, value } => WriteOp::Builtin {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Custom {
                id,
                value,
                underlying,
            } => WriteOp::Custom {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                underlying: Box::new(Self::prefix_write_seq(underlying, binding)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: Self::prefix_value(value, binding),
                element_type: element_type.clone(),
                element: element.clone(),
                layout: layout.clone(),
            },
        }
    }

    fn prefix_size_expr(size: &SizeExpr, binding: &str) -> SizeExpr {
        match size {
            SizeExpr::Fixed(_) | SizeExpr::Runtime => size.clone(),
            SizeExpr::StringLen(v) => SizeExpr::StringLen(Self::prefix_value(v, binding)),
            SizeExpr::BytesLen(v) => SizeExpr::BytesLen(Self::prefix_value(v, binding)),
            SizeExpr::ValueSize(v) => SizeExpr::ValueSize(Self::prefix_value(v, binding)),
            SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
            },
            SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                owner: owner.clone(),
            },
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: Self::prefix_value(value, binding),
                inner: Box::new(Self::prefix_size_expr(inner, binding)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: Self::prefix_value(value, binding),
                inner: Box::new(Self::prefix_size_expr(inner, binding)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: Self::prefix_value(value, binding),
                ok: Box::new(Self::prefix_size_expr(ok, binding)),
                err: Box::new(Self::prefix_size_expr(err, binding)),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|p| Self::prefix_size_expr(p, binding))
                    .collect(),
            ),
        }
    }

    fn prefix_write_seq(seq: &WriteSeq, binding: &str) -> WriteSeq {
        WriteSeq {
            size: Self::prefix_size_expr(&seq.size, binding),
            ops: seq
                .ops
                .iter()
                .map(|op| Self::prefix_write_op(op, binding))
                .collect(),
            shape: seq.shape,
        }
    }

    fn abi_enum_for(&self, enumeration: &EnumDef) -> &AbiEnum {
        self.abi
            .enums
            .iter()
            .find(|abi_enum| abi_enum.id == enumeration.id)
            .expect("abi enum missing")
    }

    fn lower_closure(&self, callback: &CallbackTraitDef) -> JavaClosureInterface {
        let method = callback
            .methods
            .first()
            .expect("closure must have a method");
        let abi_callback = self
            .abi_callback_for(&callback.id)
            .expect("closure abi callback missing");
        let abi_method = abi_callback
            .methods
            .iter()
            .find(|candidate| candidate.id == method.id)
            .expect("closure abi method missing");
        let interface_name = self.callback_java_type(&callback.id);
        let callbacks_class_name = self.callback_bridge_name(&callback.id);
        let params = self.lower_bridge_params(&method.params, abi_method);
        let return_info = self.lower_bridge_return(&method.returns, &abi_method.returns);
        let proxy = self.lower_closure_proxy_method(callback, method, abi_method);
        JavaClosureInterface {
            doc: callback.doc.clone(),
            invoke_doc: method.doc.clone(),
            interface_name,
            callback_id: callback.id.as_str().to_string(),
            callbacks_class_name,
            supports_proxy_wrap: false,
            supports_cleaner: self.options.min_java_version.supports_cleaner(),
            params,
            return_info,
            proxy,
        }
    }

    fn lower_callback_trait(&self, callback: &CallbackTraitDef) -> JavaCallbackTrait {
        let interface_name = NamingConvention::class_name(callback.id.as_str());
        let abi_callback = self
            .abi_callback_for(&callback.id)
            .expect("callback abi invocation missing");

        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| method.execution_kind() == ExecutionKind::Sync)
            .filter_map(|method| {
                let abi_method = abi_callback
                    .methods
                    .iter()
                    .find(|candidate| candidate.id == method.id)?;
                Some(self.lower_sync_callback_method(callback, method, abi_method))
            })
            .collect();

        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.execution_kind() == ExecutionKind::Async)
            .filter_map(|method| {
                let abi_method = abi_callback
                    .methods
                    .iter()
                    .find(|candidate| candidate.id == method.id)?;
                Some(self.lower_async_callback_method(callback, method, abi_method))
            })
            .collect();

        JavaCallbackTrait {
            doc: callback.doc.clone(),
            interface_name,
            callback_id: callback.id.as_str().to_string(),
            supports_cleaner: self.options.min_java_version.supports_cleaner(),
            sync_methods,
            async_methods,
        }
    }

    fn lower_sync_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaSyncCallbackMethod {
        let proxy = self.lower_sync_callback_proxy_method(callback, method, abi_method);
        let return_info = self.lower_bridge_return(&method.returns, &abi_method.returns);
        JavaSyncCallbackMethod {
            doc: method.doc.clone(),
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            params: self.lower_bridge_params(&method.params, abi_method),
            return_info,
            proxy,
        }
    }

    fn lower_async_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaAsyncCallbackMethod {
        let proxy = self.lower_async_callback_proxy_method(callback, method, abi_method);
        JavaAsyncCallbackMethod {
            doc: method.doc.clone(),
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            params: self.lower_bridge_params(&method.params, abi_method),
            return_info: self.lower_bridge_return(&method.returns, &abi_method.returns),
            invoker_suffix: self.invoker_suffix_from_return_shape(&abi_method.returns),
            proxy,
        }
    }

    fn lower_sync_callback_proxy_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaCallbackProxySyncMethod {
        let wire_writers = self.callback_proxy_wire_writers(method, abi_method);
        let params = self.lower_callback_proxy_params(method, abi_method, &wire_writers);
        let return_type = self
            .lower_bridge_return(&method.returns, &abi_method.returns)
            .as_ref()
            .map(|return_info| return_info.java_type().to_string())
            .unwrap_or_else(|| "void".to_string());
        JavaCallbackProxySyncMethod {
            native_name: self.callback_proxy_native_name(callback, method),
            params,
            return_type,
            return_plan: self.return_plan_for_shape(
                &method.returns,
                &abi_method.returns,
                &abi_method.error,
            ),
            wire_writers,
        }
    }

    fn lower_closure_proxy_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaCallbackProxySyncMethod {
        let wire_writers = self.callback_proxy_wire_writers(method, abi_method);
        let params = self.lower_callback_proxy_params(method, abi_method, &wire_writers);
        let return_type = self
            .lower_bridge_return(&method.returns, &abi_method.returns)
            .as_ref()
            .map(|return_info| return_info.java_type().to_string())
            .unwrap_or_else(|| "void".to_string());
        JavaCallbackProxySyncMethod {
            native_name: self.closure_proxy_native_name(callback),
            params,
            return_type,
            return_plan: self.return_plan_for_shape(
                &method.returns,
                &abi_method.returns,
                &abi_method.error,
            ),
            wire_writers,
        }
    }

    fn lower_async_callback_proxy_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> JavaCallbackProxyAsyncMethod {
        let wire_writers = self.callback_proxy_wire_writers(method, abi_method);
        let params = self.lower_callback_proxy_params(method, abi_method, &wire_writers);
        let method_pascal = NamingConvention::class_name(method.id.as_str());
        JavaCallbackProxyAsyncMethod {
            native_name: self.callback_proxy_native_name(callback, method),
            params,
            return_type: self.return_java_type(&method.returns),
            return_plan: self.return_plan_for_shape(
                &method.returns,
                &abi_method.returns,
                &abi_method.error,
            ),
            wire_writers,
            success_name: format!("complete{}", method_pascal),
            failure_name: format!("fail{}", method_pascal),
        }
    }

    fn callback_proxy_wire_writers(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> Vec<JavaWireWriter> {
        let abi_params = self.callback_input_abi_params(method, abi_method);
        method
            .params
            .iter()
            .filter_map(|param| {
                let abi_param = abi_params.get(param.name.as_str())?;
                self.input_write_ops(abi_param).map(|encode_ops| {
                    let param_name = param.name.as_str().to_string();
                    let binding_name = format!("_wire_{}", param.name.as_str());
                    let encode_expr = self.emit_write_expr(&encode_ops, &binding_name);
                    JavaWireWriter {
                        binding_name,
                        param_name,
                        size_expr: self.emit_size_expr(&encode_ops.size),
                        encode_expr,
                    }
                })
            })
            .collect()
    }

    fn lower_callback_proxy_params(
        &self,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
        wire_writers: &[JavaWireWriter],
    ) -> Vec<JavaParam> {
        let abi_params = self.callback_input_abi_params(method, abi_method);
        let input_bindings = JavaInputBindings {
            direct_composites: Vec::new(),
            wire_writers: wire_writers.to_vec(),
        };
        method
            .params
            .iter()
            .filter_map(|param| {
                let abi_param = abi_params.get(param.name.as_str())?;
                Some(self.lower_native_param(
                    param.name.as_str(),
                    &param.type_expr,
                    &NamingConvention::field_name(param.name.as_str()),
                    abi_param.transport(),
                    &input_bindings,
                ))
            })
            .collect()
    }

    fn callback_input_abi_params<'b>(
        &self,
        method: &'b CallbackMethodDef,
        abi_method: &'b AbiCallbackMethod,
    ) -> HashMap<&'b str, &'b AbiParam> {
        let abi_params = abi_method
            .params
            .iter()
            .filter_map(|param| match &param.role {
                ParamRole::Input { .. } => Some((param.name.as_str(), param)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();

        method
            .params
            .iter()
            .filter_map(|param| {
                abi_params
                    .get(param.name.as_str())
                    .copied()
                    .map(|abi_param| (param.name.as_str(), abi_param))
            })
            .collect()
    }

    fn lower_bridge_params(
        &self,
        params: &[ParamDef],
        abi_method: &AbiCallbackMethod,
    ) -> Vec<JavaBridgeParam> {
        let abi_params = abi_method
            .params
            .iter()
            .filter_map(|param| match &param.role {
                ParamRole::Input { .. } => Some((param.name.as_str(), param)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();

        params
            .iter()
            .filter_map(|param| {
                let abi_param = abi_params.get(param.name.as_str())?;
                Some(self.lower_bridge_param(param, abi_param))
            })
            .collect()
    }

    fn lower_bridge_param(&self, param: &ParamDef, abi_param: &AbiParam) -> JavaBridgeParam {
        let name = NamingConvention::field_name(param.name.as_str());
        let java_type = self.java_type(&param.type_expr);

        match &abi_param.role {
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => JavaBridgeParam {
                name: name.clone(),
                java_type,
                jni_type: self.callback_direct_param_jni_type(abi_param),
                decode_expr: self.callback_direct_param_decode_expr(param, abi_param, &name),
            },
            ParamRole::Input {
                decode_ops: Some(decode_ops),
                ..
            } => JavaBridgeParam {
                name: name.clone(),
                java_type,
                jni_type: "java.nio.ByteBuffer".to_string(),
                decode_expr: self.callback_encoded_param_decode_expr(decode_ops, &name),
            },
            other => panic!("unsupported Java callback param role: {:?}", other),
        }
    }

    fn callback_direct_param_jni_type(&self, abi_param: &AbiParam) -> String {
        match &abi_param.abi_type {
            AbiType::Bool => "boolean".to_string(),
            AbiType::I8 | AbiType::U8 => "byte".to_string(),
            AbiType::I16 | AbiType::U16 => "short".to_string(),
            AbiType::I32 | AbiType::U32 => "int".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => "long".to_string(),
            AbiType::F32 => "float".to_string(),
            AbiType::F64 => "double".to_string(),
            other => panic!("unsupported Java scalar callback ABI type: {:?}", other),
        }
    }

    fn callback_direct_param_decode_expr(
        &self,
        param: &ParamDef,
        abi_param: &AbiParam,
        name: &str,
    ) -> String {
        match (&param.type_expr, &abi_param.role) {
            (
                TypeExpr::Enum(enum_id),
                ParamRole::Input {
                    transport: Transport::Scalar(ScalarOrigin::CStyleEnum { .. }),
                    ..
                },
            ) => format!(
                "{}.fromValue({})",
                NamingConvention::class_name(enum_id.as_str()),
                name
            ),
            _ => name.to_string(),
        }
    }

    fn callback_encoded_param_decode_expr(&self, decode_ops: &ReadSeq, name: &str) -> String {
        let decode_expr = self.emit_reader_read(decode_ops);
        format!(
            "WireReader.decodeBuffer({}, reader -> {})",
            name, decode_expr
        )
    }

    fn lower_bridge_return(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
    ) -> Option<JavaBridgeReturn> {
        match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(JavaBridgeReturn::Value(
                self.lower_value_bridge_return(ty, ret_shape),
            )),
            ReturnDef::Result { ok, err } => Some(JavaBridgeReturn::Result(
                self.lower_result_bridge_return(ok, err, ret_shape),
            )),
        }
    }

    fn lower_value_bridge_return(
        &self,
        ty: &TypeExpr,
        ret_shape: &ReturnShape,
    ) -> JavaValueBridgeReturn {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => unreachable!("value return requested for void callback"),
            ValueReturnStrategy::Scalar(strategy) => {
                let Some(transport) = &ret_shape.transport else {
                    unreachable!("scalar callback return must have transport");
                };
                let primitive = match transport {
                    Transport::Scalar(origin) => origin.primitive(),
                    other => unreachable!(
                        "scalar callback return must use scalar transport: {:?}",
                        other
                    ),
                };
                let suffix = match strategy {
                    ScalarReturnStrategy::PrimitiveValue => String::new(),
                    ScalarReturnStrategy::CStyleEnumTag => ".value".to_string(),
                };
                JavaValueBridgeReturn {
                    java_type: self.java_type(ty),
                    jni_type: mappings::java_type(primitive).to_string(),
                    default_value: mappings::java_default_value(primitive).to_string(),
                    render: JavaValueBridgeRender::Direct {
                        prefix: String::new(),
                        suffix,
                    },
                }
            }
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                let encode_ops = ret_shape
                    .encode_ops
                    .as_ref()
                    .expect("encoded callback return should provide encode ops");
                JavaValueBridgeReturn {
                    java_type: self.java_type(ty),
                    jni_type: "byte[]".to_string(),
                    default_value: "new byte[0]".to_string(),
                    render: self.encoded_return_render(encode_ops, "value"),
                }
            }
            ValueReturnStrategy::ObjectHandle => JavaValueBridgeReturn {
                java_type: self.java_type(ty),
                jni_type: "long".to_string(),
                default_value: "0L".to_string(),
                render: JavaValueBridgeRender::Direct {
                    prefix: String::new(),
                    suffix: ".handle".to_string(),
                },
            },
            ValueReturnStrategy::CallbackHandle => {
                let Some(Transport::Callback { callback_id, .. }) = &ret_shape.transport else {
                    unreachable!("callback handle return must use callback transport");
                };
                JavaValueBridgeReturn {
                    java_type: self.java_type(ty),
                    jni_type: "long".to_string(),
                    default_value: "0L".to_string(),
                    render: JavaValueBridgeRender::Direct {
                        prefix: format!("{}.create(", self.callback_bridge_name(callback_id)),
                        suffix: ")".to_string(),
                    },
                }
            }
        }
    }

    fn lower_result_bridge_return(
        &self,
        ok: &TypeExpr,
        err: &TypeExpr,
        ret_shape: &ReturnShape,
    ) -> JavaResultBridgeReturn {
        let encode_ops = ret_shape
            .encode_ops
            .as_ref()
            .expect("result callback return should provide encode ops");
        let encoded_render = self.encoded_return_render(encode_ops, "result");
        let JavaValueBridgeRender::Encode {
            size_expr,
            encode_expr,
        } = encoded_render
        else {
            unreachable!("result callback encoding should always be wire encoded");
        };

        JavaResultBridgeReturn {
            ok_java_type: self.java_boxed_type(ok),
            err_java_type: self.java_boxed_type(err),
            jni_type: "byte[]".to_string(),
            default_value: "new byte[0]".to_string(),
            encode_size_expr: size_expr,
            encode_expr,
            error_capture: self.callback_error_capture(err),
        }
    }

    fn encoded_return_render(&self, encode_ops: &WriteSeq, binding: &str) -> JavaValueBridgeRender {
        let remapped = remap_root_in_seq(encode_ops, ValueExpr::Var(binding.to_string()));
        JavaValueBridgeRender::Encode {
            size_expr: self.emit_size_expr(&self.write_seq_size_expr(&remapped)),
            encode_expr: self.emit_write_expr(&remapped, "wire"),
        }
    }

    fn write_seq_size_expr(&self, encode_ops: &WriteSeq) -> SizeExpr {
        self.normalize_custom_write_seq(encode_ops).size
    }

    fn callback_error_capture(&self, err: &TypeExpr) -> JavaCallbackErrorCapture {
        let exception_class = match err {
            TypeExpr::Enum(id) => self
                .ffi
                .catalog
                .resolve_enum(id)
                .filter(|enumeration| enumeration.is_error)
                .map(|_| format!("{}.Exception", NamingConvention::class_name(id.as_str()))),
            TypeExpr::Record(id) if self.is_error_record(id) => {
                Some(NamingConvention::class_name(id.as_str()))
            }
            _ => None,
        };

        JavaCallbackErrorCapture {
            exception_class,
            is_string: matches!(err, TypeExpr::String),
        }
    }

    fn collect_async_callback_invokers(
        &self,
        callbacks: &[JavaCallbackTrait],
    ) -> Vec<JavaAsyncCallbackInvoker> {
        let mut seen = HashSet::new();
        callbacks
            .iter()
            .flat_map(|callback| callback.async_methods.iter())
            .filter_map(|method| {
                if seen.insert(method.invoker_suffix.clone()) {
                    Some(JavaAsyncCallbackInvoker {
                        suffix: method.invoker_suffix.clone(),
                        result_jni_type: method
                            .return_info
                            .as_ref()
                            .map(|return_info| return_info.jni_type().to_string()),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn invoker_suffix_from_return_shape(&self, ret_shape: &ReturnShape) -> String {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => "Void".to_string(),
            ValueReturnStrategy::Scalar(_) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar callback return must use scalar transport");
                };
                self.invoker_suffix_from_primitive(origin.primitive())
            }
            ValueReturnStrategy::ObjectHandle => "Handle".to_string(),
            ValueReturnStrategy::CallbackHandle => {
                let Some(Transport::Callback { callback_id, .. }) = &ret_shape.transport else {
                    unreachable!("callback handle return must use callback transport");
                };
                format!(
                    "CallbackHandle{}",
                    NamingConvention::class_name(callback_id.as_str())
                )
            }
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                "Wire".to_string()
            }
        }
    }

    fn invoker_suffix_from_primitive(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Bool".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "I8".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "I16".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "I32".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "I64".to_string(),
            PrimitiveType::F32 => "F32".to_string(),
            PrimitiveType::F64 => "F64".to_string(),
        }
    }

    fn callback_java_type(&self, callback_id: &CallbackId) -> String {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                format!("Closure{}", signature_id)
            }
            Some(_) => NamingConvention::class_name(callback_id.as_str()),
            None => "Object".to_string(),
        }
    }

    fn callback_bridge_name(&self, callback_id: &CallbackId) -> String {
        let callback = self.ffi.catalog.resolve_callback(callback_id);
        match callback {
            Some(cb) if matches!(cb.kind, CallbackKind::Closure) => {
                let signature_id = callback_id
                    .as_str()
                    .strip_prefix("__Closure_")
                    .unwrap_or(callback_id.as_str());
                format!("Closure{}Callbacks", signature_id)
            }
            Some(_) => format!(
                "{}Callbacks",
                NamingConvention::class_name(callback_id.as_str())
            ),
            None => "Object".to_string(),
        }
    }

    fn callback_proxy_native_name(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> String {
        format!(
            "boltffiCallback{}{}",
            NamingConvention::class_name(callback.id.as_str()),
            NamingConvention::class_name(method.id.as_str())
        )
    }

    fn closure_proxy_native_name(&self, callback: &CallbackTraitDef) -> String {
        format!("boltffi{}", self.callback_bridge_name(&callback.id))
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> Option<&AbiCallbackInvocation> {
        self.abi
            .callbacks
            .iter()
            .find(|cb| cb.callback_id == *callback_id)
    }
}

fn vec_primitive_buffer_decode(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "WireReader.booleanArrayFromRawBuffer(_buf)".to_string(),
        PrimitiveType::I8 | PrimitiveType::U8 => "_buf != null ? _buf : new byte[0]".to_string(),
        PrimitiveType::I16 | PrimitiveType::U16 => {
            "WireReader.shortArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            "WireReader.intArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            "WireReader.longArrayFromRawBuffer(_buf)".to_string()
        }
        PrimitiveType::F32 => "WireReader.floatArrayFromRawBuffer(_buf)".to_string(),
        PrimitiveType::F64 => "WireReader.doubleArrayFromRawBuffer(_buf)".to_string(),
    }
}

fn java_blittable_decode_expr(primitive: PrimitiveType, const_name: &str) -> String {
    let offset = format!("OFFSET_{}", const_name);
    match primitive {
        PrimitiveType::Bool => format!("buf.get(base + {}) != 0", offset),
        PrimitiveType::I8 | PrimitiveType::U8 => format!("buf.get(base + {})", offset),
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("buf.getShort(base + {})", offset)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("buf.getInt(base + {})", offset)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("buf.getLong(base + {})", offset)
        }
        PrimitiveType::F32 => format!("buf.getFloat(base + {})", offset),
        PrimitiveType::F64 => format!("buf.getDouble(base + {})", offset),
    }
}

fn java_blittable_encode_expr(
    primitive: PrimitiveType,
    const_name: &str,
    field_name: &str,
) -> String {
    let offset = format!("OFFSET_{}", const_name);
    let accessor = format!("item.{}()", field_name);
    match primitive {
        PrimitiveType::Bool => {
            format!("buf.put(base + {}, (byte) ({} ? 1 : 0))", offset, accessor)
        }
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!("buf.put(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("buf.putShort(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("buf.putInt(base + {}, {})", offset, accessor)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("buf.putLong(base + {}, {})", offset, accessor)
        }
        PrimitiveType::F32 => {
            format!("buf.putFloat(base + {}, {})", offset, accessor)
        }
        PrimitiveType::F64 => {
            format!("buf.putDouble(base + {}, {})", offset, accessor)
        }
    }
}

impl JavaNativeParam {
    fn from_param(param: &JavaParam) -> Self {
        Self {
            name: param.name.clone(),
            native_type: param.native_type.clone(),
            expr: param.native_expr.clone(),
        }
    }
}

impl JavaValueTypeConstructor {
    fn lower(
        lowerer: &JavaLowerer<'_>,
        owner: JavaValueTypeDef<'_>,
        constructor: &ConstructorDef,
        call: &AbiCall,
    ) -> Self {
        let input_bindings = lowerer.input_bindings_for_params(call);
        let params = constructor
            .params()
            .iter()
            .map(|param_def| {
                lowerer.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    call,
                    &input_bindings,
                )
            })
            .collect::<Vec<_>>();
        let native_params = params.iter().map(JavaNativeParam::from_param).collect();

        let owner_type = owner.type_expr();
        let constructor_return = if constructor.is_fallible() {
            ReturnDef::Result {
                ok: owner_type,
                err: TypeExpr::String,
            }
        } else if constructor.is_optional() {
            ReturnDef::Value(TypeExpr::Option(Box::new(owner_type)))
        } else {
            ReturnDef::Value(owner_type)
        };

        Self {
            doc: constructor.doc().map(str::to_string),
            name: NamingConvention::method_name(
                constructor
                    .name()
                    .expect("value type constructors must be named")
                    .as_str(),
            ),
            params,
            native_params,
            return_type: lowerer.return_java_type(&constructor_return),
            return_plan: lowerer.return_plan(&constructor_return, call),
            input_bindings,
            ffi_name: call.symbol.as_str().to_string(),
        }
    }
}

impl JavaValueTypeMethod {
    fn lower(
        lowerer: &JavaLowerer<'_>,
        owner: JavaValueTypeDef<'_>,
        method: &MethodDef,
        call: &AbiCall,
    ) -> Self {
        let call_without_self = JavaLowerer::strip_value_self_param(call);
        let input_bindings = lowerer.input_bindings_for_params(&call_without_self);
        let params = method
            .params
            .iter()
            .map(|param_def| {
                lowerer.lower_param(
                    param_def.name.as_str(),
                    &param_def.type_expr,
                    &call_without_self,
                    &input_bindings,
                )
            })
            .collect::<Vec<_>>();

        let mut native_params = Vec::new();
        let mut all_input_bindings = JavaInputBindings::default();
        if method.receiver != Receiver::Static {
            if let Some(self_native_param) = lowerer.lower_self_native_param(call) {
                native_params.push(self_native_param);
            }
            all_input_bindings = lowerer.lower_self_input_bindings(call);
        }
        native_params.extend(params.iter().map(JavaNativeParam::from_param));
        all_input_bindings
            .direct_composites
            .extend(input_bindings.direct_composites);
        all_input_bindings
            .wire_writers
            .extend(input_bindings.wire_writers);

        let mutating_void =
            method.receiver == Receiver::RefMutSelf && matches!(method.returns, ReturnDef::Void);
        let (return_type, return_plan) = if mutating_void {
            let owner_type = owner.type_expr();
            (
                lowerer.java_type(&owner_type),
                lowerer.java_return_plan_for_value(&owner_type, &call_without_self),
            )
        } else {
            (
                lowerer.return_java_type(&method.returns),
                lowerer.return_plan(&method.returns, &call_without_self),
            )
        };

        Self {
            doc: method.doc.clone(),
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: call.symbol.as_str().to_string(),
            is_static: method.callable_form() == CallableForm::StaticMethod,
            params,
            native_params,
            return_type,
            return_plan,
            input_bindings: all_input_bindings,
        }
    }
}

fn vec_c_style_enum_buffer_decode(enum_id: &str, tag_type: PrimitiveType) -> String {
    let class_name = NamingConvention::class_name(enum_id);
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!(
                "WireReader.mapByteArray(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!(
                "WireReader.mapShortRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!(
                "WireReader.mapIntRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!(
                "WireReader.mapLongRawBuffer(_buf, value -> {}.fromValue(value))",
                class_name,
            )
        }
        _ => panic!(
            "unsupported C-style enum tag type for Vec decode: {:?}",
            tag_type
        ),
    }
}

fn vec_c_style_enum_param_encode_expr(name: &str, tag_type: PrimitiveType) -> String {
    match tag_type {
        PrimitiveType::I8 | PrimitiveType::U8 => {
            format!("WireReader.toByteArray({}, item -> item.value)", name)
        }
        PrimitiveType::I16 | PrimitiveType::U16 => {
            format!("WireReader.toShortArray({}, item -> item.value)", name)
        }
        PrimitiveType::I32 | PrimitiveType::U32 => {
            format!("WireReader.toIntArray({}, item -> item.value)", name)
        }
        PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
            format!("WireReader.toLongArray({}, item -> item.value)", name)
        }
        _ => panic!(
            "unsupported C-style enum tag type for Vec param encode: {:?}",
            tag_type
        ),
    }
}

fn vec_blittable_record_param_encode_expr(record_id: &RecordId, name: &str) -> String {
    format!(
        "{}.encodeBlittableVecInput({})",
        NamingConvention::class_name(record_id.as_str()),
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef,
        CustomTypeDef, DataVariant, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef,
        ParamPassing, Receiver, RecordDef, ReturnDef, VariantPayload,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, ConverterPath, CustomTypeId, EnumId, FieldName, FunctionId, MethodId,
        ParamName, QualifiedName, RecordId, VariantName,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use crate::render::java::JavaVersion;

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

    fn lower(contract: &FfiContract) -> JavaModule {
        lower_with_version(contract, JavaVersion::JAVA_8)
    }

    fn lower_with_version(contract: &FfiContract, version: JavaVersion) -> JavaModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        let options = JavaOptions {
            library_name: None,
            min_java_version: version,
            desktop_loader: true,
        };
        JavaLowerer::new(
            contract,
            &abi,
            "com.test".to_string(),
            "test".to_string(),
            options,
        )
        .module()
    }

    fn record_def(id: &str, fields: Vec<FieldDef>) -> RecordDef {
        RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new(id),
            fields,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn custom_type_def(id: &str, repr: TypeExpr) -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new(id),
            rust_type: QualifiedName::new(format!("crate::{id}")),
            repr,
            converters: ConverterPath {
                into_ffi: QualifiedName::new(format!("crate::{id}::into_ffi")),
                try_from_ffi: QualifiedName::new(format!("crate::{id}::try_from_ffi")),
            },
            doc: None,
        }
    }

    fn field(name: &str, ty: TypeExpr) -> FieldDef {
        FieldDef {
            name: FieldName::new(name),
            type_expr: ty,
            doc: None,
            default: None,
        }
    }

    fn param(name: &str, ty: TypeExpr) -> ParamDef {
        ParamDef {
            name: ParamName::new(name),
            type_expr: ty,
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    fn function(id: &str, params: Vec<ParamDef>, returns: ReturnDef) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(id),
            params,
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn callback_method(
        id: &str,
        params: Vec<ParamDef>,
        returns: ReturnDef,
        execution_kind: ExecutionKind,
    ) -> CallbackMethodDef {
        CallbackMethodDef {
            id: MethodId::new(id),
            params,
            returns,
            execution_kind,
            doc: None,
        }
    }

    #[test]
    fn primitive_type_mapping() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "AllPrimitives",
            vec![
                field("a", TypeExpr::Primitive(PrimitiveType::Bool)),
                field("b", TypeExpr::Primitive(PrimitiveType::I8)),
                field("c", TypeExpr::Primitive(PrimitiveType::U8)),
                field("d", TypeExpr::Primitive(PrimitiveType::I16)),
                field("e", TypeExpr::Primitive(PrimitiveType::U16)),
                field("f", TypeExpr::Primitive(PrimitiveType::I32)),
                field("g", TypeExpr::Primitive(PrimitiveType::U32)),
                field("h", TypeExpr::Primitive(PrimitiveType::I64)),
                field("i", TypeExpr::Primitive(PrimitiveType::U64)),
                field("j", TypeExpr::Primitive(PrimitiveType::F32)),
                field("k", TypeExpr::Primitive(PrimitiveType::F64)),
                field("l", TypeExpr::Primitive(PrimitiveType::ISize)),
                field("m", TypeExpr::Primitive(PrimitiveType::USize)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[0].java_type, "boolean");
        assert_eq!(record.fields[1].java_type, "byte");
        assert_eq!(record.fields[2].java_type, "byte");
        assert_eq!(record.fields[3].java_type, "short");
        assert_eq!(record.fields[4].java_type, "short");
        assert_eq!(record.fields[5].java_type, "int");
        assert_eq!(record.fields[6].java_type, "int");
        assert_eq!(record.fields[7].java_type, "long");
        assert_eq!(record.fields[8].java_type, "long");
        assert_eq!(record.fields[9].java_type, "float");
        assert_eq!(record.fields[10].java_type, "double");
        assert_eq!(record.fields[11].java_type, "long");
        assert_eq!(record.fields[12].java_type, "long");
    }

    #[test]
    fn string_field_type() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Named", vec![field("name", TypeExpr::String)]));

        let module = lower(&contract);
        assert_eq!(module.records[0].fields[0].java_type, "String");
    }

    #[test]
    fn bytes_field_type() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        assert_eq!(module.records[0].fields[0].java_type, "byte[]");
    }

    #[test]
    fn option_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeValue",
            vec![
                field(
                    "count",
                    TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                ),
                field("label", TypeExpr::Option(Box::new(TypeExpr::String))),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].java_type, "java.util.Optional<Integer>");
        assert_eq!(record.fields[1].java_type, "java.util.Optional<String>");
    }

    #[test]
    fn vec_primitive_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Buffers",
            vec![
                field(
                    "ints",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                ),
                field(
                    "longs",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I64))),
                ),
                field(
                    "bytes",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::U8))),
                ),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].java_type, "int[]");
        assert_eq!(record.fields[1].java_type, "long[]");
        assert_eq!(record.fields[2].java_type, "byte[]");
    }

    #[test]
    fn vec_string_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Tags",
            vec![field("values", TypeExpr::Vec(Box::new(TypeExpr::String)))],
        ));

        let module = lower(&contract);
        assert_eq!(
            module.records[0].fields[0].java_type,
            "java.util.List<String>"
        );
    }

    #[test]
    fn record_field_names_are_camel_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Config",
            vec![
                field("max_connections", TypeExpr::Primitive(PrimitiveType::I32)),
                field("timeout_ms", TypeExpr::Primitive(PrimitiveType::U64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert_eq!(record.fields[0].name, "maxConnections");
        assert_eq!(record.fields[1].name, "timeoutMs");
    }

    #[test]
    fn record_class_name_is_pascal_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "my_point",
            vec![field("x", TypeExpr::Primitive(PrimitiveType::F64))],
        ));

        let module = lower(&contract);
        assert_eq!(module.records[0].class_name, "MyPoint");
    }

    #[test]
    fn blittable_record_detected() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Point",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert!(record.is_blittable());
        let layout = record.blittable_layout.as_ref().unwrap();
        assert_eq!(layout.struct_size, 16);
        assert_eq!(layout.fields.len(), 2);
    }

    #[test]
    fn non_blittable_record_with_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "User",
            vec![
                field("id", TypeExpr::Primitive(PrimitiveType::I64)),
                field("name", TypeExpr::String),
            ],
        ));

        let module = lower(&contract);
        assert!(!module.records[0].is_blittable());
    }

    #[test]
    fn record_shape_classic_on_java8() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Simple",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_native_on_java17_without_primitive_arrays() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Simple",
            vec![
                field("value", TypeExpr::Primitive(PrimitiveType::I32)),
                field("name", TypeExpr::String),
            ],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::NativeRecord);
    }

    #[test]
    fn record_shape_classic_on_java17_with_primitive_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithArray",
            vec![field(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_classic_on_java17_with_bytes() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithBytes",
            vec![field("data", TypeExpr::Bytes)],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_classic_on_java17_with_nested_primitive_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Nested",
            vec![field(
                "matrix",
                TypeExpr::Option(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::ClassicClass);
    }

    #[test]
    fn record_shape_native_on_java17_with_vec_of_strings() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "WithStringList",
            vec![field("tags", TypeExpr::Vec(Box::new(TypeExpr::String)))],
        ));

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert_eq!(module.records[0].shape, JavaRecordShape::NativeRecord);
    }

    #[test]
    fn bytes_equality_uses_arrays_equals() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Arrays.equals("),
            "expected Arrays.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn bytes_hash_uses_arrays_hash_code() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Buffer", vec![field("data", TypeExpr::Bytes)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.hash_expr.contains("java.util.Arrays.hashCode("),
            "expected Arrays.hashCode, got: {}",
            field.hash_expr
        );
    }

    #[test]
    fn primitive_array_equality_uses_arrays_equals() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Scores",
            vec![field(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Arrays.equals("),
            "expected Arrays.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn string_equality_uses_objects_equals() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_record(record_def("Named", vec![field("name", TypeExpr::String)]));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("java.util.Objects.equals("),
            "expected Objects.equals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn primitive_equality_uses_direct_comparison() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Counter",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("=="),
            "expected ==, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn float_equality_uses_compare() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Coords",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F32)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));

        let module = lower(&contract);
        let record = &module.records[0];
        assert!(record.fields[0].equals_expr.contains("Float.compare("));
        assert!(record.fields[1].equals_expr.contains("Double.compare("));
    }

    #[test]
    fn option_equality_checks_presence() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeValue",
            vec![field(
                "value",
                TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("isPresent()"),
            "expected isPresent check, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn nested_vec_equality_uses_list_equals() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Matrix",
            vec![field(
                "rows",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
        ));

        let module = lower(&contract);
        let field = &module.records[0].fields[0];
        assert!(
            field.equals_expr.contains("WireWriter.listEquals("),
            "expected WireWriter.listEquals, got: {}",
            field.equals_expr
        );
    }

    #[test]
    fn c_style_enum_lowering() {
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
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        assert_eq!(module.enums.len(), 1);
        let e = &module.enums[0];
        assert_eq!(e.class_name, "Status");
        assert!(e.is_c_style());
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "ACTIVE");
        assert_eq!(e.variants[1].name, "INACTIVE");
    }

    #[test]
    fn data_enum_abstract_on_java8() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    DataVariant {
                        name: VariantName::new("Circle"),
                        discriminant: 0,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::F64,
                        )]),
                        doc: None,
                    },
                    DataVariant {
                        name: VariantName::new("Square"),
                        discriminant: 1,
                        payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(
                            PrimitiveType::F64,
                        )]),
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

        let module = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert!(module.enums[0].is_abstract());
    }

    #[test]
    fn data_enum_sealed_on_java17() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Circle"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(PrimitiveType::F64)]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert!(module.enums[0].is_sealed());
    }

    #[test]
    fn data_enum_falls_back_to_abstract_with_primitive_array_field() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Payload"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Binary"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::Vec(Box::new(
                        TypeExpr::Primitive(PrimitiveType::U8),
                    ))]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_17);
        assert!(
            module.enums[0].is_abstract(),
            "should fall back to abstract due to primitive array field"
        );
    }

    #[test]
    fn data_enum_variant_names_are_pascal_case() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("some_text"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        assert_eq!(module.enums[0].variants[0].name, "SomeText");
    }

    #[test]
    fn function_primitive_return_is_direct() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_count",
            vec![],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int");
        assert!(func.return_plan.is_direct());
    }

    #[test]
    fn function_void_return() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(function("do_nothing", vec![], ReturnDef::Void));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "void");
        assert!(func.return_plan.is_void());
    }

    #[test]
    fn function_string_return_is_wire_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_name",
            vec![],
            ReturnDef::Value(TypeExpr::String),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "String");
        assert!(func.return_plan.is_decode());
        assert!(func.return_plan.decode_expr().contains("readString"));
    }

    #[test]
    fn function_bytes_return_is_buffer_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_data",
            vec![],
            ReturnDef::Value(TypeExpr::Bytes),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "byte[]");
        assert!(func.return_plan.is_decode());
        assert!(
            func.return_plan.decode_expr().contains("new byte[0]"),
            "expected null-safe buffer decode, got: {}",
            func.return_plan.decode_expr()
        );
    }

    #[test]
    fn function_string_param_converts_to_utf8_bytes() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "greet",
            vec![param("name", TypeExpr::String)],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let p = &module.functions[0].params[0];
        assert_eq!(p.java_type, "String");
        assert_eq!(p.native_type, "byte[]");
        assert!(
            p.native_expr.contains("getBytes"),
            "expected UTF-8 conversion, got: {}",
            p.native_expr
        );
    }

    #[test]
    fn function_bytes_param_passes_directly() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "process",
            vec![param("data", TypeExpr::Bytes)],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let p = &module.functions[0].params[0];
        assert_eq!(p.java_type, "byte[]");
        assert_eq!(p.native_type, "byte[]");
        assert_eq!(p.native_expr, "data");
    }

    #[test]
    fn function_record_param_uses_direct_composite_input() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Point",
            vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.functions.push(function(
            "translate",
            vec![param("point", TypeExpr::Record(RecordId::new("Point")))],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        let p = &func.params[0];
        assert_eq!(p.native_type, "ByteBuffer");
        assert!(func.input_bindings.wire_writers.is_empty());
        assert_eq!(func.input_bindings.direct_composites.len(), 1);
        assert_eq!(
            func.input_bindings.direct_composites[0].binding_name,
            "_direct_point"
        );
    }

    #[test]
    fn function_option_param_uses_wire_writer() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "maybe",
            vec![param(
                "value",
                TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.params[0].native_type, "ByteBuffer");
        assert!(!func.input_bindings.wire_writers.is_empty());
    }

    #[test]
    fn function_vec_isize_param_uses_long_array() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "echo_vec_isize",
            vec![param(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::ISize))),
            )],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        let param = &func.params[0];
        assert_eq!(param.java_type, "long[]");
        assert_eq!(param.native_type, "long[]");
        assert_eq!(param.native_expr, "values");
    }

    #[test]
    fn function_vec_blittable_record_param_uses_raw_blittable_buffer() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("Point"),
            fields: vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function(
            "make_polygon",
            vec![param(
                "points",
                TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("Point")))),
            )],
            ReturnDef::Void,
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.params[0].native_type, "ByteBuffer");
        assert!(func.input_bindings.wire_writers.is_empty());
        assert_eq!(
            func.params[0].native_expr,
            "Point.encodeBlittableVecInput(points)"
        );
    }

    #[test]
    fn function_option_return_is_wire_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "find",
            vec![],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "java.util.Optional<Integer>");
        assert!(func.return_plan.is_decode());
        assert!(func.return_plan.decode_expr().contains("Optional"));
    }

    #[test]
    fn function_error_record_result_throws_direct_exception() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            id: RecordId::new("AppError"),
            is_repr_c: true,
            is_error: true,
            fields: vec![
                field("code", TypeExpr::Primitive(PrimitiveType::I32)),
                field("message", TypeExpr::String),
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function(
            "may_fail",
            vec![],
            ReturnDef::Result {
                ok: TypeExpr::String,
                err: TypeExpr::Record(RecordId::new("AppError")),
            },
        ));

        let module = lower(&contract);
        let func = &module.functions[0];

        assert_eq!(func.return_type, "String");
        assert!(func.return_plan.is_result());
        assert!(func.return_plan.result_has_typed_exception());
        assert!(func.return_plan.result_err_throws_directly());
        assert_eq!(
            func.return_plan.result_err_decode(),
            "AppError.decode(reader)"
        );
    }

    #[test]
    fn payload_error_enum_result_uses_throwable_abstract_class() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("ApiError"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    crate::ir::definitions::DataVariant {
                        name: crate::ir::ids::VariantName::new("Network"),
                        discriminant: 0,
                        payload: VariantPayload::Struct(vec![field("message", TypeExpr::String)]),
                        doc: None,
                    },
                    crate::ir::definitions::DataVariant {
                        name: crate::ir::ids::VariantName::new("Http"),
                        discriminant: 1,
                        payload: VariantPayload::Struct(vec![
                            field("code", TypeExpr::Primitive(PrimitiveType::I32)),
                            field("message", TypeExpr::String),
                        ]),
                        doc: None,
                    },
                ],
            },
            is_error: true,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function(
            "fetch",
            vec![],
            ReturnDef::Result {
                ok: TypeExpr::String,
                err: TypeExpr::Enum(EnumId::new("ApiError")),
            },
        ));

        let module = lower(&contract);
        let enumeration = &module.enums[0];
        let func = &module.functions[0];

        assert!(enumeration.is_error());
        assert!(enumeration.is_abstract());
        assert!(func.return_plan.is_result());
        assert!(func.return_plan.result_has_typed_exception());
        assert!(func.return_plan.result_err_throws_directly());
        assert_eq!(
            func.return_plan.result_err_decode(),
            "ApiError.decode(reader)"
        );
    }

    #[test]
    fn function_name_is_camel_case() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(function("get_max_value", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions[0].name, "getMaxValue");
    }

    #[test]
    fn c_style_enum_return_is_direct_decode() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![CStyleVariant {
                    name: VariantName::new("Ok"),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function(
            "get_status",
            vec![],
            ReturnDef::Value(TypeExpr::Enum(EnumId::new("Status"))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert!(func.return_plan.is_c_style_enum());
        assert_eq!(func.return_plan.c_style_enum_class(), "Status");
    }

    #[test]
    fn record_with_record_field() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Inner",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_record(record_def(
            "Outer",
            vec![field("inner", TypeExpr::Record(RecordId::new("Inner")))],
        ));

        let module = lower(&contract);
        let outer = module
            .records
            .iter()
            .find(|r| r.class_name == "Outer")
            .unwrap();
        assert_eq!(outer.fields[0].java_type, "Inner");
    }

    #[test]
    fn vec_of_records_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "Item",
            vec![field("id", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_record(record_def(
            "Container",
            vec![field(
                "items",
                TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("Item")))),
            )],
        ));

        let module = lower(&contract);
        let container = module
            .records
            .iter()
            .find(|r| r.class_name == "Container")
            .unwrap();
        assert_eq!(container.fields[0].java_type, "java.util.List<Item>");
    }

    #[test]
    fn custom_type_record_and_functions_are_lowered_for_java() {
        let mut contract = empty_contract();
        contract.catalog.insert_custom(custom_type_def(
            "UtcDateTime",
            TypeExpr::Primitive(PrimitiveType::I64),
        ));
        contract.catalog.insert_record(record_def(
            "Event",
            vec![
                field("name", TypeExpr::String),
                field(
                    "timestamp",
                    TypeExpr::Custom(CustomTypeId::new("UtcDateTime")),
                ),
            ],
        ));
        contract.functions.push(function(
            "event_timestamp",
            vec![param("event", TypeExpr::Record(RecordId::new("Event")))],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I64)),
        ));
        contract.functions.push(function(
            "format_timestamp",
            vec![param(
                "timestamp",
                TypeExpr::Custom(CustomTypeId::new("UtcDateTime")),
            )],
            ReturnDef::Value(TypeExpr::String),
        ));

        let module = lower(&contract);
        let event = module
            .records
            .iter()
            .find(|record| record.class_name == "Event")
            .expect("Event record should be generated");
        let timestamp_field = event
            .fields
            .iter()
            .find(|field| field.name == "timestamp")
            .expect("timestamp field should be generated");
        assert_eq!(timestamp_field.java_type, "long");

        let event_timestamp = module
            .functions
            .iter()
            .find(|function| function.name == "eventTimestamp")
            .expect("eventTimestamp should be generated");
        assert_eq!(event_timestamp.params[0].java_type, "Event");

        let format_timestamp = module
            .functions
            .iter()
            .find(|function| function.name == "formatTimestamp")
            .expect("formatTimestamp should be generated");
        assert_eq!(format_timestamp.params[0].java_type, "long");
        assert_eq!(format_timestamp.params[0].native_type, "ByteBuffer");
        assert_eq!(format_timestamp.return_type, "String");
    }

    #[test]
    fn async_functions_are_included_with_async_call() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("slow_op"),
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });
        contract
            .functions
            .push(function("fast_op", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 2);
        let slow = module
            .functions
            .iter()
            .find(|f| f.name == "slowOp")
            .unwrap();
        let fast = module
            .functions
            .iter()
            .find(|f| f.name == "fastOp")
            .unwrap();
        assert!(slow.is_async());
        assert!(!fast.is_async());
    }

    #[test]
    fn callback_params_are_supported() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "with_callback",
            vec![param(
                "cb",
                TypeExpr::Callback(crate::ir::ids::CallbackId::new("Foo")),
            )],
            ReturnDef::Void,
        ));
        contract
            .functions
            .push(function("simple", vec![], ReturnDef::Void));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn callback_return_uses_wrap_strategy() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![callback_method(
                "on_value",
                vec![param("value", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Void,
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });
        contract.functions.push(function(
            "next_listener",
            vec![],
            ReturnDef::Value(TypeExpr::Callback(CallbackId::new("Listener"))),
        ));

        let module = lower(&contract);
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "nextListener")
            .unwrap();

        assert!(function.return_plan.is_callback());
        assert_eq!(
            function.return_plan.callback_bridge_class(),
            "ListenerCallbacks"
        );
        assert_eq!(function.return_plan.native_return_type, "long");
    }

    #[test]
    fn callback_trait_sync_proxy_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![callback_method(
                "on_value",
                vec![param("value", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower(&contract);
        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "Listener")
            .unwrap();
        let method = &callback.sync_methods[0];

        assert_eq!(
            callback.proxy_clone_native_name(),
            "boltffiCallbackListenerClone"
        );
        assert_eq!(
            callback.proxy_release_native_name(),
            "boltffiCallbackListenerRelease"
        );
        assert_eq!(method.proxy.native_name, "boltffiCallbackListenerOnValue");
        assert_eq!(method.proxy.return_type, "int");
        assert!(method.proxy.return_plan.is_direct());
    }

    #[test]
    fn closure_proxy_is_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("__Closure_Stream"),
            methods: vec![callback_method(
                "call",
                vec![param("value", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Closure,
            doc: None,
        });
        contract.functions.push(function(
            "next_closure",
            vec![],
            ReturnDef::Value(TypeExpr::Callback(CallbackId::new("__Closure_Stream"))),
        ));

        let module = lower(&contract);
        let closure = module
            .closures
            .iter()
            .find(|closure| closure.interface_name == "ClosureStream")
            .unwrap();

        assert_eq!(closure.proxy.native_name, "boltffiClosureStreamCallbacks");
        assert_eq!(
            closure.proxy_clone_native_name(),
            "boltffiClosureStreamCallbacksClone"
        );
        assert_eq!(
            closure.proxy_release_native_name(),
            "boltffiClosureStreamCallbacksRelease"
        );
    }

    #[test]
    fn async_callback_methods_are_preserved() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Listener"),
            methods: vec![callback_method(
                "on_value",
                vec![param("value", TypeExpr::Primitive(PrimitiveType::I32))],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ExecutionKind::Async,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower(&contract);
        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "Listener")
            .expect("callback should be lowered");

        assert!(callback.sync_methods.is_empty());
        assert_eq!(callback.async_methods.len(), 1);
        assert_eq!(module.async_callback_invokers.len(), 1);
        assert_eq!(module.async_callback_invokers[0].result_jni_type(), "int");
    }

    #[test]
    fn encoded_callback_params_follow_abi_decode_path() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Decoder"),
            methods: vec![callback_method(
                "on_values",
                vec![param(
                    "values",
                    TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
                )],
                ReturnDef::Void,
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower(&contract);
        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "Decoder")
            .expect("callback should be lowered");
        let method = &callback.sync_methods[0];
        let param = &method.params[0];

        assert_eq!(param.jni_type, "java.nio.ByteBuffer");
        assert!(param.decode_expr.contains("WireReader.decodeBuffer"));
        assert!(param.decode_expr.contains("reader.readIntArray()"));
    }

    #[test]
    fn callback_proxy_string_params_use_wire_writer_buffers() {
        let mut contract = empty_contract();
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("Formatter"),
            methods: vec![callback_method(
                "format",
                vec![
                    param("scope", TypeExpr::String),
                    param("message", TypeExpr::String),
                ],
                ReturnDef::Value(TypeExpr::String),
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower(&contract);
        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "Formatter")
            .expect("callback should be lowered");
        let method = &callback.sync_methods[0];

        assert_eq!(method.proxy.params[0].native_type, "ByteBuffer");
        assert_eq!(method.proxy.params[1].native_type, "ByteBuffer");
        assert_eq!(method.proxy.params[0].native_expr, "_wire_scope.toBuffer()");
        assert_eq!(
            method.proxy.params[1].native_expr,
            "_wire_message.toBuffer()"
        );
    }

    #[test]
    fn c_style_enum_callback_params_use_direct_scalar_bridge() {
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
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("StatusMapper"),
            methods: vec![callback_method(
                "map_status",
                vec![param("status", TypeExpr::Enum(EnumId::new("Status")))],
                ReturnDef::Value(TypeExpr::Enum(EnumId::new("Status"))),
                ExecutionKind::Sync,
            )],
            kind: CallbackKind::Trait,
            doc: None,
        });

        let module = lower(&contract);
        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "StatusMapper")
            .expect("callback should be lowered");
        let param = &callback.sync_methods[0].params[0];

        assert_eq!(param.jni_type, "int");
        assert_eq!(param.decode_expr, "Status.fromValue(status)");
    }

    #[test]
    fn module_metadata() {
        let contract = empty_contract();
        let module = lower(&contract);
        assert_eq!(module.package_name, "com.test");
        assert_eq!(module.class_name, "Test");
    }

    #[test]
    fn vec_primitive_return_is_buffer_decode() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "get_scores",
            vec![],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int[]");
        assert!(func.return_plan.is_decode());
    }

    #[test]
    fn data_enum_field_has_wire_expressions() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("Value"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Text"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::String]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let variant = &module.enums[0].variants[0];
        let field = &variant.fields[0];
        assert!(!field.wire_decode_expr.is_empty());
        assert!(!field.wire_size_expr.is_empty());
        assert!(!field.wire_encode_expr.is_empty());
        assert!(!field.equals_expr.is_empty());
        assert!(!field.hash_expr.is_empty());
    }

    #[test]
    fn bytes_supported_in_function_params() {
        let mut contract = empty_contract();
        contract.functions.push(function(
            "echo",
            vec![param("data", TypeExpr::Bytes)],
            ReturnDef::Value(TypeExpr::Bytes),
        ));

        let module = lower(&contract);
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn option_of_bytes_field_type() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "MaybeBuffer",
            vec![field("data", TypeExpr::Option(Box::new(TypeExpr::Bytes)))],
        ));

        let module = lower(&contract);
        assert_eq!(
            module.records[0].fields[0].java_type,
            "java.util.Optional<byte[]>"
        );
    }

    fn class_def(id: &str, constructors: Vec<ConstructorDef>, methods: Vec<MethodDef>) -> ClassDef {
        ClassDef {
            id: ClassId::from(id),
            constructors,
            methods,
            streams: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn default_ctor(params: Vec<ParamDef>) -> ConstructorDef {
        ConstructorDef::Default {
            params,
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn fallible_default_ctor(params: Vec<ParamDef>) -> ConstructorDef {
        ConstructorDef::Default {
            params,
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn named_factory(name: &str) -> ConstructorDef {
        ConstructorDef::NamedFactory {
            name: name.into(),
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn named_init(name: &str, params: Vec<ParamDef>) -> ConstructorDef {
        let mut params_iter = params.into_iter();
        let first_param = params_iter
            .next()
            .expect("named init needs at least one param");
        ConstructorDef::NamedInit {
            name: name.into(),
            first_param,
            rest_params: params_iter.collect(),
            is_fallible: false,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn fallible_named_init(name: &str, params: Vec<ParamDef>) -> ConstructorDef {
        let mut params_iter = params.into_iter();
        let first_param = params_iter
            .next()
            .expect("named init needs at least one param");
        ConstructorDef::NamedInit {
            name: name.into(),
            first_param,
            rest_params: params_iter.collect(),
            is_fallible: true,
            is_optional: false,
            doc: None,
            deprecated: None,
        }
    }

    fn instance_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::RefSelf,
            params,
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn static_method(name: &str, params: Vec<ParamDef>, returns: ReturnDef) -> MethodDef {
        MethodDef {
            id: MethodId::from(name),
            receiver: Receiver::Static,
            params,
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn param_def(name: &str, ty: TypeExpr) -> ParamDef {
        ParamDef {
            name: ParamName::from(name),
            type_expr: ty,
            passing: ParamPassing::Value,
            doc: None,
        }
    }

    #[test]
    fn class_basic_default_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![param_def(
                "initial",
                TypeExpr::Primitive(PrimitiveType::I32),
            )])],
            vec![],
        ));

        let module = lower(&contract);
        assert_eq!(module.classes.len(), 1);

        let class = &module.classes[0];
        assert_eq!(class.class_name, "Counter");
        assert_eq!(class.constructors.len(), 1);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Primary);
        assert!(!class.constructors[0].is_fallible);
        assert_eq!(class.constructors[0].params.len(), 1);
        assert_eq!(class.constructors[0].params[0].name, "initial");
        assert_eq!(class.constructors[0].params[0].java_type, "int");
    }

    #[test]
    fn class_factory_constructor() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_def("inventory", vec![named_factory("empty")], vec![]));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors.len(), 1);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[0].name, "empty");
        assert!(class.has_factory_constructors());
    }

    #[test]
    fn class_named_init_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![named_init(
                "with_capacity",
                vec![param_def(
                    "capacity",
                    TypeExpr::Primitive(PrimitiveType::I32),
                )],
            )],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Secondary);
        assert_eq!(class.constructors[0].name, "withCapacity");
        assert_eq!(class.constructors[0].params.len(), 1);
        assert_eq!(class.constructors[0].params[0].name, "capacity");
    }

    #[test]
    fn class_fallible_constructor() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "connection",
            vec![fallible_default_ctor(vec![param_def(
                "url",
                TypeExpr::String,
            )])],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert!(class.constructors[0].is_fallible);
    }

    #[test]
    fn class_instance_method_returning_primitive() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "get_value",
                vec![],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].name, "getValue");
        assert!(!class.methods[0].is_static);
        assert_eq!(class.methods[0].return_type, "int");
    }

    #[test]
    fn class_static_method() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "math_utils",
            vec![default_ctor(vec![])],
            vec![static_method(
                "add",
                vec![
                    param_def("a", TypeExpr::Primitive(PrimitiveType::I32)),
                    param_def("b", TypeExpr::Primitive(PrimitiveType::I32)),
                ],
                ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert!(class.methods[0].is_static);
        assert_eq!(class.methods[0].name, "add");
        assert_eq!(class.methods[0].params.len(), 2);
        assert!(class.has_static_methods());
    }

    #[test]
    fn class_void_method() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![])],
            vec![instance_method("increment", vec![], ReturnDef::Void)],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods[0].return_type, "void");
        assert!(class.methods[0].return_plan.is_void());
    }

    #[test]
    fn class_method_returning_string() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "person",
            vec![default_ctor(vec![param_def("name", TypeExpr::String)])],
            vec![instance_method(
                "get_name",
                vec![],
                ReturnDef::Value(TypeExpr::String),
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods[0].return_type, "String");
    }

    #[test]
    fn class_includes_async_methods_with_async_call() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "worker",
            vec![default_ctor(vec![])],
            vec![
                instance_method(
                    "sync_op",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                ),
                MethodDef {
                    id: MethodId::from("async_op"),
                    receiver: Receiver::RefSelf,
                    params: vec![],
                    returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
                    execution_kind: ExecutionKind::Async,
                    doc: None,
                    deprecated: None,
                },
            ],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 2);
        let sync_method = class.methods.iter().find(|m| m.name == "syncOp").unwrap();
        let async_method = class.methods.iter().find(|m| m.name == "asyncOp").unwrap();
        assert!(!sync_method.is_async());
        assert!(async_method.is_async());
        let ac = async_method.async_call.as_ref().unwrap();
        assert!(!ac.poll.is_empty());
        assert!(!ac.complete.is_empty());
        assert!(!ac.cancel.is_empty());
        assert!(!ac.free.is_empty());
    }

    #[test]
    fn class_includes_result_methods() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "processor",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "process",
                vec![],
                ReturnDef::Result {
                    ok: TypeExpr::Primitive(PrimitiveType::I32),
                    err: TypeExpr::String,
                },
            )],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.methods.len(), 1);
        assert_eq!(class.methods[0].return_type, "int");
        assert!(class.methods[0].return_plan.is_result());
        assert!(class.methods[0].return_plan.result_err_is_string());
    }

    #[test]
    fn class_ffi_free_name() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_class(class_def("counter", vec![default_ctor(vec![])], vec![]));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert!(!class.ffi_free.is_empty());
        assert!(class.ffi_free.contains("counter"));
    }

    #[test]
    fn class_multiple_constructors() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![
                default_ctor(vec![]),
                named_factory("empty"),
                named_init(
                    "with_capacity",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
            ],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];
        assert_eq!(class.constructors.len(), 3);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Primary);
        assert_eq!(class.constructors[1].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[2].kind, JavaConstructorKind::Secondary);
    }

    #[test]
    fn class_constructor_signature_collision_demotes_fallible_named_init_to_factory() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(class_def(
            "inventory",
            vec![
                named_init(
                    "with_capacity",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
                fallible_named_init(
                    "try_new",
                    vec![param_def(
                        "capacity",
                        TypeExpr::Primitive(PrimitiveType::I32),
                    )],
                ),
            ],
            vec![],
        ));

        let module = lower(&contract);
        let class = &module.classes[0];

        assert_eq!(class.constructors.len(), 2);
        assert_eq!(class.constructors[0].kind, JavaConstructorKind::Secondary);
        assert_eq!(class.constructors[0].name, "withCapacity");
        assert_eq!(class.constructors[1].kind, JavaConstructorKind::Factory);
        assert_eq!(class.constructors[1].name, "tryNew");
        assert!(class.has_factory_constructors());
    }

    #[test]
    fn class_direct_composite_params_do_not_report_wire_usage() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_def(
            "config",
            vec![field("value", TypeExpr::Primitive(PrimitiveType::I32))],
        ));
        contract.catalog.insert_class(class_def(
            "counter",
            vec![default_ctor(vec![param_def(
                "config",
                TypeExpr::Record(RecordId::new("config")),
            )])],
            vec![],
        ));

        let module = lower(&contract);
        assert!(!module.has_wire_params());
        assert_eq!(
            module.classes[0].constructors[0]
                .input_bindings
                .direct_composites
                .len(),
            1
        );
        assert!(
            module.classes[0].constructors[0]
                .input_bindings
                .wire_writers
                .is_empty()
        );
    }

    #[test]
    fn record_value_type_members_are_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            is_error: false,
            id: RecordId::new("point"),
            fields: vec![
                field("x", TypeExpr::Primitive(PrimitiveType::F64)),
                field("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
            constructors: vec![
                named_factory("origin"),
                named_init(
                    "from_polar",
                    vec![
                        param_def("radius", TypeExpr::Primitive(PrimitiveType::F64)),
                        param_def("theta", TypeExpr::Primitive(PrimitiveType::F64)),
                    ],
                ),
                fallible_named_init(
                    "try_unit",
                    vec![
                        param_def("x", TypeExpr::Primitive(PrimitiveType::F64)),
                        param_def("y", TypeExpr::Primitive(PrimitiveType::F64)),
                    ],
                ),
            ],
            methods: vec![
                instance_method(
                    "distance",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                ),
                MethodDef {
                    id: MethodId::from("scale"),
                    receiver: Receiver::RefMutSelf,
                    params: vec![param_def("factor", TypeExpr::Primitive(PrimitiveType::F64))],
                    returns: ReturnDef::Void,
                    execution_kind: ExecutionKind::Sync,
                    doc: None,
                    deprecated: None,
                },
                static_method(
                    "dimensions",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
                ),
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let record = module
            .records
            .iter()
            .find(|record| record.class_name == "Point")
            .unwrap();

        assert_eq!(record.constructors.len(), 3);
        assert_eq!(record.constructors[0].name, "origin");
        assert_eq!(record.constructors[1].name, "fromPolar");
        assert_eq!(record.constructors[2].name, "tryUnit");
        assert_eq!(record.constructors[1].return_type, "Point");
        assert!(record.constructors[2].return_plan.is_result());
        assert!(record.constructors[2].return_plan.result_err_is_string());

        assert_eq!(record.methods.len(), 3);
        assert_eq!(record.methods[0].name, "distance");
        assert!(!record.methods[0].is_static);
        assert_eq!(record.methods[0].native_params[0].name, "selfBuffer");
        assert_eq!(record.methods[0].native_params[0].native_type, "ByteBuffer");
        assert_eq!(record.methods[0].native_params[0].expr, "_direct_self");

        assert_eq!(record.methods[1].name, "scale");
        assert_eq!(record.methods[1].return_type, "Point");
        assert!(record.methods[1].return_plan.is_decode());

        assert_eq!(record.methods[2].name, "dimensions");
        assert!(record.methods[2].is_static);
        assert!(record.has_constructors());
        assert!(record.has_static_methods());
        assert!(record.has_instance_methods());
    }

    #[test]
    fn record_field_defaults_generate_java_overloads() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(RecordDef {
            is_repr_c: true,
            id: RecordId::new("config"),
            is_error: false,
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
                FieldDef {
                    name: FieldName::new("alias"),
                    type_expr: TypeExpr::Option(Box::new(TypeExpr::String)),
                    doc: None,
                    default: Some(DefaultValue::String("primary".to_string())),
                },
            ],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let record = &module.records[0];

        assert_eq!(record.fields[1].default_value.as_deref(), Some("3"));
        assert_eq!(
            record.fields[2].default_value.as_deref(),
            Some("java.util.Optional.empty()")
        );
        assert_eq!(
            record.fields[3].default_value.as_deref(),
            Some("java.util.Optional.of(\"primary\")")
        );
        assert_eq!(record.default_constructors.len(), 3);
        assert_eq!(record.default_constructors[0].params.len(), 3);
        assert_eq!(
            record.default_constructors[0].arguments,
            vec![
                "name",
                "retries",
                "label",
                "java.util.Optional.of(\"primary\")"
            ]
        );
        assert_eq!(record.default_constructors[1].params.len(), 2);
        assert_eq!(
            record.default_constructors[1].arguments,
            vec![
                "name",
                "retries",
                "java.util.Optional.empty()",
                "java.util.Optional.of(\"primary\")"
            ]
        );
        assert_eq!(record.default_constructors[2].params.len(), 1);
        assert_eq!(
            record.default_constructors[2].arguments,
            vec![
                "name",
                "3",
                "java.util.Optional.empty()",
                "java.util.Optional.of(\"primary\")"
            ]
        );
    }

    #[test]
    fn c_style_enum_value_type_members_are_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("direction"),
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
            constructors: vec![
                named_init(
                    "new",
                    vec![param_def("raw", TypeExpr::Primitive(PrimitiveType::I32))],
                ),
                named_factory("cardinal"),
            ],
            methods: vec![
                instance_method(
                    "opposite",
                    vec![],
                    ReturnDef::Value(TypeExpr::Enum(EnumId::new("direction"))),
                ),
                instance_method("label", vec![], ReturnDef::Value(TypeExpr::String)),
                static_method(
                    "count",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
                ),
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let enumeration = module
            .enums
            .iter()
            .find(|enumeration| enumeration.class_name == "Direction")
            .unwrap();

        assert_eq!(enumeration.constructors.len(), 2);
        assert_eq!(enumeration.constructors[0].name, "_new");
        assert_eq!(enumeration.constructors[1].name, "cardinal");
        assert_eq!(enumeration.constructors[0].params[0].java_type, "int");

        assert_eq!(enumeration.methods.len(), 3);
        assert_eq!(enumeration.methods[0].name, "opposite");
        assert_eq!(enumeration.methods[0].return_type, "Direction");
        assert_eq!(enumeration.methods[0].native_params[0].name, "selfValue");
        assert_eq!(enumeration.methods[0].native_params[0].native_type, "int");
        assert_eq!(
            enumeration.methods[0].native_params[0].expr,
            "this.nativeValue()"
        );
        assert!(enumeration.methods[0].return_plan.is_c_style_enum());
        assert_eq!(
            enumeration.methods[0].return_plan.c_style_enum_class(),
            "Direction"
        );

        assert_eq!(enumeration.methods[1].name, "label");
        assert_eq!(enumeration.methods[1].return_type, "String");
        assert!(enumeration.methods[1].return_plan.is_decode());

        assert_eq!(enumeration.methods[2].name, "count");
        assert!(enumeration.methods[2].is_static);
        assert!(enumeration.has_constructors());
        assert!(enumeration.has_static_methods());
        assert!(enumeration.has_instance_methods());
    }

    #[test]
    fn data_enum_value_type_members_are_lowered() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: VariantName::new("Circle"),
                    discriminant: 0,
                    payload: VariantPayload::Tuple(vec![TypeExpr::Primitive(PrimitiveType::F64)]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![
                named_init(
                    "new",
                    vec![param_def("radius", TypeExpr::Primitive(PrimitiveType::F64))],
                ),
                named_factory("unit_circle"),
                fallible_named_init(
                    "try_circle",
                    vec![param_def("radius", TypeExpr::Primitive(PrimitiveType::F64))],
                ),
            ],
            methods: vec![
                instance_method(
                    "area",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
                ),
                instance_method("describe", vec![], ReturnDef::Value(TypeExpr::String)),
                static_method(
                    "variant_count",
                    vec![],
                    ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
                ),
            ],
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let enumeration = module
            .enums
            .iter()
            .find(|enumeration| enumeration.class_name == "Shape")
            .unwrap();

        assert_eq!(enumeration.constructors.len(), 3);
        assert_eq!(enumeration.constructors[0].name, "_new");
        assert_eq!(enumeration.constructors[1].name, "unitCircle");
        assert_eq!(enumeration.constructors[2].name, "tryCircle");
        assert!(enumeration.constructors[2].return_plan.is_result());
        assert!(
            enumeration.constructors[2]
                .return_plan
                .result_err_is_string()
        );

        assert_eq!(enumeration.methods.len(), 3);
        assert_eq!(enumeration.methods[0].name, "area");
        assert_eq!(enumeration.methods[0].native_params[0].name, "selfBuffer");
        assert_eq!(
            enumeration.methods[0].native_params[0].native_type,
            "ByteBuffer"
        );
        assert_eq!(
            enumeration.methods[0].native_params[0].expr,
            "_wire_self.toBuffer()"
        );
        assert!(
            !enumeration.methods[0]
                .input_bindings
                .wire_writers
                .is_empty()
        );
        assert_eq!(enumeration.methods[0].return_type, "double");
        assert!(enumeration.methods[0].return_plan.is_direct());

        assert_eq!(enumeration.methods[1].name, "describe");
        assert_eq!(enumeration.methods[1].return_type, "String");
        assert!(enumeration.methods[1].return_plan.is_decode());

        assert_eq!(enumeration.methods[2].name, "variantCount");
        assert!(enumeration.methods[2].is_static);
        assert!(enumeration.has_wire_params());
        assert!(module.has_wire_params());
    }

    #[test]
    fn handle_return_strategy_fields() {
        let return_plan = JavaReturnPlan {
            native_return_type: "long".to_string(),
            render: JavaReturnRender::Handle {
                class_name: "Counter".to_string(),
                nullable: false,
            },
        };
        assert!(return_plan.is_handle());
        assert_eq!(return_plan.handle_class(), "Counter");
        assert!(!return_plan.handle_nullable());
        assert_eq!(return_plan.native_return_type, "long");
    }

    #[test]
    fn handle_return_strategy_nullable() {
        let return_plan = JavaReturnPlan {
            native_return_type: "long".to_string(),
            render: JavaReturnRender::Handle {
                class_name: "Counter".to_string(),
                nullable: true,
            },
        };
        assert!(return_plan.handle_nullable());
    }

    #[test]
    fn handle_type_in_method_return() {
        let mut contract = empty_contract();
        let target_class_id = ClassId::from("target");
        contract
            .catalog
            .insert_class(class_def("target", vec![default_ctor(vec![])], vec![]));
        contract.catalog.insert_class(class_def(
            "factory",
            vec![default_ctor(vec![])],
            vec![instance_method(
                "create_target",
                vec![],
                ReturnDef::Value(TypeExpr::Handle(target_class_id)),
            )],
        ));

        let module = lower(&contract);
        let factory_class = module
            .classes
            .iter()
            .find(|c| c.class_name == "Factory")
            .unwrap();
        assert_eq!(factory_class.methods.len(), 1);
        assert_eq!(factory_class.methods[0].return_type, "Target");
        assert!(factory_class.methods[0].return_plan.is_handle());
        assert_eq!(
            factory_class.methods[0].return_plan.handle_class(),
            "Target"
        );
    }

    #[test]
    fn async_function_has_poll_complete_cancel_free() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fetch_data"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        assert!(func.is_async());
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.poll.contains("fetch_data"));
        assert!(ac.complete.contains("fetch_data"));
        assert!(ac.cancel.contains("fetch_data"));
        assert!(ac.free.contains("fetch_data"));
    }

    #[test]
    fn async_mode_is_virtual_thread_for_java21() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("op"),
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower_with_version(&contract, JavaVersion::JAVA_21);
        assert!(module.async_mode.is_virtual_thread());

        let module8 = lower_with_version(&contract, JavaVersion::JAVA_8);
        assert!(module8.async_mode.is_completable_future());
    }

    #[test]
    fn async_function_strategy_matches_return_type() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fetch_data"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.complete_return_plan.is_decode());
    }

    #[test]
    fn async_function_boxed_return_type_for_primitives() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("get_count"),
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        assert_eq!(func.return_type, "int");
        assert_eq!(func.boxed_return_type(), "Integer");
    }

    #[test]
    fn async_void_function_strategy() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: FunctionId::new("fire_and_forget"),
            params: vec![],
            returns: ReturnDef::Void,
            execution_kind: ExecutionKind::Async,
            doc: None,
            deprecated: None,
        });

        let module = lower(&contract);
        let func = &module.functions[0];
        let ac = func.async_call.as_ref().unwrap();
        assert!(ac.complete_return_plan.is_void());
    }
}
