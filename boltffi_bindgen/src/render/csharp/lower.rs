use std::collections::HashSet;

use boltffi_ffi_rules::naming;

use crate::ir::abi::{
    AbiCall, AbiEnum, AbiEnumField, AbiEnumPayload, AbiEnumVariant, AbiParam, AbiRecord, CallId,
    ParamRole,
};
use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::definitions::{
    ConstructorDef, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef, ParamPassing,
    Receiver, RecordDef, ReturnDef, VariantPayload,
};
use crate::ir::ids::{EnumId, FieldName, RecordId};
use crate::ir::ops::{FieldWriteOp, ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::TypeExpr;
use crate::ir::{AbiContract, FfiContract};

use super::emit;
use super::mappings;
use super::plan::{
    CSharpEnum, CSharpEnumKind, CSharpEnumVariant, CSharpFunction, CSharpMethod, CSharpModule,
    CSharpParam, CSharpParamKind, CSharpReceiver, CSharpRecord, CSharpRecordField,
    CSharpReturnKind, CSharpType, CSharpWireWriter,
};
use super::{CSharpOptions, NamingConvention};

/// Transforms the language-agnostic [`FfiContract`] and [`AbiContract`] into
/// a [`CSharpModule`] containing everything the C# templates need to render.
pub struct CSharpLowerer<'a> {
    ffi: &'a FfiContract,
    abi: &'a AbiContract,
    options: &'a CSharpOptions,
    /// Records that are fully supported — every field resolves to a type the
    /// C# backend can currently render. Populated up front because whether
    /// a record is supported can depend on whether *other* records are
    /// supported, so we need a fixed-point pass before lowering individual
    /// functions or records.
    supported_records: HashSet<String>,
    /// Enums that are fully supported. An enum qualifies when every
    /// variant's payload fields resolve to supported types. C-style
    /// variants have no fields and are trivially admitted.
    supported_enums: HashSet<String>,
}

impl<'a> CSharpLowerer<'a> {
    pub fn new(ffi: &'a FfiContract, abi: &'a AbiContract, options: &'a CSharpOptions) -> Self {
        let (supported_records, supported_enums) = Self::compute_supported_sets(ffi);
        Self {
            ffi,
            abi,
            options,
            supported_records,
            supported_enums,
        }
    }

    /// Computes which records and enums the backend can render, jointly.
    ///
    /// Records and enums can reference each other in either direction:
    /// a record field may be a data enum, and a data-enum variant field
    /// may be a record. Neither set can be computed independently, so
    /// both grow together in one fixed-point loop — each iteration tries
    /// to admit every not-yet-supported record and every not-yet-supported
    /// data enum against the current state of both sets, terminating when
    /// a pass produces no new admissions. C-style enums have no payload,
    /// so any whose repr is a legal C# enum backing type seed the enum set
    /// before iteration begins.
    ///
    /// Termination: every non-breaking iteration admits at least one new
    /// record or data enum; both catalogs are finite; admissions are
    /// monotonic. Mutually recursive types that require each other to be
    /// admitted first never make progress — the first pass finds neither
    /// admissible, no admissions are made, and the loop exits leaving
    /// both out of the supported sets.
    fn compute_supported_sets(ffi: &FfiContract) -> (HashSet<String>, HashSet<String>) {
        let mut enums: HashSet<String> = ffi
            .catalog
            .all_enums()
            .filter(|e| match &e.repr {
                EnumRepr::CStyle { tag_type, .. } => {
                    mappings::csharp_enum_backing_type(*tag_type).is_some()
                }
                EnumRepr::Data { .. } => false,
            })
            .map(|e| e.id.as_str().to_string())
            .collect();
        let mut records: HashSet<String> = HashSet::new();

        loop {
            let record_additions: Vec<String> = ffi
                .catalog
                .all_records()
                .filter(|r| !records.contains(r.id.as_str()))
                .filter(|r| {
                    r.fields
                        .iter()
                        .all(|f| Self::is_field_type_supported(&f.type_expr, &records, &enums))
                })
                .map(|r| r.id.as_str().to_string())
                .collect();
            let enum_additions: Vec<String> = ffi
                .catalog
                .all_enums()
                .filter(|e| matches!(e.repr, EnumRepr::Data { .. }))
                .filter(|e| !enums.contains(e.id.as_str()))
                .filter(|e| Self::enum_variant_fields_supported(e, &records, &enums))
                .map(|e| e.id.as_str().to_string())
                .collect();
            if record_additions.is_empty() && enum_additions.is_empty() {
                break;
            }
            records.extend(record_additions);
            enums.extend(enum_additions);
        }
        (records, enums)
    }

