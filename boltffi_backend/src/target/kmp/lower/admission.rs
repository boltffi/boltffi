//! Capability admission for KMP plans.

use std::collections::{BTreeMap, BTreeSet};

use boltffi_binding::{
    Bindings, CallableDecl, CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl,
    CustomTypeDecl, CustomTypeId, DataVariantDecl, DataVariantPayload, Decl, DeclarationRef,
    DirectValueType, EncodedFieldDecl, EnumDecl, EnumId, ErrorChannel, ErrorDecl, ExecutionDecl,
    ExportedMethodDecl, FunctionDecl, IncomingParam, InitializerDecl, Native, NativeSymbol,
    ParamPlan, Receive, RecordDecl, RecordId, ReturnPlan, RustBody, TypeRef,
};

use crate::core::DeclarationLabel;

use super::super::plan::{KmpCapability, KmpCapabilitySet, KmpPlatform, KmpSupportApi};

/// Admission engine for one selected KMP platform matrix.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct KmpAdmission<'bindings> {
    selected_platforms: Vec<KmpPlatform>,
    bindings: &'bindings Bindings<Native>,
    records: BTreeMap<RecordId, &'bindings RecordDecl<Native>>,
    enumerations: BTreeMap<EnumId, &'bindings EnumDecl<Native>>,
    custom_types: BTreeMap<CustomTypeId, &'bindings CustomTypeDecl>,
}

#[derive(Default)]
struct TypeCapabilityStack {
    custom_types: BTreeSet<CustomTypeId>,
    records: BTreeSet<RecordId>,
    enums: BTreeSet<EnumId>,
}

impl TypeCapabilityStack {
    fn new() -> Self {
        Self::default()
    }
}

impl<'bindings> KmpAdmission<'bindings> {
    /// Creates admission for the selected KMP platform matrix and binding graph.
    pub fn for_bindings(
        selected_platforms: Vec<KmpPlatform>,
        bindings: &'bindings Bindings<Native>,
    ) -> Self {
        Self {
            selected_platforms,
            bindings,
            records: record_index(bindings),
            enumerations: enum_index(bindings),
            custom_types: custom_type_index(bindings),
        }
    }

    /// Evaluates all declarations in a binding contract.
    pub fn evaluate(&self) -> KmpAdmissionReport {
        self.bindings
            .decls()
            .iter()
            .fold(KmpAdmissionReport::new(), |mut report, decl| {
                self.evaluate_declaration(DeclarationRef::from(decl))
                    .into_iter()
                    .for_each(|record| report.push(record));
                report
            })
    }

    /// Evaluates one declaration and any member APIs it owns.
    pub fn evaluate_decl(&self, decl: &Decl<Native>) -> Vec<KmpAdmissionRecord> {
        self.evaluate_declaration(DeclarationRef::from(decl))
    }

    /// Evaluates one declaration view and any member APIs it owns.
    pub fn evaluate_declaration(
        &self,
        declaration: DeclarationRef<'_, Native>,
    ) -> Vec<KmpAdmissionRecord> {
        match declaration {
            DeclarationRef::Record(record) => self.record(record),
            DeclarationRef::Enum(enumeration) => self.enumeration(enumeration),
            DeclarationRef::Function(function) => self.function(function),
            DeclarationRef::Class(class) => self.class(class),
            DeclarationRef::Callback(callback) => self.callback(callback),
            DeclarationRef::Stream(stream) => {
                let label = DeclarationLabel::from_ref(DeclarationRef::Stream(stream));
                vec![self.rejected(
                    label.kind(),
                    label.name(),
                    KmpCapabilitySet::from_iter([KmpCapability::Streams]),
                )]
            }
            DeclarationRef::Constant(constant) => self.constant(constant),
            DeclarationRef::CustomType(custom_type) => {
                vec![self.admit(
                    "custom type",
                    custom_type.name().as_path_string(),
                    union(
                        KmpCapabilitySet::from_iter([KmpCapability::CustomTypes]),
                        self.type_ref_capability_set(custom_type.representation()),
                    ),
                )]
            }
        }
    }

