use std::collections::{HashMap, HashSet};

use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::transport::{
    EncodedReturnStrategy, EnumTagStrategy, ScalarReturnStrategy, ValueReturnStrategy,
};

use crate::ir::abi::{
    AbiCall, AbiCallbackInvocation, AbiCallbackMethod, AbiContract, AbiEnum, AbiEnumField,
    AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, AbiStream, CallId, CallMode,
    ErrorTransport, ParamRole, ReturnShape, StreamItemTransport,
};
use crate::ir::codec::VecLayout;
use crate::ir::contract::FfiContract;
use crate::ir::definitions::Receiver;
use crate::ir::definitions::{
    CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, ConstructorDef, CustomTypeDef,
    DefaultValue, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, RecordDef,
    ReturnDef, StreamDef, StreamMode, VariantPayload,
};
use crate::ir::ids::{
    BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, FieldName, MethodId, ParamName, RecordId,
};
use crate::ir::ops::{
    FieldReadOp, FieldWriteOp, OffsetExpr, ReadOp, ReadSeq, SizeExpr, ValueExpr, WireShape,
    WriteOp, WriteSeq, remap_root_in_seq,
};
use crate::ir::plan::{AbiType, ScalarOrigin, SpanContent, Transport};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::render::kotlin::emit;
use crate::render::kotlin::plan::*;
use crate::render::kotlin::templates::{AsyncMethodTemplate, WireMethodTemplate};
use crate::render::kotlin::{
    FactoryStyle, KotlinApiStyle as KotlinInputApiStyle, KotlinOptions, NamingConvention,
};
use crate::render::{TypeConversion, TypeMappings};
use askama::Template;
use boltffi_ffi_rules::naming;

struct KotlinReturnMeta {
    is_unit: bool,
    is_direct: bool,
    direct_is_nullable: bool,
    cast: String,
}

pub struct KotlinLowerer<'a> {
    contract: &'a FfiContract,
    abi: &'a AbiContract,
    package_name: String,
    module_name: String,
    options: KotlinOptions,
    type_mappings: TypeMappings,
}

impl<'a> KotlinLowerer<'a> {
    pub fn new(
        contract: &'a FfiContract,
        abi: &'a AbiContract,
        package_name: String,
        module_name: String,
        options: KotlinOptions,
    ) -> Self {
        Self {
            contract,
            abi,
            package_name,
            module_name,
            options,
            type_mappings: TypeMappings::new(),
        }
    }

    pub fn with_type_mappings(mut self, mappings: TypeMappings) -> Self {
        self.type_mappings = mappings;
        self
    }

    pub fn lower(&self) -> KotlinModule {
        let has_streams = self
            .contract
            .catalog
            .all_classes()
            .any(|class| !class.streams.is_empty());
        let has_async_runtime = self.has_async_runtime(has_streams);
        let preamble = self.lower_preamble(has_async_runtime, has_streams);
        let enums = self
            .contract
            .catalog
            .all_enums()
            .map(|e| self.lower_enum(e))
            .collect::<Vec<_>>();
        let data_enum_codecs = self
            .contract
            .catalog
            .all_enums()
            .filter(|e| self.should_generate_fixed_enum_codec(e))
            .map(|e| self.lower_data_enum_codec(e))
            .collect::<Vec<_>>();
        let records = self
            .contract
            .catalog
            .all_records()
            .map(|r| self.lower_record(r))
            .collect::<Vec<_>>();
        let record_readers = self.lower_record_readers();
        let record_writers = self.lower_record_writers();
        let closures = self.lower_closures();
        let functions = self
            .contract
            .functions
            .iter()
            .map(|function| self.lower_function(function))
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_class(class))
            .collect::<Vec<_>>();
        let callbacks = self
            .contract
            .catalog
            .all_callbacks()
            .map(|c| self.lower_callback_trait(c))
            .collect::<Vec<_>>();
        let native = self.lower_native();

