use std::collections::HashSet;

use crate::ir::abi::{AbiCall, AbiEnum, AbiEnumField, AbiEnumPayload, AbiEnumVariant, CallId};
use crate::ir::definitions::{ConstructorDef, EnumDef, EnumRepr, MethodDef, Receiver};

use super::super::ast::{
    CSharpClassName, CSharpComment, CSharpEnumUnderlyingType, CSharpExpression, CSharpIdentity,
    CSharpLocalName, CSharpMethodName, CSharpType,
};
use super::super::plan::{
    CSharpEnumKind, CSharpEnumPlan, CSharpEnumVariantPlan, CSharpFieldPlan, CSharpMethodPlan,
    CSharpParamPlan, CSharpReceiver, CSharpReturnKind,
};
use super::lowerer::CSharpLowerer;
use super::wire_writers::self_wire_writer;
use super::{decode, encode, size};

impl<'a> CSharpLowerer<'a> {
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
    ///   position (`EnumTagStrategy::OrdinalIndex`). The Rust
    ///   discriminant is not part of the codec.
    pub(super) fn lower_enum(&self, enum_def: &EnumDef) -> Option<CSharpEnumPlan> {
        if !self.supported_enums.contains(&enum_def.id) {
            return None;
        }
        let class_name: CSharpClassName = (&enum_def.id).into();
        let wire_class_name = CSharpClassName::wire_helper(&class_name);
        // Variant names become nested `sealed record` types; inside the
        // abstract record's body they shadow any module-level type sharing
        // a name. Collect the set so emit helpers can qualify outer
        // references (`Demo.Point.Decode(reader)`) instead of letting them
        // resolve to the shadowing variant. Only data enums introduce
        // a nested body where shadowing applies.
        let abi_enum_for_data = match &enum_def.repr {
            EnumRepr::Data { .. } => self.abi.enums.iter().find(|e| e.id == enum_def.id),
            _ => None,
        };
        let shadowed_variant_names: HashSet<CSharpClassName> = abi_enum_for_data
            .map(|abi_enum| abi_enum.variants.iter().map(|v| (&v.name).into()).collect())
            .unwrap_or_default();
        let method_shadowed = abi_enum_for_data.map(|_| &shadowed_variant_names);
        let methods = self.lower_enum_methods(enum_def, &class_name, method_shadowed);
        let methods_class_name = if methods.is_empty() {
            None
        } else {
            Some(CSharpClassName::methods_companion(&class_name))
        };
        match &enum_def.repr {
            EnumRepr::CStyle { tag_type, variants } => {
                let lowered_variants = variants
                    .iter()
                    .enumerate()
                    .map(|(ordinal, variant)| CSharpEnumVariantPlan {
                        summary_doc: CSharpComment::from_str_option(variant.doc.as_deref()),
                        name: (&variant.name).into(),
                        tag: variant.discriminant as i32,
                        wire_tag: ordinal as i32,
                        fields: Vec::new(),
                    })
                    .collect();
                Some(CSharpEnumPlan {
                    summary_doc: CSharpComment::from_str_option(enum_def.doc.as_deref()),
                    class_name,
                    wire_class_name,
                    methods_class_name,
                    kind: CSharpEnumKind::CStyle,
                    underlying_type: Some(
                        CSharpEnumUnderlyingType::for_primitive(*tag_type)
                            .expect("supported-set filter admits only legal underlying types"),
                    ),
                    variants: lowered_variants,
                    methods,
                    is_error: enum_def.is_error,
                })
            }
            EnumRepr::Data { .. } => {
                let abi_enum = abi_enum_for_data?;
                // Share one encode/size context across all variants of
                // this enum because `WireEncodedSize` and `WireEncodeTo`
                // render all variant fields inside one method body
                // (via a switch statement). A separate decode context
                // keeps decode rendering independent. `Decode` builds
                // each variant in its own constructor call so no
                // pattern-binding leakage happens across variants.
                let mut size_locals = size::SizeLocalCounters::default();
                let mut encode_locals = encode::EncodeLocalCounters::default();
                let mut decode_locals = decode::DecodeLocalCounters::default();
                let variant_docs = enum_def.variant_docs();
                let variants = abi_enum
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(ordinal, variant)| {
                        self.lower_data_enum_variant(
                            abi_enum,
                            variant,
                            variant_docs.get(ordinal).cloned().flatten(),
                            ordinal,
                            &shadowed_variant_names,
                            &mut size_locals,
                            &mut encode_locals,
                            &mut decode_locals,
                        )
                    })
                    .collect();
                Some(CSharpEnumPlan {
                    summary_doc: CSharpComment::from_str_option(enum_def.doc.as_deref()),
                    class_name,
                    wire_class_name,
                    methods_class_name,
                    kind: CSharpEnumKind::Data,
                    underlying_type: None,
                    variants,
                    methods,
                    is_error: enum_def.is_error,
                })
            }
        }
    }

    /// Lowers one variant of a data enum, including its codec tag (resolved
    /// via `EnumTagStrategy`) and any payload fields.
    #[allow(clippy::too_many_arguments)]
    fn lower_data_enum_variant(
        &self,
        abi_enum: &AbiEnum,
        variant: &AbiEnumVariant,
        doc: Option<String>,
        ordinal: usize,
        shadowed: &HashSet<CSharpClassName>,
        size_locals: &mut size::SizeLocalCounters,
        encode_locals: &mut encode::EncodeLocalCounters,
        decode_locals: &mut decode::DecodeLocalCounters,
    ) -> CSharpEnumVariantPlan {
        let tag = abi_enum.resolve_codec_tag(ordinal, variant.discriminant) as i32;
        let fields = match &variant.payload {
            AbiEnumPayload::Unit => Vec::new(),
            AbiEnumPayload::Tuple(fields) | AbiEnumPayload::Struct(fields) => fields
                .iter()
                .map(|f| {
                    self.lower_variant_field(f, shadowed, size_locals, encode_locals, decode_locals)
                })
                .collect(),
        };
        CSharpEnumVariantPlan {
            summary_doc: CSharpComment::from_str_option(doc.as_deref()),
            name: (&variant.name).into(),
            tag,
            // For data enums the public surface is a `sealed record`,
            // not a numbered enum, so `tag` and `wire_tag` converge:
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
        shadowed: &HashSet<CSharpClassName>,
        size_locals: &mut size::SizeLocalCounters,
        encode_locals: &mut encode::EncodeLocalCounters,
        decode_locals: &mut decode::DecodeLocalCounters,
    ) -> CSharpFieldPlan {
        let prefixed = Self::prefix_write_seq(&field.encode, "_v");
        let csharp_type = self
            .lower_type(&field.type_expr)
            .expect("variant field type must be supported")
            .qualify_if_shadowed(shadowed, &self.namespace);
        CSharpFieldPlan {
            // Variant payload field docs are dropped by the ABI, so we
            // can't recover them here without a wider refactor; leave
            // empty for now.
            summary_doc: None,
            name: (&field.name).into(),
            csharp_type,
            wire_decode_expr: decode::lower_decode_expr(
                &field.decode,
                &CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("reader"))),
                Some(shadowed),
                &self.namespace,
                decode_locals,
            ),
            wire_size_expr: size::lower_size_expr(
                &prefixed.size,
                &super::value::Renames::new(),
                size_locals,
            ),
            wire_encode_stmts: encode::lower_encode_expr(
                &prefixed,
                &CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("wire"))),
                &super::value::Renames::new(),
                encode_locals,
            ),
        }
    }

    /// Walks an enum's `#[data(impl)]` constructors and methods and
    /// produces the corresponding [`CSharpMethodPlan`]s. Fallible
    /// constructors (`Result<Self, _>`), optional constructors
    /// (`Option<Self>`), methods that return `Result<_, _>`, async methods,
    /// and `&mut self` / `self` receivers are dropped silently; the C#
    /// backend doesn't model them yet.
    fn lower_enum_methods(
        &self,
        enum_def: &EnumDef,
        enum_class_name: &CSharpClassName,
        shadowed: Option<&HashSet<CSharpClassName>>,
    ) -> Vec<CSharpMethodPlan> {
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
            let call_id = CallId::EnumMethod {
                enum_id: enum_def.id.clone(),
                method_id: method_def.id.clone(),
            };
            let Some(call) = self.abi.calls.iter().find(|c| c.id == call_id) else {
                continue;
            };
            if let Some(method) =
                self.lower_enum_method(method_def, call, enum_class_name, is_data, shadowed)
            {
                methods.push(method);
            }
        }

        methods
    }

    /// Lowers a `#[data(impl)]` constructor into a static factory method
    /// on the enum's container.
    fn lower_enum_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        enum_class_name: &CSharpClassName,
        owner_is_data: bool,
    ) -> Option<CSharpMethodPlan> {
        let raw_name: &str = match ctor.name() {
            Some(id) => id.as_str(),
            None => "new",
        };
        let name = CSharpMethodName::from_source(raw_name);
        let return_type = if owner_is_data {
            CSharpType::DataEnum(enum_class_name.clone().into())
        } else {
            CSharpType::CStyleEnum(enum_class_name.clone().into())
        };
        let return_kind = if owner_is_data {
            CSharpReturnKind::WireDecodeObject {
                class_name: enum_class_name.clone(),
            }
        } else {
            CSharpReturnKind::Direct
        };
        let mut ctor_size_locals = size::SizeLocalCounters::default();
        let mut ctor_encode_locals = encode::EncodeLocalCounters::default();
        let wire_writers: Vec<_> = call
            .params
            .iter()
            .filter_map(|p| {
                self.wire_writer_for_param(p, &mut ctor_size_locals, &mut ctor_encode_locals)
            })
            .collect();
        let param_defs = ctor.params();
        let params: Vec<CSharpParamPlan> = param_defs
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        Some(CSharpMethodPlan {
            summary_doc: CSharpComment::from_str_option(ctor.doc()),
            native_method_name: CSharpMethodName::native_for_owner(enum_class_name, &name),
            name,
            ffi_name: (&call.symbol).into(),
            receiver: CSharpReceiver::Static,
            params,
            return_type,
            return_kind,
            wire_writers,
            owner_is_blittable: false,
        })
    }

    /// Lowers a `#[data(impl)]` method, mapping the receiver to one of
    /// [`CSharpReceiver::Static`], [`CSharpReceiver::InstanceNative`] (data
    /// enums), or [`CSharpReceiver::InstanceExtension`] (C-style enums).
    fn lower_enum_method(
        &self,
        method_def: &MethodDef,
        call: &AbiCall,
        enum_class_name: &CSharpClassName,
        owner_is_data: bool,
        shadowed: Option<&HashSet<CSharpClassName>>,
    ) -> Option<CSharpMethodPlan> {
        let name: CSharpMethodName = (&method_def.id).into();
        let return_type = self
            .lower_return(&method_def.returns)?
            .qualify_if_shadowed_opt(shadowed, &self.namespace);
        let return_kind = self.return_kind(
            &method_def.returns,
            &return_type,
            call.returns.decode_ops.as_ref(),
            shadowed,
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
        // param list; skip it when building wire writers and mapping
        // back to the explicit IR params, which never include `self`.
        let explicit_abi_params = if matches!(receiver, CSharpReceiver::Static) {
            &call.params[..]
        } else {
            &call.params[1..]
        };
        let mut method_size_locals = size::SizeLocalCounters::default();
        let mut method_encode_locals = encode::EncodeLocalCounters::default();
        let mut wire_writers: Vec<_> = Vec::new();
        if matches!(receiver, CSharpReceiver::InstanceNative) {
            wire_writers.push(self_wire_writer());
        }
        wire_writers.extend(explicit_abi_params.iter().filter_map(|p| {
            self.wire_writer_for_param(p, &mut method_size_locals, &mut method_encode_locals)
        }));
        let params: Vec<CSharpParamPlan> = method_def
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        Some(CSharpMethodPlan {
            summary_doc: CSharpComment::from_str_option(method_def.doc.as_deref()),
            native_method_name: CSharpMethodName::native_for_owner(enum_class_name, &name),
            name,
            ffi_name: (&call.symbol).into(),
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers,
            owner_is_blittable: false,
        })
    }
}