    /// Evaluates one declaration as a single API for host render coverage.
    pub fn evaluate_declaration_only(
        &self,
        decl: DeclarationRef<'_, Native>,
    ) -> KmpAdmissionRecord {
        let label = DeclarationLabel::from_ref(decl);
        let required = match decl {
            DeclarationRef::Record(record) => self.record_declaration_capability_set(record),
            DeclarationRef::Enum(enumeration) => self.enum_declaration_capability_set(enumeration),
            DeclarationRef::Function(function) => self.callable_capabilities(function.callable()),
            DeclarationRef::Class(_) => KmpCapabilitySet::from_iter([KmpCapability::Classes]),
            DeclarationRef::Callback(_) => KmpCapabilitySet::from_iter([KmpCapability::Callbacks]),
            DeclarationRef::Stream(_) => KmpCapabilitySet::from_iter([KmpCapability::Streams]),
            DeclarationRef::Constant(constant) => self.constant_capability_set(constant),
            DeclarationRef::CustomType(custom_type) => union(
                KmpCapabilitySet::from_iter([KmpCapability::CustomTypes]),
                self.type_ref_capability_set(custom_type.representation()),
            ),
        };
        self.admit(label.kind(), label.name(), required)
    }

    fn record(&self, record: &RecordDecl<Native>) -> Vec<KmpAdmissionRecord> {
        let owner_record = self.evaluate_declaration_only(DeclarationRef::Record(record));
        let mut records = vec![owner_record.clone()];
        records.extend(self.admit_owned_members(
            record.name(),
            record.initializers(),
            record.methods(),
            "record initializer",
            "record method",
            &owner_record,
        ));
        records
    }

    fn enumeration(&self, enumeration: &EnumDecl<Native>) -> Vec<KmpAdmissionRecord> {
        let owner_record = self.evaluate_declaration_only(DeclarationRef::Enum(enumeration));
        let mut records = vec![owner_record.clone()];
        records.extend(self.admit_owned_members(
            enumeration.name(),
            enumeration.initializers(),
            enumeration.methods(),
            "enum initializer",
            "enum method",
            &owner_record,
        ));
        records
    }

    fn function(&self, function: &FunctionDecl<Native>) -> Vec<KmpAdmissionRecord> {
        vec![self.admit_callable(
            "function",
            function.name().as_path_string(),
            function.callable(),
        )]
    }

    fn class(&self, class: &ClassDecl<Native>) -> Vec<KmpAdmissionRecord> {
        let mut records = vec![self.rejected(
            "class",
            class.name().as_path_string(),
            KmpCapabilitySet::from_iter([KmpCapability::Classes]),
        )];
        records.extend(class.initializers().iter().map(|initializer| {
            self.rejected(
                "class initializer",
                member_name(class.name(), initializer.name()),
                union(
                    KmpCapabilitySet::from_iter([KmpCapability::Classes]),
                    self.callable_capabilities(initializer.callable()),
                ),
            )
        }));
        records.extend(class.methods().iter().map(|method| {
            self.rejected(
                "class method",
                member_name(class.name(), method.name()),
                union(
                    KmpCapabilitySet::from_iter([KmpCapability::Classes]),
                    self.callable_capabilities(method.callable()),
                ),
            )
        }));
        records
    }

    fn callback(&self, callback: &CallbackDecl<Native>) -> Vec<KmpAdmissionRecord> {
        vec![self.rejected(
            "callback",
            callback.name().as_path_string(),
            KmpCapabilitySet::from_iter([KmpCapability::Callbacks]),
        )]
    }

    fn constant(&self, constant: &ConstantDecl<Native>) -> Vec<KmpAdmissionRecord> {
        vec![self.admit(
            "constant",
            constant.name().as_path_string(),
            self.constant_capability_set(constant),
        )]
    }

    fn admit_callable(
        &self,
        kind: &'static str,
        name: String,
        callable: &CallableDecl<Native, RustBody>,
    ) -> KmpAdmissionRecord {
        self.admit(kind, name, self.callable_capabilities(callable))
    }

    fn admit_owned_members(
        &self,
        owner: &boltffi_binding::CanonicalName,
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        initializer_kind: &'static str,
        method_kind: &'static str,
        owner_record: &KmpAdmissionRecord,
    ) -> Vec<KmpAdmissionRecord> {
        initializers
            .iter()
            .map(|initializer| {
                self.admit_member_callable(
                    initializer_kind,
                    member_name(owner, initializer.name()),
                    initializer.callable(),
                    owner_record,
                )
            })
            .chain(methods.iter().map(|method| {
                self.admit_member_callable(
                    method_kind,
                    member_name(owner, method.name()),
                    method.callable(),
                    owner_record,
                )
            }))
            .collect()
    }