        KotlinModule {
            package_name: self.package_name.clone(),
            prefix: preamble.prefix,
            extra_imports: preamble.extra_imports,
            custom_types: preamble.custom_types,
            enums,
            data_enum_codecs,
            records,
            record_readers,
            record_writers,
            closures,
            functions,
            classes,
            callbacks,
            native,
            api_style: match self.options.api_style {
                KotlinInputApiStyle::TopLevel => KotlinApiStyle::TopLevel,
                KotlinInputApiStyle::ModuleObject => KotlinApiStyle::ModuleObject,
            },
            module_object_name: self.options.module_object_name.clone(),
            has_async_runtime,
            has_streams,
        }
    }

    fn lower_preamble(&self, has_async_runtime: bool, has_streams: bool) -> KotlinPreamble {
        let extra_imports = self.collect_extra_imports(has_async_runtime, has_streams);
        let custom_types = self
            .contract
            .catalog
            .all_custom_types()
            .map(|custom| self.lower_custom_type(custom))
            .collect::<Vec<_>>();

        KotlinPreamble {
            prefix: naming::ffi_prefix().to_string(),
            extra_imports,
            custom_types,
            has_async_runtime,
            has_streams,
        }
    }

    fn collect_extra_imports(&self, has_async_runtime: bool, has_streams: bool) -> Vec<String> {
        let mut imports = self
            .collect_builtin_ids()
            .into_iter()
            .filter_map(|id| self.builtin_import(&id))
            .collect::<Vec<_>>();
        let coroutine_imports = if has_async_runtime {
            vec![
                "kotlinx.coroutines.CoroutineScope".to_string(),
                "kotlinx.coroutines.Dispatchers".to_string(),
                "kotlinx.coroutines.SupervisorJob".to_string(),
                "kotlinx.coroutines.Job".to_string(),
                "kotlinx.coroutines.launch".to_string(),
            ]
        } else {
            Vec::new()
        };
        let stream_imports = if has_streams {
            vec![
                "java.util.concurrent.atomic.AtomicInteger".to_string(),
                "kotlinx.coroutines.channels.awaitClose".to_string(),
                "kotlinx.coroutines.flow.Flow".to_string(),
                "kotlinx.coroutines.flow.callbackFlow".to_string(),
            ]
        } else {
            Vec::new()
        };
        let builtin_imports = vec![
            "java.time.Duration".to_string(),
            "java.time.Instant".to_string(),
            "java.util.UUID".to_string(),
            "java.net.URI".to_string(),
        ];
        coroutine_imports
            .into_iter()
            .chain(stream_imports)
            .chain(builtin_imports)
            .for_each(|import| {
                if !imports.iter().any(|item| item == &import) {
                    imports.push(import);
                }
            });
        imports
    }

    fn has_async_runtime(&self, has_streams: bool) -> bool {
        has_streams
            || self
                .contract
                .functions
                .iter()
                .any(|function| function.execution_kind() == ExecutionKind::Async)
            || self.contract.catalog.all_classes().any(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.execution_kind() == ExecutionKind::Async)
            })
            || self.contract.catalog.all_callbacks().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| method.execution_kind() == ExecutionKind::Async)
            })
    }

    fn collect_builtin_ids(&self) -> HashSet<BuiltinId> {
        let mut used = HashSet::new();
        self.contract
            .functions
            .iter()
            .for_each(|function| self.collect_builtins_from_function(function, &mut used));
        self.contract.catalog.all_classes().for_each(|class| {
            class
                .constructors
                .iter()
                .for_each(|ctor| self.collect_builtins_from_constructor(ctor, &mut used));
            class
                .methods
                .iter()
                .for_each(|method| self.collect_builtins_from_method(method, &mut used));
            class
                .streams
                .iter()
                .for_each(|stream| Self::collect_builtins_from_type(&stream.item_type, &mut used));
        });
        self.contract.catalog.all_records().for_each(|record| {
            record
                .fields
                .iter()
                .for_each(|field| Self::collect_builtins_from_type(&field.type_expr, &mut used))
        });
        self.contract.catalog.all_enums().for_each(|enumeration| {
            if let EnumRepr::Data { variants, .. } = &enumeration.repr {
                variants.iter().for_each(|variant| match &variant.payload {
                    VariantPayload::Struct(fields) => fields.iter().for_each(|field| {
                        Self::collect_builtins_from_type(&field.type_expr, &mut used)
                    }),
                    VariantPayload::Tuple(fields) => fields
                        .iter()
                        .for_each(|ty| Self::collect_builtins_from_type(ty, &mut used)),
                    VariantPayload::Unit => {}
                })
            }
        });
        self.contract
            .catalog
            .all_custom_types()
            .for_each(|custom| Self::collect_builtins_from_type(&custom.repr, &mut used));
        self.contract.catalog.all_callbacks().for_each(|callback| {
            callback.methods.iter().for_each(|method| {
                method.params.iter().for_each(|param| {
                    Self::collect_builtins_from_type(&param.type_expr, &mut used)
                });
                self.collect_builtins_from_return(&method.returns, &mut used);
            })
        });
        used
    }

    fn collect_builtins_from_function(&self, func: &FunctionDef, used: &mut HashSet<BuiltinId>) {
        func.params
            .iter()
            .for_each(|param| Self::collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&func.returns, used);
    }

    fn collect_builtins_from_constructor(
        &self,
        ctor: &ConstructorDef,
        used: &mut HashSet<BuiltinId>,
    ) {
        ctor.params()
            .iter()
            .for_each(|param| Self::collect_builtins_from_type(&param.type_expr, used));
    }

    fn collect_builtins_from_method(&self, method: &MethodDef, used: &mut HashSet<BuiltinId>) {
        method
            .params
            .iter()
            .for_each(|param| Self::collect_builtins_from_type(&param.type_expr, used));
        self.collect_builtins_from_return(&method.returns, used);
    }

    fn collect_builtins_from_return(&self, returns: &ReturnDef, used: &mut HashSet<BuiltinId>) {
        match returns {
            ReturnDef::Void => {}
            ReturnDef::Value(ty) => Self::collect_builtins_from_type(ty, used),
            ReturnDef::Result { ok, err } => {
                Self::collect_builtins_from_type(ok, used);
                Self::collect_builtins_from_type(err, used);
            }
        }
    }

    fn collect_builtins_from_type(ty: &TypeExpr, used: &mut HashSet<BuiltinId>) {
        match ty {
            TypeExpr::Builtin(id) => {
                used.insert(id.clone());
            }
            TypeExpr::Option(inner) | TypeExpr::Vec(inner) => {
                Self::collect_builtins_from_type(inner, used)
            }
            TypeExpr::Result { ok, err } => {
                Self::collect_builtins_from_type(ok, used);
                Self::collect_builtins_from_type(err, used);
            }
            _ => {}
        }
    }

    fn builtin_import(&self, id: &BuiltinId) -> Option<String> {
        match id.as_str() {
            "Duration" => Some("java.time.Duration".to_string()),
            "SystemTime" => Some("java.time.Instant".to_string()),
            "Uuid" => Some("java.util.UUID".to_string()),
            "Url" => Some("java.net.URI".to_string()),
            _ => None,
        }
    }

    fn lower_custom_type(&self, custom: &CustomTypeDef) -> KotlinCustomType {
        let class_name = NamingConvention::class_name(custom.id.as_str());
        let repr_kotlin_type = self.kotlin_type(&custom.repr);
        let custom_seq = self.custom_read_seq(custom);
        let repr_decode_expr = emit::emit_reader_read(&custom_seq);
        let custom_write_seq = self.custom_write_seq(custom);
        let repr_encode_expr = emit::emit_write_expr(&custom_write_seq);
        let repr_size_expr = emit::emit_size_expr_for_write_seq(&custom_write_seq);
        let mapping = self.type_mappings.get(custom.id.as_str());
        let (native_type, native_decode_expr, native_encode_expr) = mapping
            .map(|mapping| {
                let (decode_wrapper, encode_wrapper) =
                    self.custom_native_conversion_wrappers(mapping);
                (
                    Some(mapping.native_type.clone()),
                    Some(decode_wrapper.replace("$0", &repr_decode_expr)),
                    Some(encode_wrapper.replace("$0", "this")),
                )
            })
            .unwrap_or((None, None, None));
        let has_native_mapping = mapping.is_some();

        KotlinCustomType {
            class_name,
            native_type,
            repr_kotlin_type,
            repr_size_expr,
            repr_encode_expr,
            repr_decode_expr,
            native_decode_expr,
            native_encode_expr,
            has_native_mapping,
        }
    }

    fn custom_native_conversion_wrappers(
        &self,
        mapping: &crate::render::TypeMapping,
    ) -> (String, String) {
        match mapping.conversion {
            TypeConversion::UuidString => (
                "UUID.fromString($0)".to_string(),
                "$0.toString()".to_string(),
            ),
            TypeConversion::UrlString => {
                ("URI.create($0)".to_string(), "$0.toString()".to_string())
            }
        }
    }

    fn native_conversion_for_type(&self, type_expr: &TypeExpr) -> Option<(String, String)> {
        match type_expr {
            TypeExpr::Custom(id) => self
                .type_mappings
                .get(id.as_str())
                .map(|mapping| self.custom_native_conversion_wrappers(mapping)),
            _ => None,
        }
    }

    fn apply_native_decode_conversion(&self, type_expr: &TypeExpr, decode_expr: String) -> String {
        self.native_conversion_for_type(type_expr)
            .map(|(decode_wrapper, _)| decode_wrapper.replace("$0", &decode_expr))
            .unwrap_or(decode_expr)
    }

    fn decode_expr_with_native_conversion(
        &self,
        type_expr: &TypeExpr,
        decode_seq: &ReadSeq,
    ) -> String {
        match (type_expr, decode_seq.ops.first()) {
            (TypeExpr::Option(inner), Some(ReadOp::Option { some, .. })) => {
                let inner_expr = self.decode_expr_with_native_conversion(inner, some);
                format!("reader.readOptional {{ {} }}", inner_expr)
            }
            (
                TypeExpr::Vec(inner),
                Some(ReadOp::Vec {
                    element_type,
                    element,
                    layout,
                    ..
                }),
            ) => self.decode_vec_expr_with_native_conversion(inner, element_type, element, layout),
            (
                TypeExpr::Result { ok, err },
                Some(ReadOp::Result {
                    ok: ok_seq,
                    err: err_seq,
                    ..
                }),
            ) => {
                let ok_expr = self.decode_expr_with_native_conversion(ok, ok_seq);
                let err_expr = self.decode_expr_with_native_conversion(err, err_seq);
                format!("reader.readResult({{ {} }}, {{ {} }})", ok_expr, err_expr)
            }
            _ => {
                let base_decode = emit::emit_reader_read(decode_seq);
                self.apply_native_decode_conversion(type_expr, base_decode)
            }
        }
    }

    fn decode_vec_expr_with_native_conversion(
        &self,
        inner_type: &TypeExpr,
        element_type: &TypeExpr,
        element: &ReadSeq,
        layout: &VecLayout,
    ) -> String {
        match layout {
            VecLayout::Blittable { .. } => match element_type {
                TypeExpr::Primitive(primitive) => {
                    let method = match primitive {
                        PrimitiveType::I32 | PrimitiveType::U32 => "readIntArray",
                        PrimitiveType::I16 | PrimitiveType::U16 => "readShortArray",
                        PrimitiveType::I64
                        | PrimitiveType::U64
                        | PrimitiveType::ISize
                        | PrimitiveType::USize => "readLongArray",
                        PrimitiveType::F32 => "readFloatArray",
                        PrimitiveType::F64 => "readDoubleArray",
                        PrimitiveType::U8 | PrimitiveType::I8 => "readBytes",
                        PrimitiveType::Bool => "readBooleanArray",
                    };
                    format!("reader.{}()", method)
                }
                _ => {
                    let inner_expr = self.decode_expr_with_native_conversion(inner_type, element);
                    format!("reader.readList {{ {} }}", inner_expr)
                }
            },
            VecLayout::Encoded => {
                let inner_expr = self.decode_expr_with_native_conversion(inner_type, element);
                format!("reader.readList {{ {} }}", inner_expr)
            }
        }
    }

    fn apply_native_encode_conversion(
        &self,
        type_expr: &TypeExpr,
        encode_expr: String,
        field_name: &str,
    ) -> String {
        self.apply_native_encode_conversion_for_binding(type_expr, encode_expr, field_name)
    }

    fn apply_native_encode_conversion_for_binding(
        &self,
        type_expr: &TypeExpr,
        encode_expr: String,
        binding_name: &str,
    ) -> String {
        match type_expr {
            TypeExpr::Custom(_) => self
                .native_conversion_for_type(type_expr)
                .map(|(_, encode_wrapper)| {
                    let converted = encode_wrapper.replace("$0", binding_name);
                    Self::replace_identifier_occurrences(&encode_expr, binding_name, &converted)
                })
                .unwrap_or(encode_expr),
            TypeExpr::Option(inner) => {
                self.apply_native_encode_conversion_for_binding(inner, encode_expr, "v")
            }
            TypeExpr::Vec(inner) => {
                self.apply_native_encode_conversion_for_binding(inner, encode_expr, "item")
            }
            TypeExpr::Result { ok, err } => {
                let with_ok =
                    self.apply_native_encode_conversion_for_binding(ok, encode_expr, "okVal");
                self.apply_native_encode_conversion_for_binding(err, with_ok, "errVal")
            }
            _ => encode_expr,
        }
    }

    fn replace_identifier_occurrences(
        expression: &str,
        identifier: &str,
        replacement: &str,
    ) -> String {
        if identifier.is_empty() {
            return expression.to_string();
        }

        let mut result = String::with_capacity(expression.len());
        let mut cursor = 0;

        while let Some(relative_index) = expression[cursor..].find(identifier) {
            let start = cursor + relative_index;
            let end = start + identifier.len();
            let previous = expression[..start].chars().next_back();
            let next = expression[end..].chars().next();
            let previous_is_identifier = previous.map(Self::is_identifier_char).unwrap_or(false);
            let next_is_identifier = next.map(Self::is_identifier_char).unwrap_or(false);

            if previous_is_identifier || next_is_identifier {
                result.push_str(&expression[cursor..end]);
                cursor = end;
            } else {
                result.push_str(&expression[cursor..start]);
                result.push_str(replacement);
                cursor = end;
            }
        }

        result.push_str(&expression[cursor..]);
        result
    }

    fn is_identifier_char(character: char) -> bool {
        character.is_ascii_alphanumeric() || character == '_'
    }

    fn custom_read_seq(&self, custom: &CustomTypeDef) -> ReadSeq {
        self.find_custom_read_seq(&custom.id)
            .unwrap_or_else(|| self.read_seq_from_repr(&custom.repr))
    }

    fn custom_write_seq(&self, custom: &CustomTypeDef) -> WriteSeq {
        let base_seq = self
            .find_custom_write_seq(&custom.id)
            .unwrap_or_else(|| self.write_seq_from_repr(&custom.repr));
        let remapped = remap_root_in_seq(&base_seq, ValueExpr::Var("repr".to_string()));
        self.normalize_custom_write_seq(&custom.repr, remapped)
    }

    fn normalize_custom_write_seq(&self, repr: &TypeExpr, seq: WriteSeq) -> WriteSeq {
        let _ = repr;
        self.strip_field_access_in_write_seq(&seq)
    }

    fn strip_field_access_in_write_seq(&self, seq: &WriteSeq) -> WriteSeq {
        WriteSeq {
            size: Self::strip_field_access_in_size(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.strip_field_access_in_write_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn strip_field_access_in_size(size: &SizeExpr) -> SizeExpr {
        match size {
            SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
            SizeExpr::Runtime => SizeExpr::Runtime,
            SizeExpr::StringLen(value) => {
                SizeExpr::StringLen(Self::strip_field_access_in_value(value))
            }
            SizeExpr::BytesLen(value) => {
                SizeExpr::BytesLen(Self::strip_field_access_in_value(value))
            }
            SizeExpr::ValueSize(value) => {
                SizeExpr::ValueSize(Self::strip_field_access_in_value(value))
            }
            SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
                value: Self::strip_field_access_in_value(value),
                owner: owner.clone(),
            },
            SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
                id: id.clone(),
                value: Self::strip_field_access_in_value(value),
            },
            SizeExpr::Sum(parts) => {
                SizeExpr::Sum(parts.iter().map(Self::strip_field_access_in_size).collect())
            }
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: Self::strip_field_access_in_value(value),
                inner: Box::new(Self::strip_field_access_in_size(inner)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: Self::strip_field_access_in_value(value),
                inner: Box::new(Self::strip_field_access_in_size(inner)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: Self::strip_field_access_in_value(value),
                ok: Box::new(Self::strip_field_access_in_size(ok)),
                err: Box::new(Self::strip_field_access_in_size(err)),
            },
        }
    }

    fn strip_field_access_in_write_op(&self, op: &WriteOp) -> WriteOp {
        match op {
            WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
                primitive: *primitive,
                value: Self::strip_field_access_in_value(value),
            },
            WriteOp::String { value } => WriteOp::String {
                value: Self::strip_field_access_in_value(value),
            },
            WriteOp::Bytes { value } => WriteOp::Bytes {
                value: Self::strip_field_access_in_value(value),
            },
            WriteOp::Builtin { id, value } => WriteOp::Builtin {
                id: id.clone(),
                value: Self::strip_field_access_in_value(value),
            },
            WriteOp::Option { value, some } => WriteOp::Option {
                value: Self::strip_field_access_in_value(value),
                some: Box::new(self.strip_field_access_in_write_seq(some)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: Self::strip_field_access_in_value(value),
                element_type: element_type.clone(),
                element: Box::new(self.strip_field_access_in_write_seq(element)),
                layout: layout.clone(),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: Self::strip_field_access_in_value(value),
                fields: fields
                    .iter()
                    .map(|field| FieldWriteOp {
                        name: field.name.clone(),
                        accessor: Self::strip_field_access_in_value(&field.accessor),
                        seq: self.strip_field_access_in_write_seq(&field.seq),
                    })
                    .collect(),
            },
            WriteOp::Enum { id, value, layout } => WriteOp::Enum {
                id: id.clone(),
                value: Self::strip_field_access_in_value(value),
                layout: layout.clone(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: Self::strip_field_access_in_value(value),
                ok: Box::new(self.strip_field_access_in_write_seq(ok)),
                err: Box::new(self.strip_field_access_in_write_seq(err)),
            },
            WriteOp::Custom {
                id,
                value,
                underlying,
            } => WriteOp::Custom {
                id: id.clone(),
                value: Self::strip_field_access_in_value(value),
                underlying: Box::new(self.strip_field_access_in_write_seq(underlying)),
            },
        }
    }

    fn strip_field_access_in_value(value: &ValueExpr) -> ValueExpr {
        match value {
            ValueExpr::Field(parent, name) => {
                let stripped_parent = Self::strip_field_access_in_value(parent);
                match &stripped_parent {
                    ValueExpr::Var(var) if var == "repr" => ValueExpr::Var("repr".to_string()),
                    ValueExpr::Named(name) if name == "repr" => ValueExpr::Var("repr".to_string()),
                    _ => ValueExpr::Field(Box::new(stripped_parent), name.clone()),
                }
            }
            ValueExpr::Instance => ValueExpr::Var("repr".to_string()),
            ValueExpr::Var(_) | ValueExpr::Named(_) => value.clone(),
        }
    }

    fn lower_enum(&self, enumeration: &EnumDef) -> KotlinEnum {
        let abi_enum = self.abi_enum_for(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let c_style_value_type = match &enumeration.repr {
            EnumRepr::CStyle { tag_type, .. } => Some(self.primitive_jni_type(*tag_type)),
            _ => None,
        };
        let kind = if enumeration.is_error {
            KotlinEnumKind::Error
        } else if abi_enum.is_c_style {
            KotlinEnumKind::CStyle
        } else {
            KotlinEnumKind::Sealed
        };
        let variant_names = abi_enum
            .variants
            .iter()
            .map(|variant| NamingConvention::class_name(variant.name.as_str()))
            .collect::<HashSet<_>>();
        let variant_docs = enumeration.variant_docs();
        let variants = abi_enum
            .variants
            .iter()
            .enumerate()
            .map(|(i, variant)| {
                let mut v = self.lower_enum_variant(abi_enum, variant, i, kind, &variant_names);
                v.doc = variant_docs.get(i).cloned().flatten();
                v
            })
            .collect::<Vec<_>>();
        let constructors = enumeration
            .constructor_calls()
            .map(|(call_id, ctor)| {
                self.lower_value_type_constructor(
                    ctor,
                    self.find_abi_call(&call_id),
                    TypeExpr::Enum(enumeration.id.clone()),
                    KotlinConstructorSurface::CompanionFactory,
                )
            })
            .collect::<Vec<_>>();
        let mut methods = enumeration
            .method_calls()
            .map(|(call_id, method)| {
                self.lower_value_type_method(method, &call_id, enumeration.id.as_str())
            })
            .collect::<Vec<_>>();
        if matches!(enumeration.repr, EnumRepr::Data { .. }) {
            for method in &mut methods {
                match &mut method.impl_ {
                    KotlinMethodImpl::SyncMethod(src) | KotlinMethodImpl::AsyncMethod(src) => {
                        self.qualify_collisions_in_string(src, &variant_names);
                    }
                }
            }
        }
        KotlinEnum {
            class_name,
            variants,
            kind,
            c_style_value_type,
            constructors,
            methods,
            doc: enumeration.doc.clone(),
        }
    }

    fn lower_enum_variant(
        &self,
        abi_enum: &AbiEnum,
        variant: &AbiEnumVariant,
        ordinal: usize,
        kind: KotlinEnumKind,
        variant_names: &HashSet<String>,
    ) -> KotlinEnumVariant {
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|field| self.lower_enum_field(field, variant_names))
                .collect(),
        };
        let name = match kind {
            KotlinEnumKind::CStyle => NamingConvention::enum_entry_name(variant.name.as_str()),
            _ => NamingConvention::class_name(variant.name.as_str()),
        };
        KotlinEnumVariant {
            name,
            tag: self.kotlin_enum_variant_tag(abi_enum, kind, ordinal, variant.discriminant),
            fields,
            doc: None,
        }
    }

    fn lower_enum_field(
        &self,
        field: &AbiEnumField,
        variant_names: &HashSet<String>,
    ) -> KotlinEnumField {
        let (mut kotlin_type, decode_name) =
            self.kotlin_type_with_disambiguation(&field.type_expr, variant_names);
        let field_name = NamingConvention::property_name(field.name.as_str());
        let base_decode = self.decode_expr_with_native_conversion(&field.type_expr, &field.decode);
        let mut wire_decode_expr = self.qualify_decode_expr(base_decode, decode_name.as_deref());
        let base_size = emit::emit_size_expr_for_write_seq(&field.encode);
        let mut wire_size_expr =
            self.apply_native_encode_conversion(&field.type_expr, base_size, &field_name);
        let base_encode = emit::emit_write_expr(&field.encode);
        let mut wire_encode =
            self.apply_native_encode_conversion(&field.type_expr, base_encode, &field_name);
        self.qualify_collisions_in_string(&mut kotlin_type, variant_names);
        self.qualify_collisions_in_string(&mut wire_decode_expr, variant_names);
        self.qualify_collisions_in_string(&mut wire_size_expr, variant_names);
        self.qualify_collisions_in_string(&mut wire_encode, variant_names);
        KotlinEnumField {
            name: field_name,
            kotlin_type,
            wire_decode_expr,
            wire_size_expr,
            wire_encode,
        }
    }

    fn lower_data_enum_codec(&self, enumeration: &EnumDef) -> KotlinDataEnumCodec {
        let abi_enum = self.abi_enum_for(enumeration);
        let layout = self.data_enum_layout(enumeration);
        let class_name = NamingConvention::class_name(enumeration.id.as_str());
        let codec_name = format!("{}Codec", class_name);
        let variants = match (&enumeration.repr, &layout) {
            (EnumRepr::Data { variants, .. }, Some(layout)) => variants
                .iter()
                .enumerate()
                .map(|(index, variant)| KotlinDataEnumVariant {
                    name: NamingConvention::class_name(variant.name.as_str()),
                    const_name: variant.name.as_str().to_uppercase(),
                    tag_value: abi_enum.resolve_codec_tag(index, variant.discriminant),
                    fields: self.lower_data_enum_codec_fields(
                        &variant.payload,
                        layout.variant_offsets.get(index),
                    ),
                })
                .collect(),
            _ => Vec::new(),
        };
        KotlinDataEnumCodec {
            class_name,
            codec_name,
            struct_size: layout.as_ref().map(|l| l.struct_size).unwrap_or(0),
            payload_offset: layout.as_ref().map(|l| l.payload_offset).unwrap_or(0),
            variants,
        }
    }

    fn kotlin_enum_variant_tag(
        &self,
        abi_enum: &AbiEnum,
        kind: KotlinEnumKind,
        ordinal: usize,
        discriminant: i128,
    ) -> i128 {
        match kind {
            KotlinEnumKind::CStyle => discriminant,
            KotlinEnumKind::Sealed | KotlinEnumKind::Error => match abi_enum.codec_tag_strategy {
                EnumTagStrategy::Discriminant => discriminant,
                EnumTagStrategy::OrdinalIndex => abi_enum.resolve_codec_tag(ordinal, discriminant),
            },
        }
    }

    fn lower_data_enum_codec_fields(
        &self,
        payload: &VariantPayload,
        offsets: Option<&Vec<usize>>,
    ) -> Vec<KotlinDataEnumField> {
        let Some(offsets) = offsets else {
            return Vec::new();
        };

        match payload {
            VariantPayload::Unit => Vec::new(),
            VariantPayload::Struct(fields) => fields
                .iter()
                .zip(offsets.iter().copied())
                .filter_map(|(field, offset)| match &field.type_expr {
                    TypeExpr::Primitive(primitive) => {
                        let param_name = NamingConvention::property_name(field.name.as_str());
                        Some(self.data_enum_field_for_primitive(*primitive, param_name, offset))
                    }
                    _ => None,
                })
                .collect(),
            VariantPayload::Tuple(types) => types
                .iter()
                .enumerate()
                .zip(offsets.iter().copied())
                .filter_map(|((index, type_expr), offset)| match type_expr {
                    TypeExpr::Primitive(primitive) => {
                        let base_name = format!("value_{}", index);
                        let param_name = NamingConvention::property_name(base_name.as_str());
                        Some(self.data_enum_field_for_primitive(*primitive, param_name, offset))
                    }
                    _ => None,
                })
                .collect(),
        }
    }

    fn data_enum_field_for_primitive(
        &self,
        primitive: PrimitiveType,
        param_name: String,
        offset: usize,
    ) -> KotlinDataEnumField {
        let (getter, putter, conversion) = self.primitive_field_accessors(primitive);
        let value_expr =
            self.primitive_write_value_expr(primitive, &format!("value.{}", param_name));
        KotlinDataEnumField {
            param_name,
            value_expr,
            offset,
            getter,
            putter,
            conversion,
        }
    }

    fn lower_record(&self, record: &RecordDef) -> KotlinRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(record, field))
            .collect::<Vec<_>>();
        let constructors = record
            .constructor_calls()
            .map(|(call_id, ctor)| {
                self.lower_value_type_constructor(
                    ctor,
                    self.find_abi_call(&call_id),
                    TypeExpr::Record(record.id.clone()),
                    KotlinConstructorSurface::CompanionFactory,
                )
            })
            .collect::<Vec<_>>();
        let methods = record
            .method_calls()
            .map(|(call_id, method)| {
                self.lower_value_type_method(method, &call_id, record.id.as_str())
            })
            .collect::<Vec<_>>();
        KotlinRecord {
            class_name,
            fields,
            is_blittable: record.is_blittable(),
            is_error: record.is_error,
            struct_size: self.record_struct_size(record.id.as_str()),
            constructors,
            methods,
            doc: record.doc.clone(),
        }
    }

    fn lower_record_field(&self, record: &RecordDef, field: &FieldDef) -> KotlinRecordField {
        let decode_seq = self
            .record_field_read_seq(&record.id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(&record.id, &field.name)
            .expect("record field encode ops");
        let field_name = NamingConvention::property_name(field.name.as_str());
        let wire_decode_expr =
            self.decode_expr_with_native_conversion(&field.type_expr, &decode_seq);
        let base_size = emit::emit_size_expr_for_write_seq(&encode_seq);
        let wire_size_expr =
            self.apply_native_encode_conversion(&field.type_expr, base_size, &field_name);
        let base_encode = emit::emit_write_expr(&encode_seq);
        let wire_encode =
            self.apply_native_encode_conversion(&field.type_expr, base_encode, &field_name);
        KotlinRecordField {
            name: field_name,
            kotlin_type: self.kotlin_type(&field.type_expr),
            default_value: field
                .default
                .as_ref()
                .map(|d| kotlin_default_literal(d, &self.kotlin_type(&field.type_expr))),
            wire_decode_expr,
            wire_size_expr,
            wire_encode,
            padding_after: self.field_padding_after(&record.id, &field.name),
            doc: field.doc.clone(),
        }
    }

    fn lower_record_readers(&self) -> Vec<KotlinRecordReader> {
        let record_ids = self.blittable_reader_record_ids();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let reader_name =
                    format!("{}Reader", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordReader {
                    reader_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (getter, _, conversion) =
                                self.primitive_field_accessors(field.primitive);
                            KotlinRecordReaderField {
                                name: NamingConvention::property_name(field.name.as_str()),
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                getter,
                                conversion,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_record_writers(&self) -> Vec<KotlinRecordWriter> {
        let record_ids = self.blittable_vec_param_records();
        self.contract
            .catalog
            .all_records()
            .filter(|record| record_ids.contains(record.id.as_str()))
            .filter_map(|record| {
                let fields = self.record_blittable_fields(&record.id)?;
                let writer_name =
                    format!("{}Writer", NamingConvention::class_name(record.id.as_str()));
                Some(KotlinRecordWriter {
                    writer_name,
                    class_name: NamingConvention::class_name(record.id.as_str()),
                    struct_size: self.record_struct_size(record.id.as_str()),
                    fields: fields
                        .iter()
                        .map(|field| {
                            let (_, putter, _) = self.primitive_field_accessors(field.primitive);
                            let value_expr = self.primitive_write_value_expr(
                                field.primitive,
                                &format!(
                                    "item.{}",
                                    NamingConvention::property_name(field.name.as_str())
                                ),
                            );
                            KotlinRecordWriterField {
                                const_name: field.name.as_str().to_uppercase(),
                                offset: field.offset,
                                putter,
                                value_expr,
                            }
                        })
                        .collect(),
                })
            })
            .collect()
    }

    fn lower_closures(&self) -> Vec<KotlinClosureInterface> {
        self.contract
            .catalog
            .all_callbacks()
            .filter(|callback| matches!(callback.kind, CallbackKind::Closure))
            .filter_map(|callback| callback.methods.first().map(|method| (callback, method)))
            .map(|(callback, method)| KotlinClosureInterface {
                interface_name: self.closure_interface_name(callback.id.as_str()),
                params: method
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| KotlinSignatureParam {
                        name: format!("p{}", index),
                        kotlin_type: self.closure_param_type(&param.type_expr),
                    })
                    .collect(),
                return_type: match &method.returns {
                    ReturnDef::Void => None,
                    _ => Some(self.kotlin_type_from_return_def(&method.returns)),
                },
            })
            .collect()
    }

    fn lower_function(&self, func: &FunctionDef) -> KotlinFunction {
        let call = self.abi_call_for_function(func);
        let output_route = &call.returns;
        let signature_params = func
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .filter_map(KotlinWireWriter::cleanup_code)
            .collect();
        let native_args = self.native_args_for_params(call, &func.params, &wire_writers);
        let return_type = self.kotlin_return_type_from_def(&func.returns, output_route);
        let return_meta = self.kotlin_host_return_meta(output_route, &func.returns);
        let decode_expr = self.decode_expr_for_call_return(output_route, &func.returns);
        let is_blittable_return = self.is_blittable_return(output_route, &func.returns);
        let async_call = match &call.mode {
            CallMode::Async(_) => Some(self.async_call_for_function(func, call)),
            CallMode::Sync => None,
        };
        KotlinFunction {
            func_name: NamingConvention::method_name(func.id.as_str()),
            signature_params,
            return_type,
            wire_writers,
            wire_writer_closes,
            native_args,
            throws: self.is_throwing_return(&func.returns),
            err_type: self.error_type_name(&func.returns),
            ffi_name: call.symbol.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            direct_return_is_nullable: return_meta.direct_is_nullable,
            return_cast: return_meta.cast,
            async_call,
            decode_expr,
            is_blittable_return,
            doc: func.doc.clone(),
        }
    }

    fn lower_class(&self, class: &ClassDef) -> KotlinClass {
        let class_name = NamingConvention::class_name(class.id.as_str());
        let constructor_surfaces = self.class_constructor_surfaces(&class.constructors);
        let constructors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, ctor)| {
                let call = self.abi_call_for_constructor(class, index);
                self.lower_constructor(ctor, call, constructor_surfaces[index])
            })
            .collect::<Vec<_>>();
        let methods = class
            .methods
            .iter()
            .map(|method| self.lower_method(class, method))
            .collect::<Vec<_>>();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                self.lower_stream(stream, abi_stream, &class_name)
            })
            .collect::<Vec<_>>();
        KotlinClass {
            class_name,
            doc: class.doc.clone(),
            prefix: naming::ffi_prefix().to_string(),
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            constructors,
            methods,
            streams,
            use_companion_methods: matches!(
                self.options.factory_style,
                FactoryStyle::CompanionMethods
            ),
        }
    }

    fn lower_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        surface: KotlinConstructorSurface,
    ) -> KotlinConstructor {
        let name = match ctor {
            ConstructorDef::Default { .. } => "new".to_string(),
            ConstructorDef::NamedFactory { name, .. } => name.as_str().to_string(),
            ConstructorDef::NamedInit { name, .. } => name.as_str().to_string(),
        };
        let signature_params = ctor
            .params()
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .filter_map(KotlinWireWriter::cleanup_code)
            .collect();
        let ctor_param_defs: Vec<ParamDef> = ctor.params().into_iter().cloned().collect();
        let native_args = self.native_args_for_params(call, &ctor_param_defs, &wire_writers);
        KotlinConstructor {
            name: NamingConvention::method_name(&name),
            surface,
            is_fallible: ctor.is_fallible(),
            return_type: None,
            throws: false,
            err_type: "FfiException".to_string(),
            return_is_direct: false,
            return_cast: String::new(),
            decode_expr: String::new(),
            is_blittable_return: false,
            signature_params,
            wire_writers,
            wire_writer_closes,
            native_args,
            ffi_name: call.symbol.as_str().to_string(),
            doc: ctor.doc().map(String::from),
        }
    }

    fn lower_value_type_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        owner_type: TypeExpr,
        surface: KotlinConstructorSurface,
    ) -> KotlinConstructor {
        let returns = if ctor.is_fallible() {
            ReturnDef::Result {
                ok: owner_type.clone(),
                err: TypeExpr::String,
            }
        } else if ctor.is_optional() {
            ReturnDef::Value(TypeExpr::Option(Box::new(owner_type.clone())))
        } else {
            ReturnDef::Value(owner_type)
        };
        let output_route = &call.returns;
        let return_type = self.kotlin_return_type_from_def(&returns, output_route);
        let return_meta = self.kotlin_host_return_meta(output_route, &returns);
        let decode_expr = self.decode_expr_for_call_return(output_route, &returns);
        let is_blittable_return = self.is_blittable_return(output_route, &returns);
        let err_type = self.error_type_name(&returns);
        let mut constructor = self.lower_constructor(ctor, call, surface);
        constructor.return_type = return_type;
        constructor.throws = self.is_throwing_return(&returns);
        constructor.err_type = err_type;
        constructor.return_is_direct = return_meta.is_direct;
        constructor.return_cast = return_meta.cast;
        constructor.decode_expr = decode_expr;
        constructor.is_blittable_return = is_blittable_return;
        constructor
    }

    fn class_constructor_surfaces(
        &self,
        constructors: &[ConstructorDef],
    ) -> Vec<KotlinConstructorSurface> {
        let prefer_companion_methods =
            matches!(self.options.factory_style, FactoryStyle::CompanionMethods);
        let mut surfaces = constructors
            .iter()
            .map(|constructor| match constructor {
                ConstructorDef::Default { .. } => KotlinConstructorSurface::Constructor,
                ConstructorDef::NamedFactory { .. } => KotlinConstructorSurface::CompanionFactory,
                ConstructorDef::NamedInit { .. } if prefer_companion_methods => {
                    KotlinConstructorSurface::CompanionFactory
                }
                ConstructorDef::NamedInit { .. } => KotlinConstructorSurface::Constructor,
            })
            .collect::<Vec<_>>();

        if prefer_companion_methods {
            return surfaces;
        }

        let mut constructors_by_signature = HashMap::<Vec<String>, Vec<usize>>::new();
        constructors
            .iter()
            .enumerate()
            .filter(|(index, _)| matches!(surfaces[*index], KotlinConstructorSurface::Constructor))
            .for_each(|(index, constructor)| {
                constructors_by_signature
                    .entry(self.constructor_signature_key(constructor))
                    .or_default()
                    .push(index);
            });

        constructors_by_signature
            .into_values()
            .filter(|indices| indices.len() > 1)
            .for_each(|indices| {
                let preferred_index = indices
                    .iter()
                    .copied()
                    .min_by_key(|index| {
                        let constructor = &constructors[*index];
                        (
                            !matches!(constructor, ConstructorDef::Default { .. }),
                            constructor.is_fallible(),
                            *index,
                        )
                    })
                    .expect("constructor collision group must be non-empty");
                indices
                    .into_iter()
                    .filter(|index| *index != preferred_index)
                    .for_each(|index| surfaces[index] = KotlinConstructorSurface::CompanionFactory);
            });

        surfaces
    }

    fn constructor_signature_key(&self, constructor: &ConstructorDef) -> Vec<String> {
        constructor
            .params()
            .iter()
            .map(|param| self.kotlin_type(&param.type_expr))
            .collect()
    }

    fn lower_method(&self, class: &ClassDef, method: &MethodDef) -> KotlinMethod {
        let call = Self::strip_receiver(self.abi_call_for_method(class, method));
        let call = &call;
        let output_route = &call.returns;
        let wire_writers = self.wire_writers_for_params(call);
        let wire_writer_closes: Vec<String> = wire_writers
            .iter()
            .filter_map(KotlinWireWriter::cleanup_code)
            .collect();
        let native_args = self.native_args_for_params(call, &method.params, &wire_writers);
        let signature_params = method
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let return_type = self.kotlin_return_type_from_def(&method.returns, output_route);
        let return_meta = self.kotlin_host_return_meta(output_route, &method.returns);
        let decode_expr = self.decode_expr_for_call_return(output_route, &method.returns);
        let is_blittable_return = self.is_blittable_return(output_route, &method.returns);
        let ffi_name = call.symbol.as_str().to_string();
        let include_handle = method.receiver != Receiver::Static;
        let err_type = self.error_type_name(&method.returns);
        let rendered = if method.execution_kind() == ExecutionKind::Async {
            let async_call = self.async_call_for_method(class, method, call);
            AsyncMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                include_handle,
                ffi_poll: &async_call.poll,
                ffi_complete: &async_call.complete,
                ffi_cancel: &async_call.cancel,
                ffi_free: &async_call.free,
                return_is_unit: async_call.return_is_unit,
                return_is_direct: async_call.return_is_direct,
                direct_return_is_nullable: async_call.direct_return_is_nullable,
                return_cast: &async_call.return_cast,
                decode_expr: &async_call.decode_expr,
                is_blittable_return: async_call.is_blittable_return,
                doc: &method.doc,
            }
            .render()
            .unwrap()
        } else {
            WireMethodTemplate {
                method_name: &NamingConvention::method_name(method.id.as_str()),
                signature_params: &signature_params,
                return_type: return_type.as_deref(),
                wire_writers: &wire_writers,
                wire_writer_closes: &wire_writer_closes,
                native_args: &native_args,
                throws: self.is_throwing_return(&method.returns),
                err_type: &err_type,
                ffi_name: &ffi_name,
                return_is_unit: return_meta.is_unit,
                return_is_direct: return_meta.is_direct,
                direct_return_is_nullable: return_meta.direct_is_nullable,
                return_cast: &return_meta.cast,
                decode_expr: &decode_expr,
                is_blittable_return,
                include_handle,
                override_method: false,
                doc: &method.doc,
            }
            .render()
            .unwrap()
        };
        KotlinMethod {
            impl_: if method.execution_kind() == ExecutionKind::Async {
                KotlinMethodImpl::AsyncMethod(rendered)
            } else {
                KotlinMethodImpl::SyncMethod(rendered)
            },
            is_static: method.callable_form() == CallableForm::StaticMethod,
        }
    }

    fn lower_value_type_method(
        &self,
        method: &MethodDef,
        call_id: &CallId,
        type_name: &str,
    ) -> KotlinMethod {
        let call = self.find_abi_call(call_id);
        let call_without_self = Self::strip_self_param(call);
        let call_ref = &call_without_self;
        let output_route = &call_ref.returns;
        let wire_writers = self.wire_writers_for_params(call_ref);
        let native_args = self.native_args_for_params(call_ref, &method.params, &wire_writers);
        let signature_params = method
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();

        let mutating_void =
            method.receiver == Receiver::RefMutSelf && matches!(method.returns, ReturnDef::Void);

        let return_type = if mutating_void {
            Some(NamingConvention::class_name(type_name))
        } else {
            self.kotlin_return_type_from_def(&method.returns, output_route)
        };
        let return_meta = self.kotlin_host_return_meta(output_route, &method.returns);
        let decode_expr = self.decode_expr_for_call_return(output_route, &method.returns);
        let is_blittable_return = self.is_blittable_return(output_route, &method.returns);
        let ffi_name = call.symbol.as_str().to_string();
        let err_type = self.error_type_name(&method.returns);

        let self_wire = self.build_self_wire_writer(call);
        let self_native_arg = self.build_self_native_arg(call);

        let all_wire_writers: Vec<_> = self_wire.into_iter().chain(wire_writers).collect();
        let all_wire_writer_closes: Vec<String> = all_wire_writers
            .iter()
            .filter_map(KotlinWireWriter::cleanup_code)
            .collect();
        let all_native_args: Vec<_> = self_native_arg.into_iter().chain(native_args).collect();

        let rendered = WireMethodTemplate {
            method_name: &NamingConvention::method_name(method.id.as_str()),
            signature_params: &signature_params,
            return_type: return_type.as_deref(),
            wire_writers: &all_wire_writers,
            wire_writer_closes: &all_wire_writer_closes,
            native_args: &all_native_args,
            throws: self.is_throwing_return(&method.returns),
            err_type: &err_type,
            ffi_name: &ffi_name,
            return_is_unit: return_meta.is_unit && !mutating_void,
            return_is_direct: return_meta.is_direct,
            direct_return_is_nullable: return_meta.direct_is_nullable,
            return_cast: &return_meta.cast,
            decode_expr: &decode_expr,
            is_blittable_return,
            include_handle: false,
            override_method: false,
            doc: &method.doc,
        }
        .render()
        .unwrap();

        KotlinMethod {
            impl_: KotlinMethodImpl::SyncMethod(rendered),
            is_static: method.callable_form() == CallableForm::StaticMethod,
        }
    }

    fn strip_self_param(call: &AbiCall) -> AbiCall {
        AbiCall {
            params: call
                .params
                .iter()
                .filter(|p| p.name.as_str() != "self")
                .cloned()
                .collect(),
            ..call.clone()
        }
    }

    fn build_self_wire_writer(&self, call: &AbiCall) -> Vec<KotlinWireWriter> {
        let self_param = call.params.iter().find(|p| p.name.as_str() == "self");
        let Some(param) = self_param else {
            return vec![];
        };
        if let ParamRole::Input {
            encode_ops: Some(ops),
            ..
        } = &param.role
        {
            let remapped = remap_root_in_seq(ops, ValueExpr::Var("this".into()));
            vec![KotlinWireWriter::WireBuffer {
                binding_name: "wire_writer_self".to_string(),
                size_expr: emit::emit_size_expr_for_write_seq(&remapped),
                encode_expr: emit::emit_write_expr(&remapped),
            }]
        } else {
            vec![]
        }
    }

    fn build_self_native_arg(&self, call: &AbiCall) -> Vec<String> {
        let self_param = call.params.iter().find(|p| p.name.as_str() == "self");
        let Some(param) = self_param else {
            return vec![];
        };
        match &param.role {
            ParamRole::Input {
                encode_ops: Some(_),
                ..
            } => vec!["wire_writer_self.buffer".to_string()],
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => vec!["this.value".to_string()],
            _ => vec![],
        }
    }

    fn lower_stream(
        &self,
        stream_def: &StreamDef,
        stream: &AbiStream,
        class_name: &str,
    ) -> KotlinStream {
        let method_name_pascal = NamingConvention::class_name(stream.stream_id.as_str());
        let mode = match stream.mode {
            StreamMode::Async => KotlinStreamMode::Async,
            StreamMode::Batch => KotlinStreamMode::Batch {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
            StreamMode::Callback => KotlinStreamMode::Callback {
                class_name: class_name.to_string(),
                method_name_pascal: method_name_pascal.clone(),
            },
        };
        KotlinStream {
            name: NamingConvention::method_name(stream.stream_id.as_str()),
            mode,
            item_type: self.kotlin_type(&stream_def.item_type),
            pop_batch_items_expr: self.stream_pop_batch_items_expr(stream),
            subscribe: stream.subscribe.to_string(),
            poll: stream.poll.to_string(),
            pop_batch: stream.pop_batch.to_string(),
            wait: stream.wait.to_string(),
            unsubscribe: stream.unsubscribe.to_string(),
            free: stream.free.to_string(),
        }
    }

    fn stream_pop_batch_items_expr(&self, stream: &AbiStream) -> String {
        match &stream.item_transport {
            Transport::Scalar(origin) => Self::direct_scalar_stream_items_expr(origin),
            Transport::Composite(layout) => {
                let reader_name = format!(
                    "{}Reader",
                    NamingConvention::class_name(layout.record_id.as_str())
                );
                format!(
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).let {{ buffer -> {}.readAll(buffer, 0, bytes.size / {}.STRUCT_SIZE) }}",
                    reader_name, reader_name
                )
            }
            _ => {
                let StreamItemTransport::WireEncoded { decode_ops } = &stream.item;
                let item_decode =
                    emit::emit_reader_read(&self.rebase_read_seq(decode_ops, "pos", "0"));
                format!(
                    "run {{ val reader = WireReader(bytes); val count = reader.readI32(); List(count) {{ {} }} }}",
                    item_decode
                )
            }
        }
    }

    fn direct_scalar_stream_items_expr(origin: &ScalarOrigin) -> String {
        match origin {
            ScalarOrigin::Primitive(primitive) => match primitive {
                PrimitiveType::Bool => {
                    "List(bytes.size) { index -> bytes[index].toInt() != 0 }".to_string()
                }
                PrimitiveType::I8 => "List(bytes.size) { index -> bytes[index] }".to_string(),
                PrimitiveType::U8 => {
                    "List(bytes.size) { index -> bytes[index].toUByte() }".to_string()
                }
                PrimitiveType::I16 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asShortBuffer().let { buffer -> List(buffer.remaining()) { buffer.get() } }".to_string()
                }
                PrimitiveType::U16 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asShortBuffer().let { buffer -> List(buffer.remaining()) { buffer.get().toUShort() } }".to_string()
                }
                PrimitiveType::I32 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asIntBuffer().let { buffer -> List(buffer.remaining()) { buffer.get() } }".to_string()
                }
                PrimitiveType::U32 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asIntBuffer().let { buffer -> List(buffer.remaining()) { buffer.get().toUInt() } }".to_string()
                }
                PrimitiveType::I64 | PrimitiveType::ISize => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asLongBuffer().let { buffer -> List(buffer.remaining()) { buffer.get() } }".to_string()
                }
                PrimitiveType::U64 | PrimitiveType::USize => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asLongBuffer().let { buffer -> List(buffer.remaining()) { buffer.get().toULong() } }".to_string()
                }
                PrimitiveType::F32 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asFloatBuffer().let { buffer -> List(buffer.remaining()) { buffer.get() } }".to_string()
                }
                PrimitiveType::F64 => {
                    "ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asDoubleBuffer().let { buffer -> List(buffer.remaining()) { buffer.get() } }".to_string()
                }
            },
            ScalarOrigin::CStyleEnum { enum_id, tag_type } => {
                let enum_name = NamingConvention::class_name(enum_id.as_str());
                let values_expr =
                    Self::direct_scalar_stream_items_expr(&ScalarOrigin::Primitive(*tag_type));
                format!("{}.map {{ value -> {}.fromValue(value) }}", values_expr, enum_name)
            }
        }
    }

    fn abi_stream<'b>(&'b self, class: &ClassDef, stream: &StreamDef) -> &'b AbiStream {
        self.abi
            .streams
            .iter()
            .find(|item| item.class_id == class.id && item.stream_id == stream.id)
            .expect("abi stream")
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
            ReadOp::Bytes { offset } => ReadOp::Bytes {
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

    fn lower_callback_trait(&self, callback: &CallbackTraitDef) -> KotlinCallbackTrait {
        let interface_name = NamingConvention::class_name(callback.id.as_str());
        let handle_map_name = format!("{}HandleMap", interface_name);
        let callbacks_object = format!("{}Callbacks", interface_name);
        let bridge_name = format!("{}Bridge", interface_name);
        let proxy_class_name = format!("{}Proxy", interface_name);
        let supports_proxy_wrap = callback
            .methods
            .iter()
            .all(|method| method.execution_kind() == ExecutionKind::Sync);
        let sync_methods = callback
            .methods
            .iter()
            .filter(|method| method.execution_kind() == ExecutionKind::Sync)
            .map(|method| self.lower_callback_method(callback, method))
            .collect();
        let async_methods = callback
            .methods
            .iter()
            .filter(|method| method.execution_kind() == ExecutionKind::Async)
            .map(|method| self.lower_async_callback_method(callback, method))
            .collect();
        let proxy_methods = if supports_proxy_wrap {
            callback
                .methods
                .iter()
                .filter(|method| method.execution_kind() == ExecutionKind::Sync)
                .map(|method| self.render_callback_proxy_method(callback, method))
                .collect()
        } else {
            Vec::new()
        };
        let proxy_native_methods = if supports_proxy_wrap {
            callback
                .methods
                .iter()
                .filter(|method| method.execution_kind() == ExecutionKind::Sync)
                .map(|method| self.lower_callback_proxy_native_method(callback, method))
                .collect()
        } else {
            Vec::new()
        };
        KotlinCallbackTrait {
            interface_name,
            handle_map_name,
            callbacks_object,
            bridge_name,
            proxy_class_name,
            supports_proxy_wrap,
            proxy_release_name: self.callback_proxy_release_name(callback),
            proxy_methods,
            proxy_native_methods,
            doc: callback.doc.clone(),
            is_closure: matches!(callback.kind, CallbackKind::Closure),
            sync_methods,
            async_methods,
        }
    }

    fn lower_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let output_route = &abi_method.returns;
        let abi_param_map: HashMap<_, _> = abi_method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect();
        let params = method
            .params
            .iter()
            .filter_map(|def| {
                let abi_param = abi_param_map.get(&def.name)?;
                Some(self.lower_callback_param(def, abi_param))
            })
            .collect();
        let return_info =
            self.callback_return_info(&method.returns, output_route, &abi_method.error);
        KotlinCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            params,
            return_info,
            doc: method.doc.clone(),
        }
    }

    fn lower_async_callback_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinAsyncCallbackMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let output_route = &abi_method.returns;
        let abi_param_map: HashMap<_, _> = abi_method
            .params
            .iter()
            .map(|param| (param.name.clone(), param))
            .collect();
        let params = method
            .params
            .iter()
            .filter_map(|def| {
                let abi_param = abi_param_map.get(&def.name)?;
                Some(self.lower_callback_param(def, abi_param))
            })
            .collect();
        let return_info =
            self.callback_return_info(&method.returns, output_route, &abi_method.error);
        let invoker = self.async_callback_invoker(&return_info, output_route);
        let method_name_pascal = NamingConvention::class_name(method.id.as_str());
        KotlinAsyncCallbackMethod {
            name: NamingConvention::method_name(method.id.as_str()),
            ffi_name: abi_method.vtable_field.as_str().to_string(),
            complete_name: format!("complete{}", method_name_pascal),
            fail_name: format!("fail{}", method_name_pascal),
            invoker_name: invoker.name,
            params,
            return_info,
            doc: method.doc.clone(),
        }
    }

    fn render_callback_proxy_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> String {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let proxy_call = self.callback_proxy_call(callback, method, abi_method);
        let wire_writers = self.wire_writers_for_params(&proxy_call);
        let wire_writer_closes = wire_writers
            .iter()
            .filter_map(KotlinWireWriter::cleanup_code)
            .collect::<Vec<_>>();
        let native_args = self.native_args_for_params(&proxy_call, &method.params, &wire_writers);
        let signature_params = method
            .params
            .iter()
            .map(|param| KotlinSignatureParam {
                name: NamingConvention::param_name(param.name.as_str()),
                kotlin_type: self.kotlin_type(&param.type_expr),
            })
            .collect::<Vec<_>>();
        let return_type = self.kotlin_return_type_from_def(&method.returns, &abi_method.returns);
        let return_meta = self.kotlin_return_meta(&abi_method.returns);
        let decode_expr = self.decode_expr_for_call_return(&abi_method.returns, &method.returns);
        let err_type = self.error_type_name(&method.returns);

        WireMethodTemplate {
            method_name: &NamingConvention::method_name(method.id.as_str()),
            signature_params: &signature_params,
            return_type: return_type.as_deref(),
            wire_writers: &wire_writers,
            wire_writer_closes: &wire_writer_closes,
            native_args: &native_args,
            throws: self.is_throwing_return(&method.returns),
            err_type: &err_type,
            ffi_name: &self.callback_proxy_native_name(callback, method),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            direct_return_is_nullable: return_meta.direct_is_nullable,
            return_cast: &return_meta.cast,
            decode_expr: &decode_expr,
            is_blittable_return: self.is_blittable_return(&abi_method.returns, &method.returns),
            include_handle: true,
            override_method: true,
            doc: &method.doc,
        }
        .render()
        .unwrap()
    }

    fn lower_callback_proxy_native_method(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
    ) -> KotlinNativeSyncMethod {
        let abi_method = self.abi_callback_method(&callback.id, &method.id);
        let proxy_call = self.callback_proxy_call(callback, method, abi_method);
        KotlinNativeSyncMethod {
            ffi_name: self.callback_proxy_native_name(callback, method),
            include_handle: true,
            params: self
                .visible_native_params(&proxy_call)
                .into_iter()
                .map(|param| KotlinNativeParam {
                    name: param.name.as_str().to_string(),
                    jni_type: self.jni_type_for_param(param),
                })
                .collect(),
            return_jni_type: self.return_jni_type_for_callback_shape(&abi_method.returns),
        }
    }

    fn callback_proxy_call(
        &self,
        callback: &CallbackTraitDef,
        method: &CallbackMethodDef,
        abi_method: &AbiCallbackMethod,
    ) -> AbiCall {
        let method_param_names = method
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect::<HashSet<_>>();
        AbiCall {
            id: CallId::Function(crate::ir::ids::FunctionId::new(format!(
                "__callback_proxy_{}_{}",
                callback.id.as_str(),
                method.id.as_str()
            ))),
            symbol: naming::Name::<naming::GlobalSymbol>::new(
                self.callback_proxy_native_name(callback, method),
            ),
            mode: CallMode::Sync,
            params: abi_method
                .params
                .iter()
                .filter(|param| match &param.role {
                    ParamRole::Input { .. } => method_param_names.contains(&param.name),
                    ParamRole::SyntheticLen { for_param } => method_param_names.contains(for_param),
                    ParamRole::CallbackContext { .. }
                    | ParamRole::OutLen { .. }
                    | ParamRole::OutDirect
                    | ParamRole::StatusOut => false,
                })
                .cloned()
                .collect(),
            returns: abi_method.returns.clone(),
            error: abi_method.error.clone(),
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

    fn callback_proxy_release_name(&self, callback: &CallbackTraitDef) -> String {
        format!(
            "boltffiCallback{}Release",
            NamingConvention::class_name(callback.id.as_str())
        )
    }

    fn return_jni_type_for_callback_shape(&self, ret_shape: &ReturnShape) -> String {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => "Unit".to_string(),
            ValueReturnStrategy::Scalar(_) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar callback return requires scalar transport");
                };
                self.jni_type_for_abi(&AbiType::from(origin.primitive()))
            }
            ValueReturnStrategy::ObjectHandle | ValueReturnStrategy::CallbackHandle => {
                "Long".to_string()
            }
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                "ByteArray".to_string()
            }
        }
    }

    fn lower_callback_param(&self, def: &ParamDef, param: &AbiParam) -> KotlinCallbackParam {
        let name = NamingConvention::param_name(param.name.as_str());
        let kotlin_type = self.kotlin_type(&def.type_expr);
        match &param.role {
            ParamRole::Input {
                transport: Transport::Scalar(_),
                ..
            } => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: self.jni_type_for_abi(&param.abi_type),
                conversion: self.callback_direct_conversion(def, param, &name),
            },
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Encoded(_)),
                decode_ops: Some(decode_ops),
                ..
            } => KotlinCallbackParam {
                name: name.clone(),
                kotlin_type,
                jni_type: "ByteBuffer".to_string(),
                conversion: self.callback_encoded_conversion(decode_ops, &name),
            },
            _ => unreachable!("unsupported callback param role: {:?}", param.role),
        }
    }

    fn async_callback_invoker(
        &self,
        return_info: &Option<KotlinCallbackReturn>,
        ret_shape: &ReturnShape,
    ) -> KotlinAsyncCallbackInvoker {
        let mut result_jni_type = return_info.as_ref().map(|ret| ret.jni_type.clone());
        let suffix = self.invoker_suffix_from_return_shape(ret_shape);
        if suffix == "Void" {
            result_jni_type = None;
        }
        KotlinAsyncCallbackInvoker {
            name: format!("invokeAsyncCallback{}", suffix),
            result_jni_type,
        }
    }

    fn invoker_suffix_from_return_shape(&self, ret_shape: &ReturnShape) -> String {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => "Void".to_string(),
            ValueReturnStrategy::Scalar(_) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar return strategy requires scalar transport");
                };
                self.invoker_suffix_from_primitive(origin.primitive())
            }
            ValueReturnStrategy::ObjectHandle | ValueReturnStrategy::CallbackHandle => {
                "Handle".to_string()
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

    fn callback_return_info(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
        error: &ErrorTransport,
    ) -> Option<KotlinCallbackReturn> {
        let kotlin_type = self.kotlin_return_type_from_def(returns, ret_shape)?;
        let return_type = self.callback_return_type(returns);
        let (jni_type, default_value, to_jni) = match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => return None,
            ValueReturnStrategy::Scalar(_) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar return strategy requires scalar transport");
                };
                let abi_type = AbiType::from(origin.primitive());
                (
                    self.jni_type_for_abi(&abi_type),
                    self.callback_default_value_for_abi(&abi_type),
                    self.callback_return_cast(return_type, &abi_type),
                )
            }
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                let to_jni = ret_shape
                    .encode_ops
                    .as_ref()
                    .map(|encode_ops| self.callback_return_wire_encode(encode_ops))
                    .or_else(|| self.callback_direct_span_wire_encode(returns, ret_shape))
                    .or_else(|| {
                        let return_type = self.callback_return_type(returns)?;
                        let encode_ops = self.find_write_seq_for_type(return_type)?;
                        Some(self.callback_return_wire_encode(&encode_ops))
                    })
                    .unwrap_or_default();
                ("ByteArray".to_string(), "byteArrayOf()".to_string(), to_jni)
            }
            ValueReturnStrategy::ObjectHandle => {
                let Some(Transport::Handle { class_id, nullable }) = &ret_shape.transport else {
                    unreachable!("object handle return strategy requires handle transport");
                };
                (
                    "Long".to_string(),
                    "0L".to_string(),
                    self.callback_return_handle_cast(class_id, *nullable),
                )
            }
            ValueReturnStrategy::CallbackHandle => {
                let Some(Transport::Callback {
                    callback_id,
                    nullable,
                    ..
                }) = &ret_shape.transport
                else {
                    unreachable!("callback handle return strategy requires callback transport");
                };
                (
                    "Long".to_string(),
                    "0L".to_string(),
                    self.callback_return_callback_cast(callback_id, *nullable),
                )
            }
        };
        let (error_type, error_is_throwable) = match returns {
            ReturnDef::Result { err, .. } => {
                let err_type = self.kotlin_type(err);
                let throwable = match err {
                    TypeExpr::Enum(id) => self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .map(|e| e.is_error)
                        .unwrap_or(false),
                    TypeExpr::Record(id) => self
                        .contract
                        .catalog
                        .resolve_record(id)
                        .map(|record| record.is_error)
                        .unwrap_or(false),
                    _ => false,
                };
                (Some(err_type), throwable)
            }
            _ => (None, false),
        };
        let to_jni_result = self.build_result_wire_encode(returns, ret_shape, error);
        Some(KotlinCallbackReturn {
            kotlin_type,
            jni_type,
            default_value,
            to_jni,
            to_jni_result,
            error_type,
            error_is_throwable,
        })
    }

    fn callback_return_type<'b>(&self, returns: &'b ReturnDef) -> Option<&'b TypeExpr> {
        match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(ty),
            ReturnDef::Result { ok, .. } => Some(ok),
        }
    }

    fn callback_direct_span_wire_encode(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
    ) -> Option<String> {
        match (returns, &ret_shape.transport) {
            (
                ReturnDef::Value(TypeExpr::Bytes),
                Some(Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                    PrimitiveType::U8,
                )))),
            ) => Some(
                ".let { value -> run { val writer = WireWriterPool.acquire(4 + value.size); try { val wire = writer.writer; wire.writeBytes(value); wire.toByteArray() } finally { writer.close() } } }".to_string(),
            ),
            (
                ReturnDef::Value(TypeExpr::Vec(inner)),
                Some(Transport::Span(SpanContent::Scalar(origin))),
            ) => self.callback_direct_scalar_vec_wire_encode(inner.as_ref(), origin),
            (
                ReturnDef::Value(TypeExpr::Vec(_)),
                Some(Transport::Span(SpanContent::Composite(layout))),
            ) => {
                let writer_name = format!("{}Writer", layout.record_id.as_str());
                Some(format!(
                    ".let {{ value -> run {{ val writer = WireWriterPool.acquire(4 + value.size * {}.STRUCT_SIZE); try {{ val wire = writer.writer; wire.writeU32(value.size.toUInt()); {}.writeAllToWire(wire, value); wire.toByteArray() }} finally {{ writer.close() }} }} }}",
                    writer_name, writer_name
                ))
            }
            _ => None,
        }
    }

    fn callback_direct_scalar_vec_wire_encode(
        &self,
        element_type: &TypeExpr,
        origin: &ScalarOrigin,
    ) -> Option<String> {
        match (element_type, origin) {
            (TypeExpr::Primitive(primitive), ScalarOrigin::Primitive(origin_primitive))
                if primitive == origin_primitive =>
            {
                let element_size = primitive.wire_size_bytes();
                Some(format!(
                    ".let {{ value -> run {{ val writer = WireWriterPool.acquire(4 + value.size * {element_size}); try {{ val wire = writer.writer; wire.writePrimitiveList(value); wire.toByteArray() }} finally {{ writer.close() }} }} }}"
                ))
            }
            (TypeExpr::Enum(_), ScalarOrigin::CStyleEnum { tag_type, .. }) => {
                let element_size = tag_type.wire_size_bytes();
                let write_method = self.kotlin_wire_write_method_for_primitive(*tag_type);
                let element_expr = self.kotlin_integral_cast_expr(*tag_type, "item.value");
                Some(format!(
                    ".let {{ value -> run {{ val writer = WireWriterPool.acquire(4 + value.size * {element_size}); try {{ val wire = writer.writer; wire.writeU32(value.size.toUInt()); value.forEach {{ item -> wire.{write_method}({element_expr}) }}; wire.toByteArray() }} finally {{ writer.close() }} }} }}"
                ))
            }
            _ => None,
        }
    }

    fn callback_direct_conversion(&self, def: &ParamDef, param: &AbiParam, name: &str) -> String {
        match (&def.type_expr, &param.role) {
            (
                TypeExpr::Enum(enum_id),
                ParamRole::Input {
                    transport: Transport::Scalar(ScalarOrigin::CStyleEnum { .. }),
                    ..
                },
            ) => {
                let enum_name = NamingConvention::class_name(enum_id.as_str());
                format!("{}.fromValue({})", enum_name, name)
            }
            (ty, _) => match ty {
                TypeExpr::Primitive(p) => match p {
                    PrimitiveType::U8 => format!("{}.toUByte()", name),
                    PrimitiveType::U16 => format!("{}.toUShort()", name),
                    PrimitiveType::U32 => format!("{}.toUInt()", name),
                    PrimitiveType::U64 | PrimitiveType::USize => format!("{}.toULong()", name),
                    _ => name.to_string(),
                },
                _ => name.to_string(),
            },
        }
    }

    fn callback_encoded_conversion(&self, decode_ops: &ReadSeq, name: &str) -> String {
        let decode_expr = emit::emit_reader_read(decode_ops);
        format!(
            "run {{ val reader = WireReader({}); {} }}",
            name, decode_expr
        )
    }

    fn callback_return_cast(&self, ty: Option<&TypeExpr>, abi: &AbiType) -> String {
        match ty {
            Some(TypeExpr::Enum(enum_id)) => self
                .contract
                .catalog
                .resolve_enum(enum_id)
                .and_then(|enumeration| match enumeration.repr {
                    EnumRepr::CStyle { .. } => Some(".value".to_string()),
                    EnumRepr::Data { .. } => None,
                })
                .unwrap_or_else(|| self.callback_return_cast_for_abi(abi)),
            _ => self.callback_return_cast_for_abi(abi),
        }
    }

    fn callback_return_cast_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toByte()".to_string(),
            AbiType::U16 => ".toShort()".to_string(),
            AbiType::U32 => ".toInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toLong()".to_string(),
            _ => String::new(),
        }
    }

    fn callback_default_value_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "false".to_string(),
            AbiType::I8 | AbiType::U8 => "0".to_string(),
            AbiType::I16 | AbiType::U16 => "0".to_string(),
            AbiType::I32 | AbiType::U32 => "0".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => "0L".to_string(),
            AbiType::F32 => "0f".to_string(),
            AbiType::F64 => "0.0".to_string(),
            _ => "0".to_string(),
        }
    }

    fn callback_return_handle_cast(&self, _class_id: &ClassId, nullable: bool) -> String {
        if nullable {
            "?.handle ?: 0L".to_string()
        } else {
            ".handle".to_string()
        }
    }

    fn callback_return_callback_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!("?.let {{ {}.create(it) }} ?: 0L", bridge)
        } else {
            format!(".let {{ {}.create(it) }}", bridge)
        }
    }

    fn callback_return_wire_encode(&self, encode_ops: &WriteSeq) -> String {
        let size_expr = emit::emit_size_expr_for_write_seq(encode_ops);
        let encode_expr = emit::emit_write_expr(encode_ops);
        format!(
            ".let {{ value -> run {{ val writer = WireWriterPool.acquire({}); try {{ val wire = writer.writer; {}; wire.toByteArray() }} finally {{ writer.close() }} }} }}",
            size_expr, encode_expr
        )
    }

    fn throws_success_encode_ops(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
    ) -> Option<WriteSeq> {
        match returns {
            ReturnDef::Result { .. } => {
                ret_shape
                    .encode_ops
                    .as_ref()
                    .and_then(|encode_ops| match encode_ops.ops.first() {
                        Some(WriteOp::Result { ok, .. }) => {
                            Some(remap_root_in_seq(ok, ValueExpr::Var("okVal".to_string())))
                        }
                        _ => None,
                    })
            }
            _ => None,
        }
    }

    fn build_result_wire_encode(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
        error: &ErrorTransport,
    ) -> Option<String> {
        let ok_seq = self
            .throws_success_encode_ops(returns, ret_shape)
            .or_else(|| {
                ret_shape.encode_ops.as_ref().map(|ok_encode_ops| {
                    remap_root_in_seq(ok_encode_ops, ValueExpr::Var("okVal".to_string()))
                })
            })?;
        let err_encode_ops = match error {
            ErrorTransport::Encoded {
                encode_ops: Some(err_encode_ops),
                ..
            } => err_encode_ops,
            _ => return None,
        };

        let err_seq = remap_root_in_seq(err_encode_ops, ValueExpr::Var("errVal".to_string()));
        let ok_size = Self::size_expr_for_write_seq(&ok_seq);
        let err_size = Self::size_expr_for_write_seq(&err_seq);
        let value = ValueExpr::Var("value".to_string());

        let result_seq = WriteSeq {
            size: SizeExpr::ResultSize {
                value: value.clone(),
                ok: Box::new(ok_size),
                err: Box::new(err_size),
            },
            ops: vec![WriteOp::Result {
                value: value.clone(),
                ok: Box::new(ok_seq),
                err: Box::new(err_seq),
            }],
            shape: WireShape::Value,
        };

        Some(self.callback_return_wire_encode(&result_seq))
    }

    fn size_expr_for_write_seq(seq: &WriteSeq) -> SizeExpr {
        match seq.ops.first() {
            Some(WriteOp::Custom { value, .. }) => SizeExpr::WireSize {
                value: value.clone(),
                owner: None,
            },
            _ => seq.size.clone(),
        }
    }

    fn lower_native(&self) -> KotlinNative {
        let functions = self
            .contract
            .functions
            .iter()
            .map(|func| self.lower_native_function(func))
            .collect::<Vec<_>>();
        let class_symbols =
            self.contract
                .catalog
                .all_classes()
                .flat_map(|class| {
                    let ctor_symbols = class.constructors.iter().enumerate().map(|(index, _)| {
                        self.abi_call_for_constructor(class, index).symbol.clone()
                    });
                    let method_symbols = class
                        .methods
                        .iter()
                        .map(|method| self.abi_call_for_method(class, method).symbol.clone());
                    ctor_symbols.chain(method_symbols)
                })
                .collect::<HashSet<_>>();
        let declared_symbols = functions
            .iter()
            .map(|f| f.ffi_name.as_str())
            .chain(class_symbols.iter().map(|s| s.as_str()))
            .collect::<HashSet<_>>();
        let wire_functions = self
            .abi
            .calls
            .iter()
            .filter(|call| !declared_symbols.contains(call.symbol.as_str()))
            .filter(|call| matches!(call.mode, CallMode::Sync))
            .map(|call| KotlinNativeWireFunction {
                ffi_name: call.symbol.as_str().to_string(),
                params: self
                    .visible_native_params(call)
                    .into_iter()
                    .map(|param| KotlinNativeParam {
                        name: param.name.as_str().to_string(),
                        jni_type: self.jni_type_for_param(param),
                    })
                    .collect(),
                return_jni_type: self.jni_type_for_return_shape(&call.returns),
            })
            .collect::<Vec<_>>();
        let classes = self
            .contract
            .catalog
            .all_classes()
            .map(|class| self.lower_native_class(class))
            .collect::<Vec<_>>();
        let async_callback_invokers = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| {
                callback
                    .methods
                    .iter()
                    .filter(|method| method.execution_kind() == ExecutionKind::Async)
                    .map(|method| {
                        let abi_method = self.abi_callback_method(&callback.id, &method.id);
                        let return_info = self.callback_return_info(
                            &method.returns,
                            &abi_method.returns,
                            &abi_method.error,
                        );
                        self.async_callback_invoker(&return_info, &abi_method.returns)
                    })
            })
            .fold(
                (HashSet::new(), Vec::new()),
                |(mut seen, mut invokers), invoker| {
                    if seen.insert(invoker.name.clone()) {
                        invokers.push(invoker);
                    }
                    (seen, invokers)
                },
            )
            .1;
        KotlinNative {
            lib_name: self
                .options
                .library_name
                .clone()
                .unwrap_or_else(|| naming::library_name(&self.contract.package.name)),
            desktop_loader: self.options.desktop_loader,
            prefix: naming::ffi_prefix().to_string(),
            functions,
            wire_functions,
            classes,
            async_callback_invokers,
        }
    }

    fn lower_native_function(&self, func: &FunctionDef) -> KotlinNativeFunction {
        let call = self.abi_call_for_function(func);
        let return_jni_type = self.jni_type_for_return_shape(&call.returns);
        let complete_return_jni_type = match &call.mode {
            CallMode::Async(async_call) => {
                self.jni_type_for_async_complete_shape(&async_call.result)
            }
            CallMode::Sync => String::new(),
        };
        let async_ffi = match &call.mode {
            CallMode::Async(async_call) => Some(KotlinNativeAsyncFfi {
                ffi_poll: async_call.poll.as_str().to_string(),
                ffi_complete: async_call.complete.as_str().to_string(),
                ffi_cancel: async_call.cancel.as_str().to_string(),
                ffi_free: async_call.free.as_str().to_string(),
                complete_return_jni_type,
            }),
            CallMode::Sync => None,
        };
        KotlinNativeFunction {
            ffi_name: call.symbol.as_str().to_string(),
            params: self
                .visible_native_params(call)
                .into_iter()
                .map(|param| KotlinNativeParam {
                    name: param.name.as_str().to_string(),
                    jni_type: self.jni_type_for_param(param),
                })
                .collect(),
            return_jni_type,
            async_ffi,
        }
    }

    fn lower_native_class(&self, class: &ClassDef) -> KotlinNativeClass {
        let ctors = class
            .constructors
            .iter()
            .enumerate()
            .map(|(index, _ctor)| {
                let call = self.abi_call_for_constructor(class, index);
                KotlinNativeCtor {
                    ffi_name: call.symbol.as_str().to_string(),
                    params: self
                        .visible_native_params(call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                }
            })
            .collect();
        let async_methods = class
            .methods
            .iter()
            .filter(|method| method.is_async())
            .map(|method| {
                let call = Self::strip_receiver(self.abi_call_for_method(class, method));
                let async_call = match &call.mode {
                    CallMode::Async(async_call) => async_call,
                    CallMode::Sync => unreachable!("async method missing async call"),
                };
                KotlinNativeAsyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    ffi_poll: async_call.poll.as_str().to_string(),
                    ffi_complete: async_call.complete.as_str().to_string(),
                    ffi_cancel: async_call.cancel.as_str().to_string(),
                    ffi_free: async_call.free.as_str().to_string(),
                    include_handle: method.receiver != Receiver::Static,
                    params: self
                        .visible_native_params(&call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                    return_jni_type: self.jni_type_for_async_complete_shape(&async_call.result),
                }
            })
            .collect();
        let sync_methods = class
            .methods
            .iter()
            .filter(|method| !method.is_async())
            .map(|method| {
                let call = Self::strip_receiver(self.abi_call_for_method(class, method));
                KotlinNativeSyncMethod {
                    ffi_name: call.symbol.as_str().to_string(),
                    include_handle: method.receiver != Receiver::Static,
                    params: self
                        .visible_native_params(&call)
                        .into_iter()
                        .map(|param| KotlinNativeParam {
                            name: param.name.as_str().to_string(),
                            jni_type: self.jni_type_for_param(param),
                        })
                        .collect(),
                    return_jni_type: self.jni_type_for_return(&call),
                }
            })
            .collect();
        let streams = class
            .streams
            .iter()
            .map(|stream| {
                let abi_stream = self.abi_stream(class, stream);
                KotlinNativeStream {
                    subscribe: abi_stream.subscribe.as_str().to_string(),
                    poll: abi_stream.poll.as_str().to_string(),
                    pop_batch: abi_stream.pop_batch.as_str().to_string(),
                    wait: abi_stream.wait.as_str().to_string(),
                    unsubscribe: abi_stream.unsubscribe.as_str().to_string(),
                    free: abi_stream.free.as_str().to_string(),
                }
            })
            .collect();
        KotlinNativeClass {
            ffi_free: naming::class_ffi_free(class.id.as_str()).into_string(),
            ctors,
            async_methods,
            sync_methods,
            streams,
        }
    }

    fn jni_type_for_return(&self, call: &AbiCall) -> String {
        self.jni_type_for_return_shape(&call.returns)
    }

    fn jni_type_for_return_shape(&self, ret_shape: &ReturnShape) -> String {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => "Unit".to_string(),
            ValueReturnStrategy::Scalar(_) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar return strategy requires scalar transport");
                };
                self.jni_type_for_abi(&AbiType::from(origin.primitive()))
            }
            ValueReturnStrategy::ObjectHandle | ValueReturnStrategy::CallbackHandle => {
                "Long".to_string()
            }
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String) => "String?".to_string(),
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                "ByteArray?".to_string()
            }
        }
    }

    fn jni_type_for_async_complete_shape(&self, ret_shape: &ReturnShape) -> String {
        if Self::is_utf8_string_return(ret_shape) {
            return "ByteArray?".to_string();
        }
        self.jni_type_for_return_shape(ret_shape)
    }

    fn jni_type_for_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "Byte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "Short".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "Int".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "Long".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "Long".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer(_)
            | AbiType::OwnedBuffer
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_)
            | AbiType::CallbackHandle => "Long".to_string(),
            AbiType::Struct(_) => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn jni_param_mapping(&self, param: &AbiParam, type_expr: Option<&TypeExpr>) -> JniParamMapping {
        match &param.role {
            ParamRole::Input {
                transport: Transport::Scalar(_),
                len_param: None,
                ..
            } => {
                let c_style_enum_id = type_expr.and_then(|ty| match ty {
                    TypeExpr::Enum(id) => self
                        .contract
                        .catalog
                        .resolve_enum(id)
                        .filter(|e| !matches!(e.repr, EnumRepr::Data { .. }))
                        .map(|_| id.clone()),
                    _ => None,
                });
                JniParamMapping {
                    role: JniParamRole::Direct {
                        jni_type: self.jni_type_for_abi(&param.abi_type),
                        c_style_enum_id,
                    },
                    len_companion: None,
                }
            }
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Utf8),
                len_param,
                ..
            } => JniParamMapping {
                role: JniParamRole::StringParam,
                len_companion: len_param.clone(),
            },
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Scalar(origin)),
                len_param,
                ..
            } => JniParamMapping {
                role: JniParamRole::Buffer {
                    jni_type: self.jni_buffer_type(&AbiType::from(origin.primitive())),
                },
                len_companion: len_param.clone(),
            },
            ParamRole::Input {
                transport: Transport::Span(SpanContent::Encoded(_)),
                len_param,
                ..
            }
            | ParamRole::Input {
                transport: Transport::Composite(_),
                len_param,
                ..
            }
            | ParamRole::Input {
                transport: Transport::Span(SpanContent::Composite(_)),
                len_param,
                ..
            } => JniParamMapping {
                role: JniParamRole::Encoded,
                len_companion: len_param.clone(),
            },
            ParamRole::Input {
                transport: Transport::Handle { nullable, .. },
                ..
            } => JniParamMapping {
                role: JniParamRole::Handle {
                    nullable: *nullable,
                },
                len_companion: None,
            },
            ParamRole::Input {
                transport:
                    Transport::Callback {
                        callback_id,
                        nullable,
                        ..
                    },
                ..
            } => JniParamMapping {
                role: JniParamRole::Callback {
                    callback_id: callback_id.clone(),
                    nullable: *nullable,
                },
                len_companion: None,
            },
            ParamRole::Input {
                transport: Transport::Scalar(_),
                len_param: Some(len_param),
                ..
            } => JniParamMapping {
                role: JniParamRole::Direct {
                    jni_type: self.jni_type_for_abi(&param.abi_type),
                    c_style_enum_id: None,
                },
                len_companion: Some(len_param.clone()),
            },
            ParamRole::SyntheticLen { .. }
            | ParamRole::CallbackContext { .. }
            | ParamRole::OutLen { .. }
            | ParamRole::OutDirect
            | ParamRole::StatusOut => JniParamMapping {
                role: JniParamRole::Hidden,
                len_companion: None,
            },
        }
    }

    fn jni_type_for_param(&self, param: &AbiParam) -> String {
        self.jni_param_mapping(param, None).jni_type()
    }

    fn jni_buffer_type(&self, element_abi: &AbiType) -> String {
        match element_abi {
            AbiType::I32 | AbiType::U32 => "IntArray".to_string(),
            AbiType::I16 | AbiType::U16 => "ShortArray".to_string(),
            AbiType::I64 | AbiType::U64 | AbiType::ISize | AbiType::USize => {
                "LongArray".to_string()
            }
            AbiType::F32 => "FloatArray".to_string(),
            AbiType::F64 => "DoubleArray".to_string(),
            AbiType::U8 | AbiType::I8 => "ByteArray".to_string(),
            AbiType::Bool => "BooleanArray".to_string(),
            _ => "ByteBuffer".to_string(),
        }
    }

    fn kotlin_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Primitive(p) => self.primitive_kotlin_type(*p),
            TypeExpr::String => "String".to_string(),
            TypeExpr::Bytes => "ByteArray".to_string(),
            TypeExpr::Builtin(id) => self.builtin_kotlin_type(id),
            TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Custom(id) => {
                if let Some(mapping) = self.type_mappings.get(id.as_str()) {
                    mapping.native_type.clone()
                } else {
                    NamingConvention::class_name(id.as_str())
                }
            }
            TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
            TypeExpr::Vec(inner) => self.kotlin_vec_type(inner),
            TypeExpr::Option(inner) => format!("{}?", self.kotlin_type(inner)),
            TypeExpr::Result { ok, err } => {
                format!(
                    "BoltFFIResult<{}, {}>",
                    self.kotlin_type(ok),
                    self.kotlin_type(err)
                )
            }
            TypeExpr::Handle(class_id) => NamingConvention::class_name(class_id.as_str()),
            TypeExpr::Callback(callback_id) => NamingConvention::class_name(callback_id.as_str()),
            TypeExpr::Void => "Unit".to_string(),
        }
    }

    fn kotlin_type_with_disambiguation(
        &self,
        ty: &TypeExpr,
        reserved: &HashSet<String>,
    ) -> (String, Option<String>) {
        match ty {
            TypeExpr::Record(id) => self.disambiguate_type_name(id.as_str(), reserved),
            TypeExpr::Custom(id) => {
                if self.type_mappings.contains_key(id.as_str()) {
                    (self.kotlin_type(ty), None)
                } else {
                    self.disambiguate_type_name(id.as_str(), reserved)
                }
            }
            TypeExpr::Enum(id) => self.disambiguate_type_name(id.as_str(), reserved),
            _ => (self.kotlin_type(ty), None),
        }
    }

    fn qualify_decode_expr(&self, expr: String, qualified: Option<&str>) -> String {
        let Some(qualified) = qualified else {
            return expr;
        };
        let unqualified = qualified.rsplit('.').next().unwrap_or(qualified);
        let prefix = format!("{}.", unqualified);
        expr.strip_prefix(&prefix)
            .map(|suffix| format!("{}.{}", qualified, suffix))
            .unwrap_or(expr)
    }

    /// Rewrites a Kotlin source fragment so any bare occurrence of a
    /// shadowed sibling name becomes fully qualified. Boundary
    /// detection matches identifier chars and `.` so existing FQNs
    /// like `com.boltffi.demo.Point` are left alone.
    fn qualify_collisions_in_string(&self, src: &mut String, sibling_names: &HashSet<String>) {
        for name in sibling_names {
            let qualified = format!("{}.{name}", self.package_name);
            let mut search_from = 0;
            while let Some(idx_in_rest) = src[search_from..].find(name.as_str()) {
                let idx = search_from + idx_in_rest;
                let end = idx + name.len();
                let left_ok = match src[..idx].chars().next_back() {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_' && c != '.',
                };
                let right_ok = match src[end..].chars().next() {
                    None => true,
                    Some(c) => !c.is_ascii_alphanumeric() && c != '_',
                };
                if left_ok && right_ok {
                    src.replace_range(idx..end, &qualified);
                    search_from = idx + qualified.len();
                } else {
                    search_from = end;
                }
            }
        }
    }

    fn disambiguate_type_name(
        &self,
        type_name: &str,
        reserved: &HashSet<String>,
    ) -> (String, Option<String>) {
        let class_name = NamingConvention::class_name(type_name);
        if reserved.contains(&class_name) {
            let qualified = format!("{}.{}", self.package_name, class_name);
            (qualified.clone(), Some(qualified))
        } else {
            (class_name, None)
        }
    }

    fn kotlin_vec_type(&self, inner: &TypeExpr) -> String {
        match inner {
            TypeExpr::Primitive(p) => match p {
                PrimitiveType::I32 | PrimitiveType::U32 => "IntArray".to_string(),
                PrimitiveType::I16 | PrimitiveType::U16 => "ShortArray".to_string(),
                PrimitiveType::I64
                | PrimitiveType::U64
                | PrimitiveType::ISize
                | PrimitiveType::USize => "LongArray".to_string(),
                PrimitiveType::F32 => "FloatArray".to_string(),
                PrimitiveType::F64 => "DoubleArray".to_string(),
                PrimitiveType::U8 | PrimitiveType::I8 => "ByteArray".to_string(),
                PrimitiveType::Bool => "BooleanArray".to_string(),
            },
            _ => format!("List<{}>", self.kotlin_type(inner)),
        }
    }

    fn primitive_kotlin_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U8 => "UByte".to_string(),
            PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U16 => "UShort".to_string(),
            PrimitiveType::I32 => "Int".to_string(),
            PrimitiveType::U32 => "UInt".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "Long".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "ULong".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn primitive_jni_type(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::I8 | PrimitiveType::U8 => "Byte".to_string(),
            PrimitiveType::I16 | PrimitiveType::U16 => "Short".to_string(),
            PrimitiveType::I32 | PrimitiveType::U32 => "Int".to_string(),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "Long".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        }
    }

    fn builtin_kotlin_type(&self, id: &BuiltinId) -> String {
        match id.as_str() {
            "Duration" => "Duration".to_string(),
            "SystemTime" => "Instant".to_string(),
            "Uuid" => "UUID".to_string(),
            "Url" => "URI".to_string(),
            _ => "String".to_string(),
        }
    }

    fn kotlin_return_type_from_def(
        &self,
        returns: &ReturnDef,
        ret_shape: &ReturnShape,
    ) -> Option<String> {
        let base = match returns {
            ReturnDef::Void => None,
            ReturnDef::Value(ty) => Some(self.kotlin_type(ty)),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => Some("Unit".to_string()),
                _ => Some(self.kotlin_type(ok)),
            },
        };
        match &ret_shape.transport {
            Some(Transport::Handle { nullable: true, .. })
            | Some(Transport::Callback { nullable: true, .. }) => base.map(|ty| {
                if ty.ends_with('?') {
                    ty
                } else {
                    format!("{}?", ty)
                }
            }),
            _ => base,
        }
    }

    fn kotlin_type_from_return_def(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Void => "Unit".to_string(),
            ReturnDef::Value(ty) => self.kotlin_type(ty),
            ReturnDef::Result { ok, .. } => match ok {
                TypeExpr::Void => "Unit".to_string(),
                _ => self.kotlin_type(ok),
            },
        }
    }

    fn closure_interface_name(&self, callback_id: &str) -> String {
        let signature = callback_id
            .strip_prefix("__Closure_")
            .unwrap_or(callback_id);
        format!("{}Callback", signature)
    }

    fn closure_param_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Record(_) => "java.nio.ByteBuffer".to_string(),
            _ => self.kotlin_type(ty),
        }
    }

    fn kotlin_type_from_abi(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::Bool => "Boolean".to_string(),
            AbiType::I8 => "Byte".to_string(),
            AbiType::U8 => "UByte".to_string(),
            AbiType::I16 => "Short".to_string(),
            AbiType::U16 => "UShort".to_string(),
            AbiType::I32 => "Int".to_string(),
            AbiType::U32 => "UInt".to_string(),
            AbiType::I64 => "Long".to_string(),
            AbiType::U64 => "ULong".to_string(),
            AbiType::ISize => "Long".to_string(),
            AbiType::USize => "ULong".to_string(),
            AbiType::F32 => "Float".to_string(),
            AbiType::F64 => "Double".to_string(),
            AbiType::Pointer(_)
            | AbiType::OwnedBuffer
            | AbiType::InlineCallbackFn { .. }
            | AbiType::Handle(_)
            | AbiType::CallbackHandle => "Long".to_string(),
            AbiType::Struct(_) => "Long".to_string(),
            AbiType::Void => "Unit".to_string(),
        }
    }

    fn return_type_from_decode_ops(&self, seq: &ReadSeq) -> String {
        let op = seq.ops.first().expect("decode op");
        match op {
            ReadOp::Primitive { primitive, .. } => self.primitive_kotlin_type(*primitive),
            ReadOp::String { .. } => "String".to_string(),
            ReadOp::Bytes { .. } => "ByteArray".to_string(),
            ReadOp::Builtin { id, .. } => self.builtin_kotlin_type(id),
            ReadOp::Record { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Enum { id, .. } => NamingConvention::class_name(id.as_str()),
            ReadOp::Vec { element_type, .. } => self.kotlin_vec_type(element_type),
            ReadOp::Option { some, .. } => format!("{}?", self.return_type_from_decode_ops(some)),
            ReadOp::Result { ok, .. } => self.return_type_from_decode_ops(ok),
            ReadOp::Custom { id, .. } => NamingConvention::class_name(id.as_str()),
        }
    }

    fn kotlin_return_meta(&self, ret_shape: &ReturnShape) -> KotlinReturnMeta {
        match ret_shape.value_return_strategy() {
            ValueReturnStrategy::Void => KotlinReturnMeta {
                is_unit: true,
                is_direct: false,
                direct_is_nullable: false,
                cast: String::new(),
            },
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue)
            | ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag) => {
                let Some(Transport::Scalar(origin)) = &ret_shape.transport else {
                    unreachable!("scalar return strategy requires scalar transport");
                };
                let cast = match origin {
                    ScalarOrigin::CStyleEnum { enum_id, .. } => {
                        let enum_name = NamingConvention::class_name(enum_id.as_str());
                        format!(".let {{ {}.fromValue(it) }}", enum_name)
                    }
                    _ => self.kotlin_return_cast(&AbiType::from(origin.primitive())),
                };
                KotlinReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    direct_is_nullable: false,
                    cast,
                }
            }
            ValueReturnStrategy::ObjectHandle => {
                let Some(Transport::Handle { class_id, nullable }) = &ret_shape.transport else {
                    unreachable!("object handle return strategy requires handle transport");
                };
                KotlinReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    direct_is_nullable: false,
                    cast: self.kotlin_handle_return_cast(class_id, *nullable),
                }
            }
            ValueReturnStrategy::CallbackHandle => {
                let Some(Transport::Callback {
                    callback_id,
                    nullable,
                    ..
                }) = &ret_shape.transport
                else {
                    unreachable!("callback handle return strategy requires callback transport");
                };
                KotlinReturnMeta {
                    is_unit: false,
                    is_direct: true,
                    direct_is_nullable: false,
                    cast: self.kotlin_callback_return_cast(callback_id, *nullable),
                }
            }
            ValueReturnStrategy::CompositeValue | ValueReturnStrategy::Buffer(_) => {
                KotlinReturnMeta {
                    is_unit: false,
                    is_direct: false,
                    direct_is_nullable: false,
                    cast: String::new(),
                }
            }
        }
    }

    fn kotlin_host_return_meta(
        &self,
        ret_shape: &ReturnShape,
        returns_def: &ReturnDef,
    ) -> KotlinReturnMeta {
        if matches!(returns_def, ReturnDef::Value(TypeExpr::String))
            && Self::is_utf8_string_return(ret_shape)
        {
            return KotlinReturnMeta {
                is_unit: false,
                is_direct: true,
                direct_is_nullable: true,
                cast: String::new(),
            };
        }
        self.kotlin_return_meta(ret_shape)
    }

    fn kotlin_async_host_return_meta(
        &self,
        ret_shape: &ReturnShape,
        returns_def: &ReturnDef,
    ) -> KotlinReturnMeta {
        if matches!(returns_def, ReturnDef::Value(TypeExpr::String))
            && Self::is_utf8_string_return(ret_shape)
        {
            return KotlinReturnMeta {
                is_unit: false,
                is_direct: false,
                direct_is_nullable: false,
                cast: String::new(),
            };
        }
        self.kotlin_host_return_meta(ret_shape, returns_def)
    }

    fn is_utf8_string_return(ret_shape: &ReturnShape) -> bool {
        matches!(
            ret_shape.value_return_strategy(),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String)
        )
    }

    fn kotlin_handle_return_cast(&self, class_id: &ClassId, nullable: bool) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}(it) }}", class_name)
        } else {
            format!(".let {{ {}(it) }}", class_name)
        }
    }

    fn kotlin_callback_return_cast(&self, callback_id: &CallbackId, nullable: bool) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(".takeIf {{ it != 0L }}?.let {{ {}.wrap(it) }}", bridge)
        } else {
            format!(".let {{ {}.wrap(it) }}", bridge)
        }
    }

    fn kotlin_return_cast(&self, abi: &AbiType) -> String {
        match abi {
            AbiType::U8 => ".toUByte()".to_string(),
            AbiType::U16 => ".toUShort()".to_string(),
            AbiType::U32 => ".toUInt()".to_string(),
            AbiType::U64 | AbiType::USize => ".toULong()".to_string(),
            _ => String::new(),
        }
    }

    fn is_throwing_return(&self, returns: &ReturnDef) -> bool {
        matches!(returns, ReturnDef::Result { .. })
    }

    fn error_type_name(&self, returns: &ReturnDef) -> String {
        match returns {
            ReturnDef::Result { err, .. } => match err {
                TypeExpr::Enum(id) if self.is_error_enum(id) => {
                    NamingConvention::class_name(id.as_str())
                }
                TypeExpr::Record(id) if self.is_error_record(id) => {
                    NamingConvention::class_name(id.as_str())
                }
                _ => "FfiException".to_string(),
            },
            _ => "FfiException".to_string(),
        }
    }

    fn err_to_throwable(&self, err: &TypeExpr) -> String {
        match err {
            TypeExpr::String => "FfiException(-1, err)".to_string(),
            TypeExpr::Enum(id) if self.is_error_enum(id) => "err".to_string(),
            TypeExpr::Record(id) if self.is_error_record(id) => "err".to_string(),
            _ => "FfiException(-1, \"Error: $err\")".to_string(),
        }
    }

    fn is_error_enum(&self, id: &EnumId) -> bool {
        self.contract
            .catalog
            .resolve_enum(id)
            .map(|enumeration| enumeration.is_error)
            .unwrap_or(false)
    }

    fn is_error_record(&self, id: &RecordId) -> bool {
        self.contract
            .catalog
            .resolve_record(id)
            .map(|record| record.is_error)
            .unwrap_or(false)
    }

    fn wire_writers_for_params(&self, call: &AbiCall) -> Vec<KotlinWireWriter> {
        call.params
            .iter()
            .filter_map(|param| {
                self.pointer_sized_scalar_span_writer(param)
                    .or_else(|| self.composite_span_buffer_binding(param))
                    .or_else(|| {
                        self.input_write_ops(param)
                            .map(|encode_ops| KotlinWireWriter::WireBuffer {
                                binding_name: format!("wire_writer_{}", param.name.as_str()),
                                size_expr: emit::emit_size_expr_for_write_seq(&encode_ops),
                                encode_expr: emit::emit_write_expr(&encode_ops),
                            })
                    })
            })
            .collect()
    }

    fn pointer_sized_scalar_span_writer(&self, param: &AbiParam) -> Option<KotlinWireWriter> {
        match &param.role {
            ParamRole::Input {
                transport:
                    Transport::Span(SpanContent::Scalar(ScalarOrigin::Primitive(
                        primitive @ (PrimitiveType::ISize | PrimitiveType::USize),
                    ))),
                ..
            } => {
                let name = NamingConvention::param_name(param.name.as_str());
                Some(KotlinWireWriter::WireBuffer {
                    binding_name: format!("wire_writer_{}", param.name.as_str()),
                    size_expr: format!("4 + {}.size * {}", name, primitive.wire_size_bytes()),
                    encode_expr: format!("wire.writePrimitiveList({})", name),
                })
            }
            _ => None,
        }
    }

    fn composite_span_buffer_binding(&self, param: &AbiParam) -> Option<KotlinWireWriter> {
        let ParamRole::Input {
            transport: Transport::Span(SpanContent::Composite(layout)),
            ..
        } = &param.role
        else {
            return None;
        };

        let binding_name = format!("wire_writer_{}", param.name.as_str());
        let param_name = NamingConvention::param_name(param.name.as_str());
        let writer_name = format!(
            "{}Writer",
            NamingConvention::class_name(layout.record_id.as_str())
        );
        Some(KotlinWireWriter::PackedBuffer {
            binding_name,
            pack_expr: format!("{writer_name}.pack({param_name})"),
        })
    }

    fn native_arg_for_mapping(
        &self,
        param: &AbiParam,
        mapping: &JniParamMapping,
        writers: &[KotlinWireWriter],
    ) -> String {
        let name = NamingConvention::param_name(param.name.as_str());
        match &mapping.role {
            JniParamRole::Direct {
                c_style_enum_id: Some(_),
                ..
            } => format!("{}.value", name),
            JniParamRole::Direct { .. } => match &param.abi_type {
                AbiType::U64 | AbiType::USize => format!("{}.toLong()", name),
                AbiType::U32 => format!("{}.toInt()", name),
                AbiType::U16 => format!("{}.toShort()", name),
                AbiType::U8 => format!("{}.toByte()", name),
                _ => name,
            },
            JniParamRole::StringParam => format!("{}.toByteArray(Charsets.UTF_8)", name),
            JniParamRole::Buffer { .. } => match &param.role {
                ParamRole::Input {
                    transport:
                        Transport::Span(SpanContent::Scalar(ScalarOrigin::CStyleEnum {
                            tag_type, ..
                        })),
                    ..
                } => {
                    let array_type = self.kotlin_array_type_for_primitive(*tag_type);
                    let element_expr =
                        self.kotlin_integral_cast_expr(*tag_type, &format!("{}[it].value", name));
                    format!("{}({}.size) {{ {} }}", array_type, name, element_expr)
                }
                _ => name,
            },
            JniParamRole::Encoded => writers
                .iter()
                .find(|writer| {
                    writer.binding_name() == format!("wire_writer_{}", param.name.as_str())
                })
                .map(KotlinWireWriter::native_buffer_expr)
                .unwrap_or_else(|| "wire.buffer".to_string()),
            JniParamRole::Handle { nullable } => {
                if *nullable {
                    format!("{}?.handle ?: 0L", name)
                } else {
                    format!("{}.handle", name)
                }
            }
            JniParamRole::Callback {
                callback_id,
                nullable,
            } => {
                let bridge = format!(
                    "{}Bridge",
                    NamingConvention::class_name(callback_id.as_str())
                );
                if *nullable {
                    format!("{}?.let {{ {}.create(it) }} ?: 0L", name, bridge)
                } else {
                    format!("{}.create({})", bridge, name)
                }
            }
            JniParamRole::OutBuffer => self.writer_pack_expr_for_param(param, &name),
            JniParamRole::Hidden => name,
        }
    }

    fn jni_param_mappings<'b>(
        &self,
        call: &'b AbiCall,
        param_defs: &[ParamDef],
    ) -> Vec<(&'b AbiParam, JniParamMapping)> {
        let input_params: Vec<_> = call
            .params
            .iter()
            .filter(|p| matches!(p.role, ParamRole::Input { .. }))
            .collect();
        let mut def_iter = param_defs.iter();
        call.params
            .iter()
            .map(|param| {
                let type_expr = if matches!(param.role, ParamRole::Input { .. }) {
                    if input_params.iter().any(|p| {
                        p.name.as_str() == "self"
                            && matches!(
                                p.role,
                                ParamRole::Input {
                                    transport: Transport::Handle { .. },
                                    ..
                                }
                            )
                    }) && param.name.as_str() == "self"
                    {
                        None
                    } else {
                        def_iter.next().map(|d| &d.type_expr)
                    }
                } else {
                    None
                };
                (param, self.jni_param_mapping(param, type_expr))
            })
            .collect()
    }

    fn visible_native_params<'b>(&'b self, call: &'b AbiCall) -> Vec<&'b AbiParam> {
        let mappings = self.jni_param_mappings(call, &[]);
        let len_params: HashSet<&ParamName> = mappings
            .iter()
            .filter_map(|(_, m)| m.len_companion.as_ref())
            .collect();
        mappings
            .iter()
            .filter(|(param, mapping)| !len_params.contains(&param.name) && mapping.is_visible())
            .map(|(param, _)| *param)
            .collect()
    }

    fn native_args_for_params(
        &self,
        call: &AbiCall,
        param_defs: &[ParamDef],
        writers: &[KotlinWireWriter],
    ) -> Vec<String> {
        let mappings = self.jni_param_mappings(call, param_defs);
        let len_params: HashSet<&ParamName> = mappings
            .iter()
            .filter_map(|(_, m)| m.len_companion.as_ref())
            .collect();
        mappings
            .iter()
            .filter(|(param, mapping)| !len_params.contains(&param.name) && mapping.is_visible())
            .map(|(param, mapping)| self.native_arg_for_mapping(param, mapping, writers))
            .collect()
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

    fn decode_expr_for_call_return(
        &self,
        ret_shape: &ReturnShape,
        returns_def: &ReturnDef,
    ) -> String {
        if !self.is_throwing_return(returns_def)
            && let Some(direct_decode) = self.decode_direct_buffer_return(ret_shape)
        {
            return direct_decode;
        }
        if let Some(decode_ops) = &ret_shape.decode_ops {
            if self.is_throwing_return(returns_def) {
                self.decode_result_expr(returns_def, decode_ops)
            } else if self.is_blittable_return(ret_shape, returns_def) {
                self.decode_blittable_return(ret_shape, decode_ops)
            } else {
                emit::emit_reader_read(decode_ops)
            }
        } else {
            match &ret_shape.transport {
                None | Some(Transport::Scalar(_)) => String::new(),
                Some(Transport::Handle { class_id, nullable }) => {
                    self.decode_handle_return(class_id, *nullable, "result")
                }
                Some(Transport::Callback {
                    callback_id,
                    nullable,
                    ..
                }) => self.decode_callback_return(callback_id, *nullable, "result"),
                _ => unreachable!(),
            }
        }
    }

    fn decode_result_expr(&self, returns: &ReturnDef, decode_ops: &ReadSeq) -> String {
        let (ok_seq, err_seq) = match decode_ops.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            _ => return emit::emit_reader_read(decode_ops),
        };
        let raw_ok_expr = emit::emit_reader_read(ok_seq);
        let ok_expr = match returns {
            ReturnDef::Result {
                ok: TypeExpr::Void, ..
            } => "Unit".to_string(),
            ReturnDef::Result {
                ok: TypeExpr::Enum(id),
                ..
            } if self
                .contract
                .catalog
                .resolve_enum(id)
                .map(|e| matches!(e.repr, EnumRepr::CStyle { .. }))
                .unwrap_or(false) =>
            {
                let enum_name = NamingConvention::class_name(id.as_str());
                format!("{}.fromValue({})", enum_name, raw_ok_expr)
            }
            _ => raw_ok_expr,
        };
        let err_expr = emit::emit_reader_read(err_seq);
        format!(
            "reader.readResult({{ {} }}, {{ {} }}).getOrThrow()",
            ok_expr, err_expr
        )
    }

    fn decode_handle_return(&self, class_id: &ClassId, nullable: bool, value_expr: &str) -> String {
        let class_name = NamingConvention::class_name(class_id.as_str());
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}(it) }}",
                value_expr, class_name
            )
        } else {
            format!("{}({})", class_name, value_expr)
        }
    }

    fn decode_callback_return(
        &self,
        callback_id: &CallbackId,
        nullable: bool,
        value_expr: &str,
    ) -> String {
        let bridge = format!(
            "{}Bridge",
            NamingConvention::class_name(callback_id.as_str())
        );
        if nullable {
            format!(
                "{}.takeIf {{ it != 0L }}?.let {{ {}.wrap(it) }}",
                value_expr, bridge
            )
        } else {
            format!("{}.wrap({})", bridge, value_expr)
        }
    }

    fn is_blittable_return(&self, ret_shape: &ReturnShape, returns_def: &ReturnDef) -> bool {
        if self.is_throwing_return(returns_def) {
            return false;
        }
        match &ret_shape.transport {
            Some(Transport::Span(SpanContent::Scalar(_))) => true,
            Some(Transport::Span(SpanContent::Composite(_))) => true,
            _ => ret_shape
                .decode_ops
                .as_ref()
                .map(|ops| self.is_blittable_decode_seq(ops))
                .unwrap_or(false),
        }
    }

    fn decode_direct_buffer_return(&self, ret_shape: &ReturnShape) -> Option<String> {
        match &ret_shape.transport {
            Some(Transport::Span(SpanContent::Scalar(origin))) => {
                Some(self.decode_direct_scalar_vec(origin))
            }
            Some(Transport::Span(SpanContent::Composite(layout))) => {
                let class_name = NamingConvention::class_name(layout.record_id.as_str());
                Some(format!(
                    "{}Reader.readAll(buffer, 0, buffer.capacity() / {}Reader.STRUCT_SIZE)",
                    class_name, class_name
                ))
            }
            _ => None,
        }
    }

    fn decode_direct_scalar_vec(&self, origin: &ScalarOrigin) -> String {
        match origin {
            ScalarOrigin::Primitive(p) => self.decode_direct_primitive_vec(*p),
            ScalarOrigin::CStyleEnum { enum_id, tag_type } => {
                let class_name = NamingConvention::class_name(enum_id.as_str());
                let array_view = self.kotlin_buffer_view_for_primitive(*tag_type);
                format!(
                    "{}.let {{ values -> List(values.size) {{ {}.fromValue(values[it]) }} }}",
                    array_view, class_name
                )
            }
        }
    }

    fn decode_direct_primitive_vec(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::I8 => "ByteArray(buffer.remaining()).also { buffer.get(it) }".to_string(),
            PrimitiveType::U8 => "buffer.remaining().let { n -> ByteArray(n).also { buffer.get(it) } }".to_string(),
            PrimitiveType::I16 => "buffer.asShortBuffer().let { sb -> ShortArray(sb.remaining()).also { sb.get(it) } }".to_string(),
            PrimitiveType::U16 => "buffer.asShortBuffer().let { sb -> ShortArray(sb.remaining()).also { sb.get(it) } }".to_string(),
            PrimitiveType::I32 => "buffer.asIntBuffer().let { ib -> IntArray(ib.remaining()).also { ib.get(it) } }".to_string(),
            PrimitiveType::U32 => "buffer.asIntBuffer().let { ib -> IntArray(ib.remaining()).also { ib.get(it) } }".to_string(),
            PrimitiveType::I64 | PrimitiveType::ISize => "buffer.asLongBuffer().let { lb -> LongArray(lb.remaining()).also { lb.get(it) } }".to_string(),
            PrimitiveType::U64 | PrimitiveType::USize => "buffer.asLongBuffer().let { lb -> LongArray(lb.remaining()).also { lb.get(it) } }".to_string(),
            PrimitiveType::F32 => "buffer.asFloatBuffer().let { fb -> FloatArray(fb.remaining()).also { fb.get(it) } }".to_string(),
            PrimitiveType::F64 => "buffer.asDoubleBuffer().let { db -> DoubleArray(db.remaining()).also { db.get(it) } }".to_string(),
            PrimitiveType::Bool => "ByteArray(buffer.remaining()).also { buffer.get(it) }.map { it != 0.toByte() }.toBooleanArray()".to_string(),
        }
    }

    fn decode_blittable_return(&self, ret_shape: &ReturnShape, decode_ops: &ReadSeq) -> String {
        if let Some(direct_decode) = self.decode_direct_buffer_return(ret_shape) {
            return direct_decode;
        }
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => {
                format!(
                    "{}Reader.read(buffer, 0)",
                    NamingConvention::class_name(id.as_str())
                )
            }
            Some(ReadOp::Vec {
                element_type: TypeExpr::Record(id),
                layout: VecLayout::Blittable { .. },
                ..
            }) => format!(
                "{}Reader.readAll(buffer, 4, buffer.getInt(0))",
                NamingConvention::class_name(id.as_str())
            ),
            _ => emit::emit_reader_read(decode_ops),
        }
    }

    fn async_call_for_method(
        &self,
        _class: &ClassDef,
        method: &MethodDef,
        call: &AbiCall,
    ) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => unreachable!("async method missing async call"),
        };
        let result_route = &async_call.result;
        let return_meta = self.kotlin_async_host_return_meta(result_route, &method.returns);
        let decode_expr = self.decode_expr_for_call_return(result_route, &method.returns);
        let is_blittable_return = self.is_blittable_return(result_route, &method.returns);
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            direct_return_is_nullable: return_meta.direct_is_nullable,
            return_cast: return_meta.cast,
            decode_expr,
            is_blittable_return,
        }
    }

    fn is_blittable_decode_seq(&self, decode_ops: &ReadSeq) -> bool {
        match decode_ops.ops.first() {
            Some(ReadOp::Record { id, .. }) => self
                .contract
                .catalog
                .resolve_record(id)
                .map(|record| record.is_blittable())
                .unwrap_or(false),
            Some(ReadOp::Vec {
                element_type,
                layout,
                ..
            }) => {
                matches!(layout, VecLayout::Blittable { .. })
                    && matches!(element_type, TypeExpr::Record(_))
            }
            _ => false,
        }
    }

    fn async_call_for_function(&self, func: &FunctionDef, call: &AbiCall) -> KotlinAsyncCall {
        let async_call = match &call.mode {
            CallMode::Async(async_call) => async_call,
            CallMode::Sync => unreachable!("async function missing async call"),
        };
        let result_route = &async_call.result;
        let return_meta = self.kotlin_async_host_return_meta(result_route, &func.returns);
        let decode_expr = self.decode_expr_for_call_return(result_route, &func.returns);
        let is_blittable_return = self.is_blittable_return(result_route, &func.returns);
        KotlinAsyncCall {
            poll: async_call.poll.as_str().to_string(),
            complete: async_call.complete.as_str().to_string(),
            cancel: async_call.cancel.as_str().to_string(),
            free: async_call.free.as_str().to_string(),
            return_is_unit: return_meta.is_unit,
            return_is_direct: return_meta.is_direct,
            direct_return_is_nullable: return_meta.direct_is_nullable,
            return_cast: return_meta.cast,
            decode_expr,
            is_blittable_return,
        }
    }

    fn record_struct_size(&self, record_id: &str) -> usize {
        self.abi
            .records
            .iter()
            .find(|record| record.id.as_str() == record_id)
            .and_then(|record| record.size)
            .unwrap_or(0)
    }

    fn field_padding_after(&self, record_id: &RecordId, field_name: &FieldName) -> usize {
        let record = match self.abi_record_for(record_id) {
            Some(record) if record.is_blittable => record,
            _ => return 0,
        };

        let fields = match self.record_field_offsets(record) {
            Some(fields) => fields,
            None => return 0,
        };
        let current = match fields.iter().find(|field| field.name == *field_name) {
            Some(field) => field,
            None => return 0,
        };
        let next_offset = fields
            .iter()
            .filter(|field| field.offset > current.offset)
            .map(|field| field.offset)
            .min()
            .unwrap_or(record.size.unwrap_or(0));

        next_offset.saturating_sub(current.offset + current.size)
    }

    fn should_generate_fixed_enum_codec(&self, enumeration: &EnumDef) -> bool {
        match &enumeration.repr {
            EnumRepr::Data { variants, .. } => {
                variants.iter().all(|variant| match &variant.payload {
                    VariantPayload::Unit => true,
                    VariantPayload::Struct(fields) => fields
                        .iter()
                        .all(|field| matches!(field.type_expr, TypeExpr::Primitive(_))),
                    VariantPayload::Tuple(fields) => {
                        fields.iter().all(|ty| matches!(ty, TypeExpr::Primitive(_)))
                    }
                })
            }
            _ => false,
        }
    }

    fn data_enum_layout(&self, enumeration: &EnumDef) -> Option<DataEnumLayout> {
        let EnumRepr::Data { variants, .. } = &enumeration.repr else {
            return None;
        };

        let tag_size = 4usize;
        let tag_alignment = 4usize;

        let variant_layouts = variants
            .iter()
            .map(|variant| self.data_enum_variant_layout(&variant.payload))
            .collect::<Vec<_>>();

        let union_alignment = variant_layouts
            .iter()
            .map(|layout| layout.alignment)
            .max()
            .unwrap_or(1);

        let union_size = variant_layouts
            .iter()
            .map(|layout| layout.size)
            .max()
            .unwrap_or(0);

        let payload_offset = align_up(tag_size, union_alignment);
        let struct_alignment = tag_alignment.max(union_alignment);
        let struct_size = align_up(
            payload_offset + align_up(union_size, union_alignment),
            struct_alignment,
        );

        Some(DataEnumLayout {
            struct_size,
            payload_offset,
            variant_offsets: variant_layouts
                .into_iter()
                .map(|layout| layout.offsets)
                .collect(),
        })
    }

    fn data_enum_variant_layout(&self, payload: &VariantPayload) -> DataEnumVariantLayout {
        match payload {
            VariantPayload::Unit => DataEnumVariantLayout {
                offsets: Vec::new(),
                size: 0,
                alignment: 1,
            },
            VariantPayload::Struct(fields) => {
                let primitives = fields
                    .iter()
                    .map(|field| match field.type_expr {
                        TypeExpr::Primitive(primitive) => primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
            VariantPayload::Tuple(fields) => {
                let primitives = fields
                    .iter()
                    .map(|ty| match ty {
                        TypeExpr::Primitive(primitive) => *primitive,
                        _ => panic!("data enum payload must be primitive"),
                    })
                    .collect::<Vec<_>>();
                self.primitive_fields_layout(&primitives)
            }
        }
    }

    fn primitive_fields_layout(&self, primitives: &[PrimitiveType]) -> DataEnumVariantLayout {
        let (offsets, size, alignment) = primitives.iter().fold(
            (Vec::new(), 0usize, 1usize),
            |(mut offsets, mut current, mut alignment), primitive| {
                let (size, align) = primitive_layout(*primitive);
                let aligned = align_up(current, align);
                offsets.push(aligned);
                current = aligned + size;
                alignment = alignment.max(align);
                (offsets, current, alignment)
            },
        );
        DataEnumVariantLayout {
            offsets,
            size: align_up(size, alignment),
            alignment,
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

    fn record_field_offsets(&self, record: &AbiRecord) -> Option<Vec<RecordFieldOffset>> {
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| {
                    let offset = read_seq_offset(&field.seq)?;
                    let size = match &field.seq.size {
                        SizeExpr::Fixed(value) => *value,
                        _ => return None,
                    };
                    Some(RecordFieldOffset {
                        name: field.name.clone(),
                        offset,
                        size,
                    })
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn record_blittable_fields(&self, record_id: &RecordId) -> Option<Vec<RecordBlittableField>> {
        let record = self.abi_record_for(record_id)?;
        if !record.is_blittable {
            return None;
        }
        match record.decode_ops.ops.first() {
            Some(ReadOp::Record { fields, .. }) => fields
                .iter()
                .map(|field| match field.seq.ops.first() {
                    Some(ReadOp::Primitive { primitive, .. }) => {
                        read_seq_offset(&field.seq).map(|offset_value| RecordBlittableField {
                            name: field.name.clone(),
                            offset: offset_value,
                            primitive: *primitive,
                        })
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }

    fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    fn abi_enum_for(&self, enumeration: &EnumDef) -> &AbiEnum {
        self.abi
            .enums
            .iter()
            .find(|abi_enum| abi_enum.id == enumeration.id)
            .expect("abi enum missing")
    }

    fn abi_call_for_function(&self, function: &FunctionDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| call.id == CallId::Function(function.id.clone()))
            .expect("abi call missing for function")
    }

    fn abi_call_for_method(&self, class: &ClassDef, method: &MethodDef) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Method {
                        class_id: class.id.clone(),
                        method_id: method.id.clone(),
                    }
            })
            .expect("abi call missing for method")
    }

    fn abi_call_for_constructor(&self, class: &ClassDef, index: usize) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| {
                call.id
                    == CallId::Constructor {
                        class_id: class.id.clone(),
                        index,
                    }
            })
            .expect("abi call missing for constructor")
    }

    fn find_abi_call(&self, call_id: &CallId) -> &AbiCall {
        self.abi
            .calls
            .iter()
            .find(|call| call.id == *call_id)
            .expect("abi call missing")
    }

    fn abi_callback_for(&self, callback_id: &CallbackId) -> &AbiCallbackInvocation {
        self.abi
            .callbacks
            .iter()
            .find(|callback| callback.callback_id == *callback_id)
            .expect("abi callback missing")
    }

    fn abi_callback_method(
        &self,
        callback_id: &CallbackId,
        method_id: &MethodId,
    ) -> &AbiCallbackMethod {
        self.abi_callback_for(callback_id)
            .methods
            .iter()
            .find(|method| method.id == *method_id)
            .expect("abi callback method missing")
    }

    fn primitive_field_accessors(&self, primitive: PrimitiveType) -> (String, String, String) {
        let getter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "get",
            PrimitiveType::I16 | PrimitiveType::U16 => "getShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "getInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "getLong",
            PrimitiveType::F32 => "getFloat",
            PrimitiveType::F64 => "getDouble",
        }
        .to_string();

        let putter = match primitive {
            PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => "put",
            PrimitiveType::I16 | PrimitiveType::U16 => "putShort",
            PrimitiveType::I32 | PrimitiveType::U32 => "putInt",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "putLong",
            PrimitiveType::F32 => "putFloat",
            PrimitiveType::F64 => "putDouble",
        }
        .to_string();

        let conversion = match primitive {
            PrimitiveType::Bool => " != 0.toByte()",
            PrimitiveType::U8 => ".toUByte()",
            PrimitiveType::U16 => ".toUShort()",
            PrimitiveType::U32 => ".toUInt()",
            PrimitiveType::U64 | PrimitiveType::USize => ".toULong()",
            _ => "",
        }
        .to_string();

        (getter, putter, conversion)
    }

    fn primitive_write_value_expr(&self, primitive: PrimitiveType, value: &str) -> String {
        match primitive {
            PrimitiveType::Bool => format!("(if ({}) 1 else 0).toByte()", value),
            PrimitiveType::U8 => format!("({}).toByte()", value),
            PrimitiveType::U16 => format!("({}).toShort()", value),
            PrimitiveType::U32 => format!("({}).toInt()", value),
            PrimitiveType::U64 | PrimitiveType::USize => format!("({}).toLong()", value),
            _ => value.to_string(),
        }
    }

    fn kotlin_array_type_for_primitive(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::I8 | PrimitiveType::U8 => "ByteArray",
            PrimitiveType::I16 | PrimitiveType::U16 => "ShortArray",
            PrimitiveType::I32 | PrimitiveType::U32 => "IntArray",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "LongArray",
            PrimitiveType::F32 => "FloatArray",
            PrimitiveType::F64 => "DoubleArray",
            PrimitiveType::Bool => "BooleanArray",
        }
    }

    fn kotlin_integral_cast_expr(&self, primitive: PrimitiveType, value: &str) -> String {
        match primitive {
            PrimitiveType::I8 | PrimitiveType::U8 => format!("({}).toByte()", value),
            PrimitiveType::I16 | PrimitiveType::U16 => format!("({}).toShort()", value),
            PrimitiveType::I32 | PrimitiveType::U32 => format!("({}).toInt()", value),
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => {
                format!("({}).toLong()", value)
            }
            _ => value.to_string(),
        }
    }

    fn kotlin_wire_write_method_for_primitive(&self, primitive: PrimitiveType) -> &'static str {
        match primitive {
            PrimitiveType::I8 | PrimitiveType::U8 => "writeI8",
            PrimitiveType::I16 | PrimitiveType::U16 => "writeI16",
            PrimitiveType::I32 | PrimitiveType::U32 => "writeI32",
            PrimitiveType::I64
            | PrimitiveType::U64
            | PrimitiveType::ISize
            | PrimitiveType::USize => "writeI64",
            PrimitiveType::Bool => "writeBool",
            PrimitiveType::F32 => "writeF32",
            PrimitiveType::F64 => "writeF64",
        }
    }

    fn kotlin_buffer_view_for_primitive(&self, primitive: PrimitiveType) -> String {
        match primitive {
            PrimitiveType::I8 | PrimitiveType::U8 => {
                "ByteArray(buffer.remaining()).also { buffer.get(it) }".to_string()
            }
            PrimitiveType::I16 | PrimitiveType::U16 => {
                "buffer.asShortBuffer().let { sb -> ShortArray(sb.remaining()).also { sb.get(it) } }"
                    .to_string()
            }
            PrimitiveType::I32 | PrimitiveType::U32 => {
                "buffer.asIntBuffer().let { ib -> IntArray(ib.remaining()).also { ib.get(it) } }"
                    .to_string()
            }
            PrimitiveType::I64 | PrimitiveType::U64 | PrimitiveType::ISize | PrimitiveType::USize => {
                "buffer.asLongBuffer().let { lb -> LongArray(lb.remaining()).also { lb.get(it) } }"
                    .to_string()
            }
            PrimitiveType::Bool => {
                "ByteArray(buffer.remaining()).also { buffer.get(it) }.map { it != 0.toByte() }.toBooleanArray()"
                    .to_string()
            }
            PrimitiveType::F32 => {
                "buffer.asFloatBuffer().let { fb -> FloatArray(fb.remaining()).also { fb.get(it) } }"
                    .to_string()
            }
            PrimitiveType::F64 => {
                "buffer.asDoubleBuffer().let { db -> DoubleArray(db.remaining()).also { db.get(it) } }"
                    .to_string()
            }
        }
    }

    fn find_custom_read_seq(&self, custom: &CustomTypeId) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find_map(|seq| Self::read_seq_custom(&seq, custom))
    }

    fn read_seqs(&self) -> Vec<ReadSeq> {
        let record_seqs = self
            .abi
            .records
            .iter()
            .map(|record| record.decode_ops.clone());
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| enumeration.decode_ops.clone());
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| field.decode.clone()).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = self.output_read_ops(&call.returns);
            let param_seqs = call
                .params
                .iter()
                .filter_map(|param| self.input_read_ops(param));
            let error_seq = match &call.error {
                ErrorTransport::Encoded { decode_ops, .. } => Some(decode_ops),
                ErrorTransport::None | ErrorTransport::StatusCode => None,
            };
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => self.output_read_ops(&async_call.result),
                CallMode::Sync => None,
            };
            return_seq
                .into_iter()
                .chain(param_seqs)
                .chain(error_seq.cloned())
                .chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = self.output_read_ops(&method.returns);
                let param_seqs = method
                    .params
                    .iter()
                    .filter_map(|param| self.input_read_ops(param));
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn read_seq_custom(seq: &ReadSeq, custom: &CustomTypeId) -> Option<ReadSeq> {
        seq.ops.iter().find_map(|op| match op {
            ReadOp::Custom { id, underlying } if id == custom => Some(*underlying.clone()),
            ReadOp::Option { some, .. } => Self::read_seq_custom(some, custom),
            ReadOp::Vec { element, .. } => Self::read_seq_custom(element, custom),
            ReadOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| Self::read_seq_custom(&field.seq, custom)),
            ReadOp::Result { ok, err, .. } => {
                Self::read_seq_custom(ok, custom).or_else(|| Self::read_seq_custom(err, custom))
            }
            _ => None,
        })
    }

    fn find_custom_write_seq(&self, custom: &CustomTypeId) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find_map(|seq| Self::write_seq_custom(&seq, custom))
    }

    fn write_seqs(&self) -> Vec<WriteSeq> {
        let record_seqs = self
            .abi
            .records
            .iter()
            .map(|record| record.encode_ops.clone());
        let enum_seqs = self
            .abi
            .enums
            .iter()
            .map(|enumeration| enumeration.encode_ops.clone());
        let enum_field_seqs = self.abi.enums.iter().flat_map(|enumeration| {
            enumeration
                .variants
                .iter()
                .flat_map(|variant| match &variant.payload {
                    AbiEnumPayload::Unit => Vec::new(),
                    AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => {
                        fields.iter().map(|field| field.encode.clone()).collect()
                    }
                })
        });
        let call_seqs = self.abi.calls.iter().flat_map(|call| {
            let return_seq = self.output_write_ops(&call.returns);
            let param_seqs = call
                .params
                .iter()
                .filter_map(|param| self.input_write_ops(param));
            let async_seq = match &call.mode {
                CallMode::Async(async_call) => self.output_write_ops(&async_call.result),
                CallMode::Sync => None,
            };
            return_seq.into_iter().chain(param_seqs).chain(async_seq)
        });
        let callback_seqs = self.abi.callbacks.iter().flat_map(|callback| {
            callback.methods.iter().flat_map(|method| {
                let return_seq = self.output_write_ops(&method.returns);
                let param_seqs = method
                    .params
                    .iter()
                    .filter_map(|param| self.input_write_ops(param));
                return_seq.into_iter().chain(param_seqs)
            })
        });

        record_seqs
            .chain(enum_seqs)
            .chain(enum_field_seqs)
            .chain(call_seqs)
            .chain(callback_seqs)
            .collect()
    }

    fn write_seq_custom(seq: &WriteSeq, custom: &CustomTypeId) -> Option<WriteSeq> {
        seq.ops.iter().find_map(|op| match op {
            WriteOp::Custom { id, underlying, .. } if id == custom => Some(*underlying.clone()),
            WriteOp::Option { some, .. } => Self::write_seq_custom(some, custom),
            WriteOp::Vec { element, .. } => Self::write_seq_custom(element, custom),
            WriteOp::Record { fields, .. } => fields
                .iter()
                .find_map(|field| Self::write_seq_custom(&field.seq, custom)),
            WriteOp::Result { ok, err, .. } => {
                Self::write_seq_custom(ok, custom).or_else(|| Self::write_seq_custom(err, custom))
            }
            _ => None,
        })
    }

    fn read_seq_from_repr(&self, repr: &TypeExpr) -> ReadSeq {
        self.find_read_seq_for_type(repr)
            .or_else(|| self.synthesized_read_seq_for_repr(repr))
            .unwrap_or_else(|| panic!("missing read ops for custom repr: {:?}", repr))
    }

    fn write_seq_from_repr(&self, repr: &TypeExpr) -> WriteSeq {
        self.find_write_seq_for_type(repr)
            .or_else(|| self.synthesized_write_seq_for_repr(repr))
            .unwrap_or_else(|| panic!("missing write ops for custom repr: {:?}", repr))
    }

    fn synthesized_read_seq_for_repr(&self, repr: &TypeExpr) -> Option<ReadSeq> {
        match repr {
            TypeExpr::Primitive(primitive) => Some(ReadSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![ReadOp::Primitive {
                    primitive: *primitive,
                    offset: OffsetExpr::Base,
                }],
                shape: WireShape::Value,
            }),
            TypeExpr::String => Some(ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::String {
                    offset: OffsetExpr::Base,
                }],
                shape: WireShape::Value,
            }),
            TypeExpr::Bytes => Some(ReadSeq {
                size: SizeExpr::Runtime,
                ops: vec![ReadOp::Bytes {
                    offset: OffsetExpr::Base,
                }],
                shape: WireShape::Value,
            }),
            _ => None,
        }
    }

    fn synthesized_write_seq_for_repr(&self, repr: &TypeExpr) -> Option<WriteSeq> {
        match repr {
            TypeExpr::Primitive(primitive) => Some(WriteSeq {
                size: SizeExpr::Fixed(primitive.wire_size_bytes()),
                ops: vec![WriteOp::Primitive {
                    primitive: *primitive,
                    value: ValueExpr::Var("repr".to_string()),
                }],
                shape: WireShape::Value,
            }),
            TypeExpr::String => Some(WriteSeq {
                size: SizeExpr::StringLen(ValueExpr::Var("repr".to_string())),
                ops: vec![WriteOp::String {
                    value: ValueExpr::Var("repr".to_string()),
                }],
                shape: WireShape::Value,
            }),
            TypeExpr::Bytes => Some(WriteSeq {
                size: SizeExpr::BytesLen(ValueExpr::Var("repr".to_string())),
                ops: vec![WriteOp::Bytes {
                    value: ValueExpr::Var("repr".to_string()),
                }],
                shape: WireShape::Value,
            }),
            _ => None,
        }
    }

    fn find_read_seq_for_type(&self, ty: &TypeExpr) -> Option<ReadSeq> {
        self.read_seqs()
            .into_iter()
            .find(|seq| Self::read_seq_matches_type(seq, ty))
    }

    fn find_write_seq_for_type(&self, ty: &TypeExpr) -> Option<WriteSeq> {
        self.write_seqs()
            .into_iter()
            .find(|seq| Self::write_seq_matches_type(seq, ty))
    }

    fn read_seq_matches_type(seq: &ReadSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(ReadOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(ReadOp::String { .. }), TypeExpr::String) => true,
            (Some(ReadOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(ReadOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(ReadOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(ReadOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(ReadOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(ReadOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(ReadOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                Self::read_seq_matches_type(some, inner)
            }
            (
                Some(ReadOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => Self::read_seq_matches_type(ok, ok_ty) && Self::read_seq_matches_type(err, err_ty),
            _ => false,
        }
    }

    fn write_seq_matches_type(seq: &WriteSeq, ty: &TypeExpr) -> bool {
        match (seq.ops.first(), ty) {
            (Some(WriteOp::Primitive { primitive, .. }), TypeExpr::Primitive(expected)) => {
                primitive == expected
            }
            (Some(WriteOp::String { .. }), TypeExpr::String) => true,
            (Some(WriteOp::Bytes { .. }), TypeExpr::Bytes) => true,
            (Some(WriteOp::Builtin { id, .. }), TypeExpr::Builtin(expected)) => id == expected,
            (Some(WriteOp::Record { id, .. }), TypeExpr::Record(expected)) => id == expected,
            (Some(WriteOp::Enum { id, .. }), TypeExpr::Enum(expected)) => id == expected,
            (Some(WriteOp::Custom { id, .. }), TypeExpr::Custom(expected)) => id == expected,
            (Some(WriteOp::Vec { element_type, .. }), TypeExpr::Vec(inner)) => {
                element_type == inner.as_ref()
            }
            (Some(WriteOp::Option { some, .. }), TypeExpr::Option(inner)) => {
                Self::write_seq_matches_type(some, inner)
            }
            (
                Some(WriteOp::Result { ok, err, .. }),
                TypeExpr::Result {
                    ok: ok_ty,
                    err: err_ty,
                },
            ) => {
                Self::write_seq_matches_type(ok, ok_ty) && Self::write_seq_matches_type(err, err_ty)
            }
            _ => false,
        }
    }

    fn blittable_reader_record_ids(&self) -> HashSet<String> {
        let sync_returns_from_decode = self.abi.calls.iter().filter_map(|call| {
            self.output_read_ops(&call.returns)
                .and_then(|seq| self.blittable_record_id_from_read_seq(&seq))
        });

        let sync_returns_from_transport = self
            .abi
            .calls
            .iter()
            .filter_map(|call| self.blittable_record_id_from_transport(&call.returns));

        let async_returns_from_decode = self.abi.calls.iter().filter_map(|call| match &call.mode {
            CallMode::Async(async_call) => self
                .output_read_ops(&async_call.result)
                .and_then(|seq| self.blittable_record_id_from_read_seq(&seq)),
            CallMode::Sync => None,
        });

        let async_returns_from_transport =
            self.abi.calls.iter().filter_map(|call| match &call.mode {
                CallMode::Async(async_call) => {
                    self.blittable_record_id_from_transport(&async_call.result)
                }
                CallMode::Sync => None,
            });

        let callback_returns_from_decode = self
            .abi
            .callbacks
            .iter()
            .flat_map(|callback| callback.methods.iter())
            .filter_map(|method| {
                self.output_read_ops(&method.returns)
                    .and_then(|seq| self.blittable_record_id_from_read_seq(&seq))
            });

        let callback_returns_from_transport = self
            .abi
            .callbacks
            .iter()
            .flat_map(|callback| callback.methods.iter())
            .filter_map(|method| self.blittable_record_id_from_transport(&method.returns));

        let stream_items =
            self.abi
                .streams
                .iter()
                .filter_map(|stream| match &stream.item_transport {
                    Transport::Composite(layout)
                        if self.is_record_blittable(layout.record_id.as_str()) =>
                    {
                        Some(layout.record_id.as_str().to_string())
                    }
                    _ => None,
                });

        sync_returns_from_decode
            .chain(sync_returns_from_transport)
            .chain(async_returns_from_decode)
            .chain(async_returns_from_transport)
            .chain(callback_returns_from_decode)
            .chain(callback_returns_from_transport)
            .chain(stream_items)
            .collect()
    }

    fn blittable_record_from_decode_ops(&self, ret_shape: &ReturnShape) -> Option<String> {
        ret_shape
            .decode_ops
            .as_ref()
            .and_then(|decode_ops| self.blittable_record_id_from_read_seq(decode_ops))
    }

    fn blittable_record_id_from_read_seq(&self, seq: &ReadSeq) -> Option<String> {
        match seq.ops.first() {
            Some(ReadOp::Record { id, .. }) if self.is_record_blittable(id.as_str()) => {
                Some(id.as_str().to_string())
            }
            Some(ReadOp::Vec {
                element_type: TypeExpr::Record(id),
                layout,
                ..
            }) if matches!(layout, VecLayout::Blittable { .. })
                && self.is_record_blittable(id.as_str()) =>
            {
                Some(id.as_str().to_string())
            }
            _ => None,
        }
    }

    fn is_record_blittable(&self, record_id: &str) -> bool {
        self.contract
            .catalog
            .resolve_record(&RecordId::new(record_id))
            .map(|record| record.is_blittable())
            .unwrap_or(false)
    }

    fn blittable_record_id_from_transport(&self, ret_shape: &ReturnShape) -> Option<String> {
        match &ret_shape.transport {
            Some(Transport::Composite(layout))
                if self.is_record_blittable(layout.record_id.as_str()) =>
            {
                Some(layout.record_id.as_str().to_string())
            }
            Some(Transport::Span(SpanContent::Composite(layout)))
                if self.is_record_blittable(layout.record_id.as_str()) =>
            {
                Some(layout.record_id.as_str().to_string())
            }
            _ => None,
        }
    }

    fn blittable_vec_param_records(&self) -> HashSet<String> {
        let types_from_functions = self
            .contract
            .functions
            .iter()
            .flat_map(|function| function.params.iter())
            .map(|param| &param.type_expr);
        let types_from_methods = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_ctors = self
            .contract
            .catalog
            .all_classes()
            .flat_map(|class| class.constructors.iter())
            .flat_map(|ctor| ctor.params().into_iter())
            .map(|param| &param.type_expr);
        let types_from_traits = self
            .contract
            .catalog
            .all_callbacks()
            .flat_map(|callback| callback.methods.iter())
            .flat_map(|method| method.params.iter())
            .map(|param| &param.type_expr);
        let types_from_records = self
            .contract
            .catalog
            .all_records()
            .flat_map(|record| record.fields.iter())
            .map(|field| &field.type_expr);
        let types_from_enums =
            self.contract
                .catalog
                .all_enums()
                .flat_map(|enumeration| match &enumeration.repr {
                    EnumRepr::Data { variants, .. } => variants
                        .iter()
                        .flat_map(|variant| match &variant.payload {
                            VariantPayload::Struct(fields) => fields
                                .iter()
                                .map(|field| &field.type_expr)
                                .collect::<Vec<_>>(),
                            VariantPayload::Tuple(fields) => fields.iter().collect::<Vec<_>>(),
                            VariantPayload::Unit => Vec::new(),
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                });

        types_from_functions
            .chain(types_from_methods)
            .chain(types_from_ctors)
            .chain(types_from_traits)
            .chain(types_from_records)
            .chain(types_from_enums)
            .filter_map(|ty| match ty {
                TypeExpr::Vec(inner) => match inner.as_ref() {
                    TypeExpr::Record(id) => Some(id.as_str().to_string()),
                    _ => None,
                },
                _ => None,
            })
            .filter(|record_name| {
                self.contract
                    .catalog
                    .all_records()
                    .any(|record| record.id.as_str() == *record_name && record.is_blittable())
            })
            .collect()
    }

    fn writer_pack_expr_for_param(&self, param: &AbiParam, kotlin_name: &str) -> String {
        let record_id = self.out_buffer_record_id(param);
        match record_id {
            Some(id) => format!(
                "{}Writer.pack({})",
                NamingConvention::class_name(&id),
                kotlin_name,
            ),
            None => kotlin_name.to_string(),
        }
    }

    fn out_buffer_record_id(&self, param: &AbiParam) -> Option<String> {
        match &param.role {
            ParamRole::OutDirect => {
                let decode_ops = match &param.role {
                    ParamRole::Input {
                        decode_ops: Some(d),
                        ..
                    } => d,
                    _ => return None,
                };
                match decode_ops.ops.first() {
                    Some(ReadOp::Vec {
                        element_type: TypeExpr::Record(id),
                        ..
                    }) => Some(id.as_str().to_string()),
                    Some(ReadOp::Record { id, .. }) => Some(id.as_str().to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn input_read_ops(&self, param: &AbiParam) -> Option<ReadSeq> {
        match &param.role {
            ParamRole::Input {
                decode_ops: Some(decode_ops),
                ..
            } => Some(decode_ops.clone()),
            _ => None,
        }
    }

    fn input_write_ops(&self, param: &AbiParam) -> Option<WriteSeq> {
        match &param.role {
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => Some(encode_ops.clone()),
            _ => None,
        }
    }

    fn output_read_ops(&self, ret_shape: &ReturnShape) -> Option<ReadSeq> {
        ret_shape.decode_ops.clone()
    }

    fn output_write_ops(&self, ret_shape: &ReturnShape) -> Option<WriteSeq> {
        ret_shape.encode_ops.clone()
    }
}

struct RecordFieldOffset {
    name: FieldName,
    offset: usize,
    size: usize,
}

struct RecordBlittableField {
    name: FieldName,
    offset: usize,
    primitive: PrimitiveType,
}

struct DataEnumLayout {
    struct_size: usize,
    payload_offset: usize,
    variant_offsets: Vec<Vec<usize>>,
}

struct DataEnumVariantLayout {
    offsets: Vec<usize>,
    size: usize,
    alignment: usize,
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

fn primitive_layout(primitive: PrimitiveType) -> (usize, usize) {
    let size = primitive.wire_size_bytes();
    let alignment = match primitive {
        PrimitiveType::Bool | PrimitiveType::I8 | PrimitiveType::U8 => 1,
        PrimitiveType::I16 | PrimitiveType::U16 => 2,
        PrimitiveType::I32 | PrimitiveType::U32 | PrimitiveType::F32 => 4,
        PrimitiveType::I64
        | PrimitiveType::U64
        | PrimitiveType::ISize
        | PrimitiveType::USize
        | PrimitiveType::F64 => 8,
    };
    (size, alignment)
}

fn read_seq_offset(seq: &ReadSeq) -> Option<usize> {
    let op = seq.ops.first()?;
    let offset = match op {
        ReadOp::Primitive { offset, .. }
        | ReadOp::String { offset }
        | ReadOp::Bytes { offset }
        | ReadOp::Builtin { offset, .. }
        | ReadOp::Record { offset, .. }
        | ReadOp::Enum { offset, .. } => offset,
        _ => return None,
    };
    match offset {
        OffsetExpr::Fixed(value) => Some(*value),
        OffsetExpr::Base => Some(0),
        OffsetExpr::BasePlus(value) => Some(*value),
        _ => None,
    }
}

fn kotlin_default_literal(default: &DefaultValue, kotlin_type: &str) -> String {
    use heck::ToUpperCamelCase;
    match default {
        DefaultValue::Bool(true) => "true".to_string(),
        DefaultValue::Bool(false) => "false".to_string(),
        DefaultValue::Integer(v) => match kotlin_type {
            "Double" => format!("{}.0", v),
            "Float" => format!("{}.0f", v),
            "UInt" => format!("{}u", v),
            "ULong" => format!("{}uL", v),
            "UShort" => format!("{}u", v),
            "UByte" => format!("{}u", v),
            "Long" => format!("{}L", v),
            _ => v.to_string(),
        },
        DefaultValue::Float(v) => {
            let has_decimal = v.fract() != 0.0;
            let base = if has_decimal {
                format!("{}", v)
            } else {
                format!("{}.0", v)
            };
            match kotlin_type {
                "Float" => format!("{}f", base),
                _ => base,
            }
        }
        DefaultValue::String(v) => format!("\"{}\"", v),
        DefaultValue::EnumVariant {
            enum_name,
            variant_name,
        } => format!(
            "{}.{}",
            enum_name.to_upper_camel_case(),
            NamingConvention::enum_entry_name(variant_name)
        ),
        DefaultValue::Null => "null".to_string(),
    }
}

struct KotlinPreamble {
    prefix: String,
    extra_imports: Vec<String>,
    custom_types: Vec<KotlinCustomType>,
    has_async_runtime: bool,
    has_streams: bool,
}

enum JniParamRole {
    Direct {
        jni_type: String,
        c_style_enum_id: Option<EnumId>,
    },
    StringParam,
    Buffer {
        jni_type: String,
    },
    Encoded,
    Handle {
        nullable: bool,
    },
    Callback {
        callback_id: CallbackId,
        nullable: bool,
    },
    OutBuffer,
    Hidden,
}

struct JniParamMapping {
    role: JniParamRole,
    len_companion: Option<ParamName>,
}

impl JniParamMapping {
    fn is_visible(&self) -> bool {
        !matches!(self.role, JniParamRole::Hidden)
    }

    fn jni_type(&self) -> String {
        match &self.role {
            JniParamRole::Direct { jni_type, .. } | JniParamRole::Buffer { jni_type } => {
                jni_type.clone()
            }
            JniParamRole::StringParam => "ByteArray".to_string(),
            JniParamRole::Encoded | JniParamRole::OutBuffer => "ByteBuffer".to_string(),
            JniParamRole::Handle { .. } | JniParamRole::Callback { .. } => "Long".to_string(),
            JniParamRole::Hidden => "Unit".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo, TypeCatalog};
    use crate::ir::definitions::{
        CallbackKind, CallbackMethodDef, CallbackTraitDef, ClassDef, FieldDef, FunctionDef,
        MethodDef, ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, StreamDef, StreamMode,
    };
    use crate::ir::ids::{
        CallbackId, ClassId, FieldName, FunctionId, MethodId, ParamName, RecordId, StreamId,
    };
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::callable::ExecutionKind;

    fn field(name: &str, primitive: PrimitiveType) -> FieldDef {
        FieldDef {
            name: FieldName::new(name),
            type_expr: TypeExpr::Primitive(primitive),
            doc: None,
            default: None,
        }
    }

    fn record_def(id: &str, fields: Vec<FieldDef>) -> RecordDef {
        RecordDef {
            id: RecordId::new(id),
            is_repr_c: true,
            is_error: false,
            fields,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn lower_module(contract: &FfiContract) -> KotlinModule {
        let abi = IrLowerer::new(contract).to_abi_contract();
        KotlinLowerer::new(
            contract,
            &abi,
            "com.example.demo".to_string(),
            "demo".to_string(),
            KotlinOptions::default(),
        )
        .lower()
    }

    fn async_string_function_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![FunctionDef {
                id: FunctionId::new("fetch_name"),
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Async,
                doc: None,
                deprecated: None,
            }],
        }
    }

    fn sync_string_function_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: TypeCatalog::default(),
            functions: vec![FunctionDef {
                id: FunctionId::new("fetch_name"),
                params: vec![],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            }],
        }
    }

    fn async_string_method_contract() -> FfiContract {
        let mut catalog = TypeCatalog::default();
        catalog.insert_class(ClassDef {
            id: ClassId::new("Named"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("load"),
                receiver: Receiver::OwnedSelf,
                params: vec![ParamDef {
                    name: ParamName::new("refresh"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::Bool),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Async,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        }
    }

    fn sync_string_method_contract() -> FfiContract {
        let mut catalog = TypeCatalog::default();
        catalog.insert_class(ClassDef {
            id: ClassId::new("Named"),
            constructors: vec![],
            methods: vec![MethodDef {
                id: MethodId::new("load"),
                receiver: Receiver::OwnedSelf,
                params: vec![ParamDef {
                    name: ParamName::new("refresh"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::Bool),
                    passing: ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::String),
                execution_kind: ExecutionKind::Sync,
                doc: None,
                deprecated: None,
            }],
            streams: vec![],
            doc: None,
            deprecated: None,
        });
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        }
    }

    #[test]
    fn lowering_emits_record_reader_for_blittable_stream_items() {
        let point_id = RecordId::new("Point");
        let mut catalog = crate::ir::contract::TypeCatalog::default();
        catalog.insert_record(record_def(
            point_id.as_str(),
            vec![
                field("x", PrimitiveType::F64),
                field("y", PrimitiveType::F64),
            ],
        ));
        catalog.insert_class(ClassDef {
            id: ClassId::new("EventBus"),
            constructors: vec![],
            methods: vec![],
            streams: vec![StreamDef {
                id: StreamId::new("subscribe_points"),
                item_type: TypeExpr::Record(point_id),
                mode: StreamMode::Async,
                doc: None,
                deprecated: None,
            }],
            doc: None,
            deprecated: None,
        });
        let contract = FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        };
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let module = KotlinLowerer::new(
            &contract,
            &abi,
            "com.example.demo".to_string(),
            "demo".to_string(),
            KotlinOptions::default(),
        )
        .lower();

        assert!(
            module
                .record_readers
                .iter()
                .any(|reader| reader.reader_name == "PointReader"),
            "expected PointReader to be emitted for stream items",
        );

        let stream = module
            .classes
            .iter()
            .find(|class| class.class_name == "EventBus")
            .and_then(|class| {
                class
                    .streams
                    .iter()
                    .find(|stream| stream.name == "subscribePoints")
            })
            .expect("stream should be lowered");
        assert!(
            stream.pop_batch_items_expr.contains("PointReader.readAll"),
            "expected stream items to be decoded via PointReader, got: {}",
            stream.pop_batch_items_expr,
        );
    }

    #[test]
    fn async_string_function_uses_status_checked_buffer_completion() {
        let contract = async_string_function_contract();
        let module = lower_module(&contract);
        let function = module
            .functions
            .iter()
            .find(|function| function.func_name == "fetchName")
            .expect("async function should be lowered");
        let native = module
            .native
            .functions
            .iter()
            .find(|function| function.ffi_name == "boltffi_fetch_name")
            .expect("native async function should be lowered");
        let async_call = function
            .async_call
            .as_ref()
            .expect("async function should include async metadata");
        let native_async = native
            .async_ffi
            .as_ref()
            .expect("native async function should include async ffi metadata");

        assert!(!async_call.return_is_direct);
        assert_eq!(async_call.decode_expr, "reader.readString()");
        assert_eq!(native_async.complete_return_jni_type, "ByteArray?");
    }

    #[test]
    fn async_string_method_uses_status_checked_buffer_completion() {
        let contract = async_string_method_contract();
        let module = lower_module(&contract);
        let class = module
            .classes
            .iter()
            .find(|class| class.class_name == "Named")
            .expect("record methods should be lowered to a class");
        let method = class
            .methods
            .iter()
            .find_map(|method| match &method.impl_ {
                KotlinMethodImpl::AsyncMethod(rendered)
                    if rendered.contains("suspend fun load(") =>
                {
                    Some(rendered)
                }
                _ => None,
            })
            .expect("async method should be lowered");
        let native_class = module
            .native
            .classes
            .iter()
            .find(|class| class.ffi_free == "boltffi_named_free")
            .expect("native class should be lowered");
        let native_method = native_class
            .async_methods
            .iter()
            .find(|method| method.ffi_name == "boltffi_named_load")
            .expect("native async method should be lowered");

        assert!(method.contains("val buf = Native.boltffi_named_load_complete(handle, future)"));
        assert!(method.contains("reader.readString()"));
        assert_eq!(native_method.return_jni_type, "ByteArray?");
    }

    #[test]
    fn sync_string_function_preserves_null_guard_on_direct_return() {
        let contract = sync_string_function_contract();
        let module = lower_module(&contract);
        let rendered = crate::render::kotlin::templates::KotlinEmitter::emit(&module);
        let function = module
            .functions
            .iter()
            .find(|function| function.func_name == "fetchName")
            .expect("sync function should be lowered");
        let native = module
            .native
            .functions
            .iter()
            .find(|function| function.ffi_name == "boltffi_fetch_name")
            .expect("native sync function should be lowered");

        assert!(function.return_is_direct);
        assert!(function.direct_return_is_nullable);
        assert!(rendered.contains("val result = Native.boltffi_fetch_name()"));
        assert!(rendered.contains("?: throw FfiException(-1, \"Null buffer returned\")"));
        assert_eq!(native.return_jni_type, "String?");
    }

    #[test]
    fn sync_string_method_preserves_null_guard_on_direct_return() {
        let contract = sync_string_method_contract();
        let module = lower_module(&contract);
        let class = module
            .classes
            .iter()
            .find(|class| class.class_name == "Named")
            .expect("class should be lowered");
        let method = class
            .methods
            .iter()
            .find_map(|method| match &method.impl_ {
                KotlinMethodImpl::SyncMethod(rendered) if rendered.contains("fun load(") => {
                    Some(rendered)
                }
                _ => None,
            })
            .expect("sync method should be lowered");
        let native_class = module
            .native
            .classes
            .iter()
            .find(|class| class.ffi_free == "boltffi_named_free")
            .expect("native class should be lowered");
        let native_method = native_class
            .sync_methods
            .iter()
            .find(|method| method.ffi_name == "boltffi_named_load")
            .expect("native sync method should be lowered");

        assert!(method.contains("val result = Native.boltffi_named_load(handle, refresh)"));
        assert!(method.contains("?: throw FfiException(-1, \"Null buffer returned\")"));
        assert_eq!(native_method.return_jni_type, "String?");
    }

    #[test]
    fn lowering_emits_record_reader_for_blittable_callback_returns() {
        let point_id = RecordId::new("DataPoint");
        let mut catalog = crate::ir::contract::TypeCatalog::default();
        catalog.insert_record(record_def(
            point_id.as_str(),
            vec![
                field("x", PrimitiveType::F64),
                field("y", PrimitiveType::F64),
                field("timestamp", PrimitiveType::I64),
            ],
        ));
        catalog.insert_callback(CallbackTraitDef {
            id: CallbackId::new("DataProvider"),
            methods: vec![CallbackMethodDef {
                id: MethodId::new("get_item"),
                params: vec![ParamDef {
                    name: ParamName::new("index"),
                    type_expr: TypeExpr::Primitive(PrimitiveType::U32),
                    passing: crate::ir::definitions::ParamPassing::Value,
                    doc: None,
                }],
                returns: ReturnDef::Value(TypeExpr::Record(point_id)),
                execution_kind: ExecutionKind::Sync,
                doc: None,
            }],
            kind: CallbackKind::Trait,
            doc: None,
        });
        let contract = FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog,
            functions: vec![],
        };
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let module = KotlinLowerer::new(
            &contract,
            &abi,
            "com.example.demo".to_string(),
            "demo".to_string(),
            KotlinOptions::default(),
        )
        .lower();

        assert!(
            module
                .record_readers
                .iter()
                .any(|reader| reader.reader_name == "DataPointReader"),
            "expected DataPointReader to be emitted for callback proxy returns",
        );

        let callback = module
            .callbacks
            .iter()
            .find(|callback| callback.interface_name == "DataProvider")
            .expect("callback should be lowered");
        let proxy_method = callback
            .proxy_methods
            .iter()
            .find(|method| method.contains("override fun getItem"))
            .expect("proxy method should exist");
        assert!(
            proxy_method.contains("DataPointReader.read(buffer, 0)"),
            "expected callback proxy to decode via DataPointReader, got: {}",
            proxy_method,
        );
    }
}