    fn enum_variant_fields_supported(
        enum_def: &EnumDef,
        records: &HashSet<String>,
        enums: &HashSet<String>,
    ) -> bool {
        let EnumRepr::Data { variants, .. } = &enum_def.repr else {
            return true;
        };
        variants.iter().all(|v| match &v.payload {
            VariantPayload::Unit => true,
            VariantPayload::Tuple(types) => types
                .iter()
                .all(|t| Self::is_field_type_supported(t, records, enums)),
            VariantPayload::Struct(fields) => fields
                .iter()
                .all(|f| Self::is_field_type_supported(&f.type_expr, records, enums)),
        })
    }

    fn is_field_type_supported(
        ty: &TypeExpr,
        records: &HashSet<String>,
        enums: &HashSet<String>,
    ) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => records.contains(id.as_str()),
            TypeExpr::Enum(id) => enums.contains(id.as_str()),
            // Vec as a field walks its inner type through the same
            // admission rules: any field-admissible type is also a valid
            // Vec element, and vice versa.
            TypeExpr::Vec(inner) => Self::is_field_type_supported(inner, records, enums),
            _ => false,
        }
    }

    /// Walk the contracts and produce a C# module plan.
    pub fn lower(&self) -> CSharpModule {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| naming::library_name(&self.ffi.package.name));

        let class_name = NamingConvention::class_name(&self.ffi.package.name);
        let namespace = NamingConvention::namespace(&self.ffi.package.name);
        let prefix = naming::ffi_prefix().to_string();

        let records: Vec<CSharpRecord> = self
            .ffi
            .catalog
            .all_records()
            .filter(|r| self.supported_records.contains(r.id.as_str()))
            .map(|r| self.lower_record(r))
            .collect();

        let enums: Vec<CSharpEnum> = self
            .ffi
            .catalog
            .all_enums()
            .filter_map(|e| self.lower_enum(e))
            .collect();

        let functions: Vec<CSharpFunction> = self
            .ffi
            .functions
            .iter()
            .filter_map(|f| self.lower_function(f))
            .collect();

        CSharpModule {
            namespace,
            class_name,
            lib_name,
            prefix,
            records,
            enums,
            functions,
        }
    }

    /// Converts a Rust FFI function definition into its C# representation,
    /// mapping Rust types to C# types and snake_case names to PascalCase.
    ///
    /// Returns `None` for functions whose signatures include types not yet
    /// supported by the C# backend.
    fn lower_function(&self, function: &FunctionDef) -> Option<CSharpFunction> {
        if function.is_async() {
            return None;
        }

        if !function.params.iter().all(|p| self.is_supported_param(p)) {
            return None;
        }

        let return_type = self.lower_return(&function.returns)?;
        let call = self.abi_call_for_function(function)?;
        let return_kind = self.return_kind(
            &function.returns,
            &return_type,
            call.returns.decode_ops.as_ref(),
        );

        let wire_writers = self.wire_writers_for_params(function)?;

        let params: Vec<CSharpParam> = function
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;

        Some(CSharpFunction {
            name: NamingConvention::method_name(function.id.as_str()),
            ffi_name: naming::function_ffi_name(function.id.as_str()).into_string(),
            params,
            return_type,
            return_kind,
            wire_writers,
        })
    }

    fn return_kind(
        &self,
        return_def: &ReturnDef,
        return_type: &CSharpType,
        decode_ops: Option<&ReadSeq>,
    ) -> CSharpReturnKind {
        if return_type.is_void() {
            return CSharpReturnKind::Void;
        }
        match return_def {
            ReturnDef::Value(TypeExpr::String) => CSharpReturnKind::WireDecodeString,
            ReturnDef::Value(TypeExpr::Record(id)) if !self.is_blittable_record(id) => {
                CSharpReturnKind::WireDecodeObject {
                    class_name: NamingConvention::class_name(id.as_str()),
                }
            }
            ReturnDef::Value(TypeExpr::Enum(id)) if self.is_data_enum(id) => {
                CSharpReturnKind::WireDecodeObject {
                    class_name: NamingConvention::class_name(id.as_str()),
                }
            }
            ReturnDef::Value(TypeExpr::Vec(inner)) => {
                let reader_call = match inner.as_ref() {
                    TypeExpr::Primitive(p) => emit::primitive_vec_reader_call(*p),
                    TypeExpr::Record(id) if self.is_blittable_record(id) => format!(
                        "ReadBlittableArray<{}>()",
                        NamingConvention::class_name(id.as_str())
                    ),
                    _ => {
                        let element_seq = Self::vec_element_read_seq(decode_ops)
                            .expect("encoded Vec return must carry decode_ops with a Vec ReadOp");
                        emit::vec_return_reader_call(inner, &element_seq)
                    }
                };
                CSharpReturnKind::WireDecodeArray { reader_call }
            }
            // Primitives, bools, blittable records, and C-style enums
            // are all direct: the CLR marshals them across P/Invoke
            // without any wrapper help.
            _ => CSharpReturnKind::Direct,
        }
    }

    /// Extract the per-element [`ReadSeq`] from a `ReadSeq` whose top op is
    /// a `Vec`. Used to render the inner decode expression that gets wrapped
    /// in `ReadEncodedArray<T>(r => ...)`. Primitive-element Vec returns
    /// never call into this; they short-circuit on the no-prefix fast path.
    fn vec_element_read_seq(decode_ops: Option<&ReadSeq>) -> Option<ReadSeq> {
        let decode = decode_ops?;
        match decode.ops.first()? {
            ReadOp::Vec { element, .. } => Some((**element).clone()),
            _ => None,
        }
    }

    fn is_data_enum(&self, id: &EnumId) -> bool {
        self.ffi
            .catalog
            .resolve_enum(id)
            .is_some_and(|e| matches!(e.repr, EnumRepr::Data { .. }))
    }

    /// Whether the record rides across P/Invoke by value with
    /// `[StructLayout(Sequential)]` and no wire encoding. Defers entirely
    /// to the IR's `is_blittable` flag, which admits all-primitive
    /// `#[repr(C)]` records only. A record that carries any user-defined
    /// type (record or enum, C-style or data) stays on the wire path —
    /// the Rust `#[export]` macro (see `boltffi_macros::data::analysis`)
    /// makes the same call, so the two sides agree on whether a given
    /// FFI symbol returns a value or an `FfiBuf`. Widening C#'s view
    /// without teaching the macro would produce a call-site / ABI
    /// mismatch and segfault at runtime.
    fn is_blittable_record(&self, id: &RecordId) -> bool {
        self.abi_record_for(id).is_some_and(|r| r.is_blittable)
    }

    fn is_supported_param(&self, param: &ParamDef) -> bool {
        param.passing == ParamPassing::Value && self.is_supported_type(&param.type_expr)
    }

    fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => self.supported_records.contains(id.as_str()),
            TypeExpr::Enum(id) => self.supported_enums.contains(id.as_str()),
            TypeExpr::Vec(inner) => self.is_supported_vec_element(inner),
            _ => false,
        }
    }

    /// Which element types the C# backend currently admits inside a
    /// top-level `Vec<_>` param or return. This is only the admission
    /// gate: primitives and blittable records can use the blittable
    /// path; strings, enums, non-blittable records, and nested vecs
    /// travel through the encoded wire form.
    fn is_supported_vec_element(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String => true,
            TypeExpr::Record(id) => self.supported_records.contains(id.as_str()),
            TypeExpr::Enum(id) => self.supported_enums.contains(id.as_str()),
            TypeExpr::Vec(inner) => self.is_supported_vec_element(inner),
            _ => false,
        }
    }

    /// Vec element types whose param form is a pinned `T[]` passed
    /// directly across P/Invoke. Primitives qualify because their C#
    /// mapping is a blittable value type; blittable records qualify
    /// because `[StructLayout(Sequential)]` guarantees the same byte
    /// layout as Rust's `#[repr(C)]`, so the CLR can hand the native
    /// side a pointer straight into the element buffer. C-style enums
    /// do NOT qualify even though their C# projection is a fixed-width
    /// value type: the Rust `#[export]` macro classifies them as
    /// `DataTypeCategory::Scalar` and routes `Vec<CStyleEnum>` through
    /// the wire-encoded path (only `Blittable` survives the macro's
    /// `supports_direct_vec` gate). Admitting them here would hand the
    /// native side a raw enum byte array where it expects a
    /// length-prefixed encoded array of I32 tags. Everything not
    /// listed rides the wire-encoded path. Tracked: issue #196.
    fn is_blittable_vec_element(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) => true,
            TypeExpr::Record(id) => self.is_blittable_record(id),
            _ => false,
        }
    }

    fn lower_param(
        &self,
        param: &ParamDef,
        wire_writers: &[CSharpWireWriter],
    ) -> Option<CSharpParam> {
        if param.passing != ParamPassing::Value {
            return None;
        }

        let csharp_type = self.lower_type(&param.type_expr)?;
        let kind = match &param.type_expr {
            TypeExpr::String => CSharpParamKind::Utf8Bytes,
            TypeExpr::Record(id) if !self.is_blittable_record(id) => {
                let writer = wire_writers
                    .iter()
                    .find(|w| w.param_name == param.name.as_str())?;
                CSharpParamKind::WireEncoded {
                    binding_name: writer.bytes_binding_name.clone(),
                }
            }
            TypeExpr::Enum(id) if self.is_data_enum(id) => {
                let writer = wire_writers
                    .iter()
                    .find(|w| w.param_name == param.name.as_str())?;
                CSharpParamKind::WireEncoded {
                    binding_name: writer.bytes_binding_name.clone(),
                }
            }
            TypeExpr::Vec(inner) if matches!(inner.as_ref(), TypeExpr::Primitive(_)) => {
                CSharpParamKind::DirectArray
            }
            TypeExpr::Vec(inner) if self.is_blittable_vec_element(inner) => {
                // Primitive arrays can use the CLR's built-in direct-array
                // path. Record arrays are less predictable once the element
                // type stops being blittable to the marshaller, e.g. because
                // it contains `bool` or `char`: P/Invoke may marshal through
                // a temporary native buffer rather than exposing the managed
                // array in place. `fixed` keeps this path zero-copy and
                // makes the ABI contract explicit: Rust reads the actual
                // managed element buffer, not a marshaled surrogate.
                let element_type = match inner.as_ref() {
                    TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
                    other => todo!(
                        "C# backend pinned-array param support not yet implemented for {other:?}"
                    ),
                };
                CSharpParamKind::PinnedArray { element_type }
            }
            TypeExpr::Vec(inner) if self.is_supported_vec_element(inner) => {
                // Vec<String> and Vec<Vec<_>> carry variable-width elements, so
                // the param travels wire-encoded rather than as a pinned T[].
                let writer = wire_writers
                    .iter()
                    .find(|w| w.param_name == param.name.as_str())?;
                CSharpParamKind::WireEncoded {
                    binding_name: writer.bytes_binding_name.clone(),
                }
            }
            // Primitives, bools, blittable records, and C-style enums
            // pass directly. The CLR marshals them across P/Invoke with
            // no extra setup.
            _ => CSharpParamKind::Direct,
        };

        Some(CSharpParam {
            name: NamingConvention::field_name(param.name.as_str()),
            csharp_type,
            kind,
        })
    }

    fn lower_return(&self, return_def: &ReturnDef) -> Option<CSharpType> {
        match return_def {
            ReturnDef::Void => Some(CSharpType::Void),
            ReturnDef::Value(type_expr) => self.lower_type(type_expr),
            ReturnDef::Result { .. } => None,
        }
    }

    fn lower_type(&self, type_expr: &TypeExpr) -> Option<CSharpType> {
        match type_expr {
            TypeExpr::Void => Some(CSharpType::Void),
            TypeExpr::Primitive(primitive) => Some(mappings::csharp_type(*primitive)),
            TypeExpr::String => Some(CSharpType::String),
            TypeExpr::Record(id) if self.supported_records.contains(id.as_str()) => Some(
                CSharpType::Record(NamingConvention::class_name(id.as_str())),
            ),
            TypeExpr::Enum(id) if self.supported_enums.contains(id.as_str()) => {
                let enum_def = self.ffi.catalog.resolve_enum(id)?;
                Some(mappings::csharp_enum_type(enum_def))
            }
            TypeExpr::Vec(inner) if self.is_supported_vec_element(inner) => {
                let inner_type = self.lower_type(inner)?;
                Some(CSharpType::Array(Box::new(inner_type)))
            }
            _ => None,
        }
    }

    fn lower_record(&self, record: &RecordDef) -> CSharpRecord {
        let class_name = NamingConvention::class_name(record.id.as_str());
        let fields = record
            .fields
            .iter()
            .map(|field| self.lower_record_field(&record.id, field))
            .collect();
        let is_blittable = self.is_blittable_record(&record.id);
        CSharpRecord {
            class_name,
            fields,
            is_blittable,
        }
    }

    /// Lowers a Rust enum definition into the C# plan, or returns `None`
    /// when the enum is not in the supported set.
    ///
    /// The two `EnumRepr` arms carry different numbering semantics:
    ///
    /// - **C-style enums** render as `public enum X : Backing`. Each C#
    ///   member's numeric value IS the Rust discriminant, because the
    ///   value crosses P/Invoke as its backing primitive and must be
    ///   bit-for-bit identical on both sides. Gapped or negative
    ///   discriminants must be preserved.
    /// - **Data enums** render as nested `sealed record` variants
    ///   dispatched by a wire tag. Tags come from the variant's ordinal
    ///   position (`EnumTagStrategy::OrdinalIndex`) — the Rust
    ///   discriminant is not part of the codec.
    fn lower_enum(&self, enum_def: &EnumDef) -> Option<CSharpEnum> {
        if !self.supported_enums.contains(enum_def.id.as_str()) {
            return None;
        }
        let class_name = NamingConvention::class_name(enum_def.id.as_str());
        let methods = self.lower_enum_methods(enum_def, &class_name);
        match &enum_def.repr {
            EnumRepr::CStyle { tag_type, variants } => {
                let lowered_variants = variants
                    .iter()
                    .enumerate()
                    .map(|(ordinal, variant)| CSharpEnumVariant {
                        name: NamingConvention::class_name(variant.name.as_str()),
                        tag: variant.discriminant as i32,
                        wire_tag: ordinal as i32,
                        fields: Vec::new(),
                    })
                    .collect();
                Some(CSharpEnum {
                    class_name,
                    kind: CSharpEnumKind::CStyle,
                    c_style_tag_type: Some(*tag_type),
                    variants: lowered_variants,
                    methods,
                })
            }
            EnumRepr::Data { .. } => {
                let abi_enum = self.abi.enums.iter().find(|e| e.id == enum_def.id)?;
                // Variant names become nested `sealed record` types; inside
                // the abstract record's body they shadow any module-level
                // type sharing a name. Collect the set so emit helpers can
                // qualify outer references (`Demo.Point.Decode(reader)`)
                // instead of letting them resolve to the shadowing variant.
                let shadowed_variant_names: HashSet<String> = abi_enum
                    .variants
                    .iter()
                    .map(|v| NamingConvention::class_name(v.name.as_str()))
                    .collect();
                let namespace = NamingConvention::namespace(&self.ffi.package.name);
                let scope = emit::ShadowScope {
                    shadowed: &shadowed_variant_names,
                    namespace: &namespace,
                };
                let variants = abi_enum
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(ordinal, variant)| {
                        self.lower_data_enum_variant(abi_enum, variant, ordinal, &scope)
                    })
                    .collect();
                Some(CSharpEnum {
                    class_name,
                    kind: CSharpEnumKind::Data,
                    c_style_tag_type: None,
                    variants,
                    methods,
                })
            }
        }
    }

    fn lower_data_enum_variant(
        &self,
        abi_enum: &AbiEnum,
        variant: &AbiEnumVariant,
        ordinal: usize,
        scope: &emit::ShadowScope,
    ) -> CSharpEnumVariant {
        let tag = abi_enum.resolve_codec_tag(ordinal, variant.discriminant) as i32;
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|f| self.lower_variant_field(f, scope))
                .collect(),
        };
        CSharpEnumVariant {
            name: NamingConvention::class_name(variant.name.as_str()),
            tag,
            // For data enums the public surface is a `sealed record`,
            // not a numbered enum, so `tag` and `wire_tag` converge —
            // both are the ordinal dispatch value used on the wire.
            wire_tag: tag,
            fields,
        }
    }

    /// Lowers one variant payload field. Write expressions are retargeted
    /// from `this.X` to `_v.X` because the template binds each variant in
    /// its switch arm (`case Circle _v: …`), not via `this`. Decode
    /// expressions pass through the shadowing scope so outer-type
    /// references survive being rendered inside the enum's body.
    fn lower_variant_field(
        &self,
        field: &AbiEnumField,
        scope: &emit::ShadowScope,
    ) -> CSharpRecordField {
        let prefixed = Self::prefix_write_seq(&field.encode, "_v");
        let csharp_type = self
            .lower_type(&field.type_expr)
            .expect("variant field type must be supported")
            .qualify_if_shadowed(scope.shadowed, scope.namespace);
        CSharpRecordField {
            name: NamingConvention::property_name(field.name.as_str()),
            csharp_type,
            wire_decode_expr: emit::emit_reader_read(&field.decode, Some(scope)),
            wire_size_expr: emit::emit_size_expr(&prefixed.size),
            wire_encode_expr: emit::emit_write_expr(&prefixed, "wire"),
        }
    }

    /// Walks an enum's `#[data(impl)]` constructors and methods and
    /// produces the corresponding C# method plans. Fallible constructors
    /// (`Result<Self, _>`), optional constructors (`Option<Self>`),
    /// methods that return `Result<_, _>`, async methods, and
    /// `&mut self` / `self` receivers are silently dropped — those
    /// shapes are served by later PRs on the roadmap, not by this one.
    fn lower_enum_methods(&self, enum_def: &EnumDef, enum_class_name: &str) -> Vec<CSharpMethod> {
        let is_data = matches!(enum_def.repr, EnumRepr::Data { .. });
        let mut methods = Vec::new();

        for (index, ctor) in enum_def.constructors.iter().enumerate() {
            if ctor.is_fallible() || ctor.is_optional() {
                continue;
            }
            let call_id = CallId::EnumConstructor {
                enum_id: enum_def.id.clone(),
                index,
            };
            let Some(call) = self.abi.calls.iter().find(|c| c.id == call_id) else {
                continue;
            };
            if let Some(method) = self.lower_enum_constructor(ctor, call, enum_class_name, is_data)
            {
                methods.push(method);
            }
        }

        for method_def in &enum_def.methods {
            if method_def.is_async() {
                continue;
            }
            if matches!(
                method_def.receiver,
                Receiver::RefMutSelf | Receiver::OwnedSelf
            ) {
                continue;
            }
            if matches!(method_def.returns, ReturnDef::Result { .. }) {
                continue;
            }
            let call_id = CallId::EnumMethod {
                enum_id: enum_def.id.clone(),
                method_id: method_def.id.clone(),
            };
            let Some(call) = self.abi.calls.iter().find(|c| c.id == call_id) else {
                continue;
            };
            if let Some(method) = self.lower_enum_method(method_def, call, enum_class_name, is_data)
            {
                methods.push(method);
            }
        }

        methods
    }

    fn lower_enum_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        enum_class_name: &str,
        owner_is_data: bool,
    ) -> Option<CSharpMethod> {
        let raw_name: &str = match ctor.name() {
            Some(id) => id.as_str(),
            None => "new",
        };
        let name = NamingConvention::method_name(raw_name);
        let return_type = if owner_is_data {
            CSharpType::DataEnum(enum_class_name.to_string())
        } else {
            CSharpType::CStyleEnum(enum_class_name.to_string())
        };
        let return_kind = if owner_is_data {
            CSharpReturnKind::WireDecodeObject {
                class_name: enum_class_name.to_string(),
            }
        } else {
            CSharpReturnKind::Direct
        };
        let wire_writers: Vec<CSharpWireWriter> = call
            .params
            .iter()
            .filter_map(|p| self.wire_writer_for_param(p))
            .collect();
        let param_defs = ctor.params();
        let params: Vec<CSharpParam> = param_defs
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        Some(CSharpMethod {
            native_method_name: format!("{enum_class_name}{name}"),
            name,
            ffi_name: call.symbol.as_str().to_string(),
            receiver: CSharpReceiver::Static,
            params,
            return_type,
            return_kind,
            wire_writers,
        })
    }

    fn lower_enum_method(
        &self,
        method_def: &MethodDef,
        call: &AbiCall,
        enum_class_name: &str,
        owner_is_data: bool,
    ) -> Option<CSharpMethod> {
        let name = NamingConvention::method_name(method_def.id.as_str());
        let return_type = match &method_def.returns {
            ReturnDef::Void => CSharpType::Void,
            ReturnDef::Value(type_expr) => self.lower_type(type_expr)?,
            ReturnDef::Result { .. } => return None,
        };
        let return_kind = self.return_kind(
            &method_def.returns,
            &return_type,
            call.returns.decode_ops.as_ref(),
        );

        let receiver = match method_def.receiver {
            Receiver::Static => CSharpReceiver::Static,
            Receiver::RefSelf | Receiver::RefMutSelf | Receiver::OwnedSelf if owner_is_data => {
                CSharpReceiver::InstanceNative
            }
            Receiver::RefSelf | Receiver::RefMutSelf | Receiver::OwnedSelf => {
                CSharpReceiver::InstanceExtension
            }
        };
        // Instance methods have a synthetic `self` prepended to the ABI
        // param list — skip it when building wire writers and mapping
        // back to the explicit IR params, which never include `self`.
        let explicit_abi_params = if matches!(receiver, CSharpReceiver::Static) {
            &call.params[..]
        } else {
            &call.params[1..]
        };
        let wire_writers: Vec<CSharpWireWriter> = explicit_abi_params
            .iter()
            .filter_map(|p| self.wire_writer_for_param(p))
            .collect();
        let params: Vec<CSharpParam> = method_def
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        Some(CSharpMethod {
            native_method_name: format!("{enum_class_name}{name}"),
            name,
            ffi_name: call.symbol.as_str().to_string(),
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers,
        })
    }

    fn lower_record_field(&self, record_id: &RecordId, field: &FieldDef) -> CSharpRecordField {
        let decode_seq = self
            .record_field_read_seq(record_id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(record_id, &field.name)
            .expect("record field encode ops");
        let csharp_type = self
            .lower_type(&field.type_expr)
            .expect("record field type must be supported");
        CSharpRecordField {
            name: NamingConvention::property_name(field.name.as_str()),
            csharp_type,
            wire_decode_expr: emit::emit_reader_read(&decode_seq, None),
            wire_size_expr: emit::emit_size_expr(&encode_seq.size),
            wire_encode_expr: emit::emit_write_expr(&encode_seq, "wire"),
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

    /// Build one [`CSharpWireWriter`] per record param, in param order.
    /// Returns `None` if the function's ABI call cannot be found (should
    /// not happen for validated contracts).
    fn wire_writers_for_params(&self, function: &FunctionDef) -> Option<Vec<CSharpWireWriter>> {
        let call = self.abi_call_for_function(function)?;
        Some(
            call.params
                .iter()
                .filter_map(|abi_param| self.wire_writer_for_param(abi_param))
                .collect(),
        )
    }

    fn wire_writer_for_param(&self, param: &AbiParam) -> Option<CSharpWireWriter> {
        let encode_ops = match &param.role {
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => encode_ops.clone(),
            _ => return None,
        };
        if !self.param_needs_wire_buffer(encode_ops.ops.first()?) {
            return None;
        }
        let param_name = param.name.as_str().to_string();
        let binding_name = format!("_wire_{}", param_name);
        let bytes_binding_name = format!("_{}Bytes", NamingConvention::field_name(&param_name));
        let encode_expr = emit::emit_write_expr(&encode_ops, &binding_name);
        Some(CSharpWireWriter {
            binding_name,
            bytes_binding_name,
            param_name,
            size_expr: emit::emit_size_expr(&encode_ops.size),
            encode_expr,
        })
    }

    /// Whether a param's encode op requires a `WireWriter` setup block
    /// before the native call.
    ///
    /// Primitives pass as value types, strings go through the UTF-8 byte
    /// path, raw bytes ride as `byte[]` directly. Blittable records and
    /// C-style enums also pass by value. Variable-width payloads
    /// (non-blittable records, data enums, vecs) need a length-prefixed
    /// buffer serialized up front.
    fn param_needs_wire_buffer(&self, op: &WriteOp) -> bool {
        match op {
            WriteOp::Primitive { .. } | WriteOp::String { .. } | WriteOp::Bytes { .. } => false,
            WriteOp::Record { id, .. } => !self.is_blittable_record(id),
            WriteOp::Enum {
                layout: EnumLayout::Data { .. },
                ..
            } => true,
            WriteOp::Enum { .. } => false,
            WriteOp::Vec {
                layout: VecLayout::Blittable { .. },
                ..
            } => false,
            WriteOp::Vec {
                layout: VecLayout::Encoded,
                ..
            } => true,
            WriteOp::Option { .. }
            | WriteOp::Result { .. }
            | WriteOp::Builtin { .. }
            | WriteOp::Custom { .. } => {
                todo!("C# backend has not yet implemented param support for {op:?}")
            }
        }
    }

    fn abi_call_for_function(&self, function: &FunctionDef) -> Option<&AbiCall> {
        self.abi.calls.iter().find(|call| match &call.id {
            CallId::Function(id) => id == &function.id,
            _ => false,
        })
    }

    /// Rewrites a [`WriteSeq`] so every reference to the encoded value's
    /// instance resolves to `{binding}` instead of the default `this`.
    /// Used for data enum variant fields, where the switch statement
    /// binds each variant as `case Circle _v:` and field references must
    /// go through `_v.Radius` rather than `this.Radius`.
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
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: Self::prefix_value(value, binding),
                element_type: element_type.clone(),
                // `element` references the per-iteration loop binding
                // (`item`), which belongs to the foreach the Vec writer
                // emits around the enclosing variant scope. Rewriting it
                // to `_v.item` would break the generated loop; leave the
                // element seq untouched.
                element: element.clone(),
                layout: layout.clone(),
            },
            other => panic!(
                "prefix_write_op: unsupported op for C# variant fields: {:?}",
                other
            ),
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

    fn prefix_size_expr(expr: &SizeExpr, binding: &str) -> SizeExpr {
        match expr {
            SizeExpr::Fixed(_) | SizeExpr::Runtime => expr.clone(),
            SizeExpr::StringLen(v) => SizeExpr::StringLen(Self::prefix_value(v, binding)),
            SizeExpr::BytesLen(v) => SizeExpr::BytesLen(Self::prefix_value(v, binding)),
            SizeExpr::ValueSize(v) => SizeExpr::ValueSize(Self::prefix_value(v, binding)),
            SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                owner: owner.clone(),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|p| Self::prefix_size_expr(p, binding))
                    .collect(),
            ),
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: Self::prefix_value(value, binding),
                // `inner` uses the per-element loop variable (`item`) the
                // encoded-array size lambda binds. The `_v` rewrite only
                // applies to the enclosing variant field reference.
                inner: inner.clone(),
                layout: layout.clone(),
            },
            other => panic!(
                "prefix_size_expr: unsupported expr for C# variant fields: {:?}",
                other
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{DataVariant, EnumDef};
    use crate::ir::types::PrimitiveType;

    fn data_enum(id: &str, variants: Vec<DataVariant>) -> EnumDef {
        EnumDef {
            id: EnumId::new(id),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants,
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn struct_variant(
        name: &str,
        discriminant: i128,
        fields: Vec<(&str, TypeExpr)>,
    ) -> DataVariant {
        DataVariant {
            name: name.into(),
            discriminant,
            payload: VariantPayload::Struct(
                fields
                    .into_iter()
                    .map(|(field_name, ty)| FieldDef {
                        name: field_name.into(),
                        type_expr: ty,
                        doc: None,
                        default: None,
                    })
                    .collect(),
            ),
            doc: None,
        }
    }

    fn record_with_one_field(id: &str, field_name: &str, type_expr: TypeExpr) -> RecordDef {
        RecordDef {
            id: RecordId::new(id),
            is_repr_c: false,
            is_error: false,
            fields: vec![FieldDef {
                name: field_name.into(),
                type_expr,
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    /// A record field that points at a data enum must still let the
    /// record qualify as supported. Records and data enums are computed
    /// in a joint fixed-point precisely so a record can wait a pass for
    /// the data enum it references, and vice versa.
    #[test]
    fn record_referencing_data_enum_is_admitted_jointly() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(data_enum(
            "shape",
            vec![struct_variant(
                "Circle",
                0,
                vec![("radius", TypeExpr::Primitive(PrimitiveType::F64))],
            )],
        ));
        contract.catalog.insert_record(record_with_one_field(
            "holder",
            "shape",
            TypeExpr::Enum(EnumId::new("shape")),
        ));

        let (records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            enums.contains("shape"),
            "expecting the data enum to be admitted first so the record can reference it",
        );
        assert!(
            records.contains("holder"),
            "expecting the record with a data-enum field to be admitted once the enum joins the set",
        );
    }

    /// A data enum whose variant carries another data enum must still be
    /// admitted to `supported_enums`. The fixed-point lets `outer` join
    /// on the iteration after `inner` is admitted, even though they're
    /// declared in a single pass.
    #[test]
    fn data_enum_referencing_another_data_enum_is_admitted() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(data_enum(
            "inner",
            vec![struct_variant(
                "Value",
                0,
                vec![("n", TypeExpr::Primitive(PrimitiveType::I32))],
            )],
        ));
        contract.catalog.insert_enum(data_enum(
            "outer",
            vec![struct_variant(
                "Wrap",
                0,
                vec![("inner", TypeExpr::Enum(EnumId::new("inner")))],
            )],
        ));

        let (_records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            enums.contains("inner"),
            "expecting the leaf data enum to be admitted",
        );
        assert!(
            enums.contains("outer"),
            "expecting the data enum referencing another data enum to join on a later fixed-point iteration",
        );
    }

    /// C# enums only support fixed-width integral backing types. A Rust
    /// `#[repr(usize)]` C-style enum therefore stays out of the supported
    /// set so the backend never tries to render an illegal `enum : nuint`.
    #[test]
    fn c_style_enum_with_usize_repr_is_not_admitted() {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("platform_status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::USize,
                variants: vec![crate::ir::definitions::CStyleVariant {
                    name: "Ready".into(),
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

        let (_records, enums) = CSharpLowerer::compute_supported_sets(&contract);

        assert!(
            !enums.contains("platform_status"),
            "expecting repr(usize) C-style enums to stay unsupported until the backend has a legal C# projection",
        );
    }
}