    fn admit_member_callable(
        &self,
        kind: &'static str,
        name: String,
        callable: &CallableDecl<Native, RustBody>,
        owner_record: &KmpAdmissionRecord,
    ) -> KmpAdmissionRecord {
        let required = union(
            owner_record.required_capabilities().clone(),
            self.callable_capabilities(callable),
        );
        if owner_record.is_admitted() {
            self.admit(kind, name, required)
        } else {
            self.rejected(kind, name, required)
        }
    }

    fn admit(
        &self,
        kind: &'static str,
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
    ) -> KmpAdmissionRecord {
        let name = name.into();
        match unsupported_reason(&self.selected_platforms, &required_capabilities) {
            Some(reason) => KmpAdmissionRecord::rejected(kind, name, required_capabilities, reason),
            None => KmpAdmissionRecord::admitted(kind, name, required_capabilities),
        }
    }

    fn rejected(
        &self,
        kind: &'static str,
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
    ) -> KmpAdmissionRecord {
        let reason = unsupported_reason(&self.selected_platforms, &required_capabilities)
            .unwrap_or_else(|| "unsupported by the KMP admission policy".to_owned());
        KmpAdmissionRecord::rejected(kind, name, required_capabilities, reason)
    }

    fn callable_capabilities(&self, callable: &CallableDecl<Native, RustBody>) -> KmpCapabilitySet {
        let execution = match callable.execution() {
            ExecutionDecl::Synchronous(_) => KmpCapability::SyncCallables,
            ExecutionDecl::Asynchronous(_) => KmpCapability::AsyncCallables,
            _ => KmpCapability::UnknownBindingShapes,
        };
        let capabilities = std::iter::once(execution)
            .chain(self.receiver_capabilities(callable.receiver()))
            .chain(
                callable
                    .params()
                    .iter()
                    .flat_map(|param| self.param_capabilities(param.payload()).into_iter()),
            )
            .chain(self.return_capabilities(callable.returns().plan()))
            .chain(self.error_capabilities(callable.error()));
        KmpCapabilitySet::from_iter(capabilities)
    }

    fn receiver_capabilities(&self, receiver: Option<Receive>) -> Vec<KmpCapability> {
        match receiver {
            Some(Receive::ByMutRef) => vec![KmpCapability::MutatingReceivers],
            Some(Receive::ByValue | Receive::ByRef) | None => Vec::new(),
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn constant_capability_set(&self, constant: &ConstantDecl<Native>) -> KmpCapabilitySet {
        let required = KmpCapabilitySet::from_iter([KmpCapability::Constants]);
        match constant.value() {
            ConstantValueDecl::Inline { ty, .. } => {
                union(required, self.type_ref_capability_set(ty))
            }
            ConstantValueDecl::Accessor { callable, .. } => {
                union(required, self.callable_capabilities(callable))
            }
            _ => union(
                required,
                KmpCapabilitySet::from_iter([KmpCapability::UnknownBindingShapes]),
            ),
        }
    }

    fn param_capabilities(&self, payload: &IncomingParam<Native>) -> Vec<KmpCapability> {
        match payload {
            IncomingParam::Value(plan) => self.param_plan_capabilities(plan),
            IncomingParam::Closure(_) => vec![KmpCapability::Callbacks],
        }
    }

    fn param_plan_capabilities<D>(&self, plan: &ParamPlan<Native, D>) -> Vec<KmpCapability>
    where
        D: boltffi_binding::Direction,
    {
        match plan {
            ParamPlan::Direct { ty, .. } => self.direct_value_capabilities(ty),
            ParamPlan::Encoded { ty, .. } => self.type_ref_capabilities(ty),
            ParamPlan::Handle { target, .. } => handle_capability(target),
            ParamPlan::ScalarOption { .. } => Vec::new(),
            ParamPlan::DirectVec { element } => match element {
                boltffi_binding::DirectVectorElementType::Primitive(_) => Vec::new(),
                boltffi_binding::DirectVectorElementType::Record(_) => {
                    vec![KmpCapability::DirectRecords]
                }
                _ => vec![KmpCapability::UnknownBindingShapes],
            },
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn return_capabilities(
        &self,
        plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    ) -> Vec<KmpCapability> {
        match plan {
            ReturnPlan::Void => Vec::new(),
            ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
                self.direct_value_capabilities(ty)
            }
            ReturnPlan::EncodedViaReturnSlot { ty, .. }
            | ReturnPlan::EncodedViaOutPointer { ty, .. } => self.type_ref_capabilities(ty),
            ReturnPlan::HandleViaReturnSlot { target, .. }
            | ReturnPlan::HandleViaOutPointer { target, .. } => handle_capability(target),
            ReturnPlan::ScalarOptionViaReturnSlot { .. } => Vec::new(),
            ReturnPlan::DirectVecViaReturnSlot { element } => match element {
                boltffi_binding::DirectVectorElementType::Primitive(_) => Vec::new(),
                boltffi_binding::DirectVectorElementType::Record(_) => {
                    vec![KmpCapability::DirectRecords]
                }
                _ => vec![KmpCapability::UnknownBindingShapes],
            },
            ReturnPlan::ClosureViaOutPointer(_) => vec![KmpCapability::Callbacks],
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn error_capabilities(
        &self,
        error: &ErrorDecl<Native, boltffi_binding::OutOfRust>,
    ) -> Vec<KmpCapability> {
        match error.channel() {
            ErrorChannel::None | ErrorChannel::Status => Vec::new(),
            ErrorChannel::Encoded { ty, .. } => self.type_ref_capabilities(ty),
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn direct_value_capabilities(&self, ty: &DirectValueType) -> Vec<KmpCapability> {
        match ty {
            DirectValueType::Primitive(_) => Vec::new(),
            DirectValueType::Record(_) => vec![KmpCapability::DirectRecords],
            DirectValueType::Enum(_) => vec![KmpCapability::CStyleEnums],
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn type_ref_capability_set(&self, ty: &TypeRef) -> KmpCapabilitySet {
        KmpCapabilitySet::from_iter(self.type_ref_capabilities(ty))
    }

    fn type_ref_capabilities(&self, ty: &TypeRef) -> Vec<KmpCapability> {
        self.type_ref_capabilities_inner(ty, &mut TypeCapabilityStack::new())
    }

    fn type_ref_capabilities_inner(
        &self,
        ty: &TypeRef,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        match ty {
            TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes => Vec::new(),
            TypeRef::Record(id) => {
                let mut capabilities = vec![KmpCapability::EncodedRecords];
                if !visiting.records.insert(*id) {
                    return capabilities;
                }
                match self.record_declaration(*id) {
                    Some(record) => {
                        capabilities.extend(self.record_declaration_capabilities(record, visiting))
                    }
                    None => capabilities.push(KmpCapability::UnknownBindingShapes),
                }
                visiting.records.remove(id);
                capabilities
            }
            TypeRef::Enum(id) => {
                let mut capabilities = vec![KmpCapability::DataEnums];
                if !visiting.enums.insert(*id) {
                    return capabilities;
                }
                match self.enum_declaration(*id) {
                    Some(enumeration) => capabilities
                        .extend(self.enum_declaration_capabilities(enumeration, visiting)),
                    None => capabilities.push(KmpCapability::UnknownBindingShapes),
                }
                visiting.enums.remove(id);
                capabilities
            }
            TypeRef::Class(_) => vec![KmpCapability::Classes],
            TypeRef::Callback(_) => vec![KmpCapability::Callbacks],
            TypeRef::Custom(id) => {
                let mut capabilities = vec![KmpCapability::CustomTypes];
                if !visiting.custom_types.insert(*id) {
                    capabilities.push(KmpCapability::UnknownBindingShapes);
                    return capabilities;
                }
                match self.custom_type(*id) {
                    Some(custom_type) => capabilities.extend(
                        self.type_ref_capabilities_inner(custom_type.representation(), visiting),
                    ),
                    None => capabilities.push(KmpCapability::UnknownBindingShapes),
                }
                visiting.custom_types.remove(id);
                capabilities
            }
            TypeRef::Optional(inner) | TypeRef::Sequence(inner) => {
                self.type_ref_capabilities_inner(inner, visiting)
            }
            TypeRef::Result { ok, err } => self
                .type_ref_capabilities_inner(ok, visiting)
                .into_iter()
                .chain(self.type_ref_capabilities_inner(err, visiting))
                .collect(),
            TypeRef::Builtin(_) => vec![KmpCapability::UnknownBindingShapes],
            TypeRef::Tuple(items) => std::iter::once(KmpCapability::UnknownBindingShapes)
                .chain(
                    items
                        .iter()
                        .flat_map(|item| self.type_ref_capabilities_inner(item, visiting)),
                )
                .collect(),
            TypeRef::Map { key, value } => std::iter::once(KmpCapability::UnknownBindingShapes)
                .chain(self.type_ref_capabilities_inner(key, visiting))
                .chain(self.type_ref_capabilities_inner(value, visiting))
                .collect(),
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn record_declaration_capability_set(&self, record: &RecordDecl<Native>) -> KmpCapabilitySet {
        KmpCapabilitySet::from_iter(
            self.record_declaration_capabilities(record, &mut TypeCapabilityStack::new()),
        )
    }

    fn record_declaration_capabilities(
        &self,
        record: &RecordDecl<Native>,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        match record {
            RecordDecl::Direct(_) => vec![KmpCapability::DirectRecords],
            RecordDecl::Encoded(record) => {
                self.encoded_record_capabilities(record.fields().iter().collect(), visiting)
            }
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn enum_declaration_capability_set(&self, enumeration: &EnumDecl<Native>) -> KmpCapabilitySet {
        KmpCapabilitySet::from_iter(
            self.enum_declaration_capabilities(enumeration, &mut TypeCapabilityStack::new()),
        )
    }

    fn enum_declaration_capabilities(
        &self,
        enumeration: &EnumDecl<Native>,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        match enumeration {
            EnumDecl::CStyle(_) => vec![KmpCapability::CStyleEnums],
            EnumDecl::Data(enumeration) => {
                self.data_enum_capabilities(enumeration.variants().iter().collect(), visiting)
            }
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn encoded_record_capabilities(
        &self,
        fields: Vec<&EncodedFieldDecl>,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        std::iter::once(KmpCapability::EncodedRecords)
            .chain(
                fields
                    .into_iter()
                    .flat_map(|field| self.type_ref_capabilities_inner(field.ty(), visiting)),
            )
            .collect()
    }

    fn data_enum_capabilities(
        &self,
        variants: Vec<&DataVariantDecl>,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        std::iter::once(KmpCapability::DataEnums)
            .chain(variants.into_iter().flat_map(|variant| {
                self.data_variant_payload_capabilities_inner(variant.payload(), visiting)
            }))
            .collect()
    }

    fn data_variant_payload_capabilities_inner(
        &self,
        payload: &DataVariantPayload,
        visiting: &mut TypeCapabilityStack,
    ) -> Vec<KmpCapability> {
        match payload {
            DataVariantPayload::Unit => Vec::new(),
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields
                .iter()
                .flat_map(|field| self.type_ref_capabilities_inner(field.ty(), visiting))
                .collect(),
            _ => vec![KmpCapability::UnknownBindingShapes],
        }
    }

    fn record_declaration(&self, id: RecordId) -> Option<&RecordDecl<Native>> {
        self.records.get(&id).copied()
    }

    fn enum_declaration(&self, id: EnumId) -> Option<&EnumDecl<Native>> {
        self.enumerations.get(&id).copied()
    }

    fn custom_type(&self, id: CustomTypeId) -> Option<&CustomTypeDecl> {
        self.custom_types.get(&id).copied()
    }
}

/// Admission report for all APIs in a binding contract.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpAdmissionReport {
    records: Vec<KmpAdmissionRecord>,
}

impl KmpAdmissionReport {
    /// Creates an empty admission report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one admission record.
    pub fn push(&mut self, record: KmpAdmissionRecord) {
        self.records.push(record);
    }

    /// Returns all admission records.
    pub fn records(&self) -> &[KmpAdmissionRecord] {
        &self.records
    }

    /// Returns admitted API records.
    pub fn admitted(&self) -> Vec<&KmpAdmissionRecord> {
        self.records
            .iter()
            .filter(|record| record.is_admitted())
            .collect()
    }

    /// Returns rejected API records.
    pub fn rejected(&self) -> Vec<&KmpAdmissionRecord> {
        self.records
            .iter()
            .filter(|record| !record.is_admitted())
            .collect()
    }

    pub(crate) fn admitted_support_apis(&self) -> Vec<KmpSupportApi> {
        self.admitted()
            .into_iter()
            .map(|record| KmpSupportApi::admitted(record.kind, record.name.clone()))
            .collect()
    }

    pub(crate) fn rejected_support_apis(&self) -> Vec<KmpSupportApi> {
        self.rejected()
            .into_iter()
            .map(|record| {
                KmpSupportApi::rejected(
                    record.kind,
                    record.name.clone(),
                    record.reason.as_deref().unwrap_or("unsupported"),
                )
            })
            .collect()
    }
}

/// Admission outcome for one KMP API.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpAdmissionRecord {
    kind: &'static str,
    name: String,
    required_capabilities: KmpCapabilitySet,
    reason: Option<String>,
}

impl KmpAdmissionRecord {
    /// Creates an admitted API record.
    pub fn admitted(
        kind: &'static str,
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            required_capabilities,
            reason: None,
        }
    }

    /// Creates a rejected API record.
    pub fn rejected(
        kind: &'static str,
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            required_capabilities,
            reason: Some(reason.into()),
        }
    }

    /// Returns whether the API is admitted.
    pub fn is_admitted(&self) -> bool {
        self.reason.is_none()
    }

    /// Returns the API kind.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Returns the API display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the capabilities this API requires.
    pub const fn required_capabilities(&self) -> &KmpCapabilitySet {
        &self.required_capabilities
    }

    /// Returns the rejection reason, if any.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

fn handle_capability(target: &boltffi_binding::HandleTarget) -> Vec<KmpCapability> {
    match target {
        boltffi_binding::HandleTarget::Class(_) => vec![KmpCapability::Classes],
        boltffi_binding::HandleTarget::Callback(_) => vec![KmpCapability::Callbacks],
        boltffi_binding::HandleTarget::Stream(_) => vec![KmpCapability::Streams],
        _ => vec![KmpCapability::UnknownBindingShapes],
    }
}

fn unsupported_reason(
    platforms: &[KmpPlatform],
    required_capabilities: &KmpCapabilitySet,
) -> Option<String> {
    if platforms.is_empty() {
        return Some("no selected KMP platforms".to_owned());
    }

    let unsupported = platforms
        .iter()
        .flat_map(|platform| {
            let platform_capabilities = platform.capabilities();
            required_capabilities
                .iter()
                .filter(move |capability| !platform_capabilities.contains(*capability))
                .map(move |capability| format!("{} on {}", capability.label(), platform.label()))
        })
        .collect::<Vec<_>>();

    (!unsupported.is_empty()).then(|| format!("unsupported {}", unsupported.join(", ")))
}

fn union(left: KmpCapabilitySet, right: KmpCapabilitySet) -> KmpCapabilitySet {
    KmpCapabilitySet::from_iter(left.iter().chain(right.iter()))
}

fn record_index(bindings: &Bindings<Native>) -> BTreeMap<RecordId, &RecordDecl<Native>> {
    bindings
        .decls()
        .iter()
        .filter_map(|decl| match decl {
            Decl::Record(record) => Some((record.id(), record.as_ref())),
            _ => None,
        })
        .collect()
}

fn enum_index(bindings: &Bindings<Native>) -> BTreeMap<EnumId, &EnumDecl<Native>> {
    bindings
        .decls()
        .iter()
        .filter_map(|decl| match decl {
            Decl::Enum(enumeration) => Some((enumeration.id(), enumeration.as_ref())),
            _ => None,
        })
        .collect()
}

fn custom_type_index(bindings: &Bindings<Native>) -> BTreeMap<CustomTypeId, &CustomTypeDecl> {
    bindings
        .decls()
        .iter()
        .filter_map(|decl| match decl {
            Decl::CustomType(custom_type) => Some((custom_type.id(), custom_type.as_ref())),
            _ => None,
        })
        .collect()
}

fn member_name(
    owner: &boltffi_binding::CanonicalName,
    member: &boltffi_binding::CanonicalName,
) -> String {
    format!("{}::{}", owner.as_path_string(), member.as_path_string())
}
