//! KMP plan lowering and admission.

pub mod admission;

use std::{collections::BTreeSet, fmt};

use boltffi_binding::{
    Bindings, Decl, DeclarationRef, DirectValueType, ErrorChannel, ExecutionDecl, FunctionDecl,
    IncomingParam, Native, ParamPlan, Primitive, Receive, ReturnPlan,
};

use crate::core::DeclarationLabel;

use super::{
    names,
    plan::{
        KmpApiPlan, KmpCommonModule, KmpFunctionPlan, KmpJvmDelegateOutput, KmpModule,
        KmpParamPlan, KmpPlatform, KmpPlatformModule, KmpSupportMode, KmpSupportReport,
        KmpTypePlan,
    },
};

/// Options controlling KMP plan lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpLoweringOptions {
    selected_platforms: Vec<KmpPlatform>,
    support_mode: KmpSupportMode,
    jvm_delegate: Option<KmpJvmDelegateOutput>,
}

impl Default for KmpLoweringOptions {
    fn default() -> Self {
        Self {
            selected_platforms: KmpPlatform::default_selected(),
            support_mode: KmpSupportMode::Strict,
            jvm_delegate: None,
        }
    }
}

impl KmpLoweringOptions {
    /// Creates lowering options using the default JVM and Android platform set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the selected KMP platforms.
    pub fn selected_platforms(mut self, selected_platforms: impl Into<Vec<KmpPlatform>>) -> Self {
        self.selected_platforms = selected_platforms.into();
        self
    }

    /// Sets the support mode used for unsupported APIs.
    pub fn support_mode(mut self, support_mode: KmpSupportMode) -> Self {
        self.support_mode = support_mode;
        self
    }

    /// Sets JVM-family delegate output available for platform body emission.
    pub fn jvm_delegate(mut self, delegate: KmpJvmDelegateOutput) -> Self {
        self.jvm_delegate = Some(delegate);
        self
    }

    /// Returns the selected KMP platforms.
    pub fn platforms(&self) -> &[KmpPlatform] {
        &self.selected_platforms
    }

    /// Returns the support mode.
    pub const fn mode(&self) -> KmpSupportMode {
        self.support_mode
    }

    /// Returns JVM-family delegate output, if available.
    pub const fn jvm_delegate_output(&self) -> Option<&KmpJvmDelegateOutput> {
        self.jvm_delegate.as_ref()
    }
}

/// Lowers classified native bindings into a KMP module plan.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpLowerer {
    options: KmpLoweringOptions,
}

impl KmpLowerer {
    /// Creates a lowerer from options.
    pub fn new(options: KmpLoweringOptions) -> Self {
        Self { options }
    }

    /// Lowers bindings into a KMP module plan.
    pub fn lower(
        &self,
        bindings: &Bindings<Native>,
    ) -> std::result::Result<KmpModule, KmpLowerError> {
        self.lower_support_plan(self.support_plan(bindings))
    }

    pub(crate) fn support_plan(&self, bindings: &Bindings<Native>) -> KmpSupportPlan {
        let admission = admission::KmpAdmission::for_bindings(
            self.options.selected_platforms.clone(),
            bindings,
        );
        let mut admission_report = admission::KmpAdmissionReport::new();
        let mut admitted = Vec::new();
        let mut declarations = Vec::new();
        let mut function_signatures = BTreeSet::new();
        for decl in bindings.decls() {
            let label = DeclarationLabel::from_ref(DeclarationRef::from(decl));
            let mut declaration_records = Vec::new();
            for record in admission.evaluate_decl(decl) {
                let support_record = if record.is_admitted() {
                    match admitted_api_plan(
                        decl,
                        &record,
                        self.options.jvm_delegate_output(),
                        &mut function_signatures,
                    ) {
                        Ok(api) => {
                            admitted.push(api);
                            record
                        }
                        Err(reason) => admission::KmpAdmissionRecord::rejected(
                            record.kind(),
                            record.name(),
                            record.required_capabilities().clone(),
                            reason,
                        ),
                    }
                } else {
                    record
                };
                admission_report.push(support_record.clone());
                declaration_records.push(support_record);
            }
            declarations.push(KmpDeclarationSupport::new(label, declaration_records));
        }

        KmpSupportPlan::new(admitted, admission_report, declarations)
    }

    pub(crate) fn lower_support_plan(
        &self,
        support_plan: KmpSupportPlan,
    ) -> std::result::Result<KmpModule, KmpLowerError> {
        let support_report = support_plan
            .support_report(self.options.support_mode, &self.options.selected_platforms);

        if self.options.selected_platforms.is_empty() {
            return Err(KmpLowerError::InvalidPlatformMatrix {
                reason: "no selected KMP platforms".to_owned(),
                report: support_report,
            });
        }

        if self.options.support_mode == KmpSupportMode::Strict
            && !support_report.rejected_apis().is_empty()
        {
            return Err(KmpLowerError::UnsupportedApis {
                report: support_report,
            });
        }

        let admitted = support_plan.into_common_apis();
        let platforms = self
            .options
            .selected_platforms
            .iter()
            .map(|platform| KmpPlatformModule::new(*platform, platform.capabilities()))
            .collect();

        let mut module = KmpModule::new(KmpCommonModule::new(admitted), platforms, support_report);
        if let Some(delegate) = self.options.jvm_delegate.clone() {
            module = module.with_jvm_delegate(delegate);
        }
        Ok(module)
    }

    /// Returns the lowerer options.
    pub const fn options(&self) -> &KmpLoweringOptions {
        &self.options
    }
}

/// Failure while lowering a KMP module plan.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum KmpLowerError {
    /// Strict mode rejected at least one API.
    UnsupportedApis {
        /// Full support report describing admitted and rejected APIs.
        report: KmpSupportReport,
    },
    /// The selected platform matrix cannot produce a KMP module.
    InvalidPlatformMatrix {
        /// Human-readable platform matrix failure.
        reason: String,
        /// Support report built with the invalid matrix for diagnostics.
        report: KmpSupportReport,
    },
}

impl KmpLowerError {
    /// Converts this KMP lowering failure into a backend render error.
    pub(crate) fn into_backend_error(self) -> crate::Error {
        match self {
            Self::UnsupportedApis { .. } => crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "unsupported KMP APIs",
            },
            Self::InvalidPlatformMatrix { .. } => crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "invalid KMP platform matrix",
            },
        }
    }
}

impl fmt::Display for KmpLowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedApis { report } => {
                write!(
                    formatter,
                    "unsupported KMP APIs: {}",
                    summarize_report(report)
                )
            }
            Self::InvalidPlatformMatrix { reason, report } => {
                write!(
                    formatter,
                    "invalid KMP platform matrix: {reason}: {}",
                    summarize_report(report)
                )
            }
        }
    }
}

impl std::error::Error for KmpLowerError {}

fn summarize_report(report: &KmpSupportReport) -> String {
    report
        .rejected_apis()
        .iter()
        .take(4)
        .map(|api| match api.reason() {
            Some(reason) => format!("{} {} ({reason})", api.kind(), api.name()),
            None => format!("{} {}", api.kind(), api.name()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Lowers bindings with default strict JVM and Android options.
pub fn lower(bindings: &Bindings<Native>) -> std::result::Result<KmpModule, KmpLowerError> {
    KmpLowerer::new(KmpLoweringOptions::new()).lower(bindings)
}

/// Ordered KMP support decisions for one binding contract.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) struct KmpSupportPlan {
    common_apis: Vec<KmpApiPlan>,
    admission_report: admission::KmpAdmissionReport,
    declarations: Vec<KmpDeclarationSupport>,
}

impl KmpSupportPlan {
    fn new(
        common_apis: Vec<KmpApiPlan>,
        admission_report: admission::KmpAdmissionReport,
        declarations: Vec<KmpDeclarationSupport>,
    ) -> Self {
        Self {
            common_apis,
            admission_report,
            declarations,
        }
    }

    pub(crate) fn declarations(&self) -> &[KmpDeclarationSupport] {
        &self.declarations
    }

    pub(crate) fn has_rejections(&self) -> bool {
        self.admission_report
            .records()
            .iter()
            .any(|record| !record.is_admitted())
    }

    fn support_report(
        &self,
        mode: KmpSupportMode,
        selected_platforms: &[KmpPlatform],
    ) -> KmpSupportReport {
        KmpSupportReport::new(
            mode,
            selected_platforms.to_vec(),
            self.admission_report.admitted_support_apis(),
            self.admission_report.rejected_support_apis(),
        )
    }

    fn into_common_apis(self) -> Vec<KmpApiPlan> {
        self.common_apis
    }
}

/// KMP support decisions owned by one source declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) struct KmpDeclarationSupport {
    label: DeclarationLabel,
    records: Vec<admission::KmpAdmissionRecord>,
}

impl KmpDeclarationSupport {
    fn new(label: DeclarationLabel, records: Vec<admission::KmpAdmissionRecord>) -> Self {
        Self { label, records }
    }

    pub(crate) const fn label(&self) -> &DeclarationLabel {
        &self.label
    }

    pub(crate) fn records(&self) -> &[admission::KmpAdmissionRecord] {
        &self.records
    }
}

fn admitted_api_plan(
    decl: &Decl<Native>,
    record: &admission::KmpAdmissionRecord,
    jvm_delegate: Option<&KmpJvmDelegateOutput>,
    function_signatures: &mut BTreeSet<String>,
) -> std::result::Result<KmpApiPlan, String> {
    if !record.is_admitted() {
        return Err("KMP API was not admitted".to_string());
    }

    match (record.kind(), DeclarationRef::from(decl)) {
        ("function", DeclarationRef::Function(function)) => {
            let function_plan = lower_native_function_plan(function)?;
            if jvm_delegate.is_some_and(|delegate| delegate.covers_function(&function_plan)) {
                reserve_function_signature(&function_plan, function_signatures)?;
                Ok(KmpApiPlan::function(
                    record.name(),
                    record.required_capabilities().clone(),
                    function_plan,
                ))
            } else {
                Err(format!(
                    "KMP JNI glue emission has not been delegated for function {}",
                    function_plan.name()
                ))
            }
        }
        _ => Err(format!(
            "KMP declaration body emission has not been ported for {} {}",
            record.kind(),
            record.name()
        )),
    }
}

/// Builds the KMP function plan for a native free function declaration.
pub fn lower_native_function_plan(
    function: &FunctionDecl<Native>,
) -> std::result::Result<KmpFunctionPlan, String> {
    let callable = function.callable();
    if callable.receiver().is_some()
        || !matches!(callable.execution(), ExecutionDecl::Synchronous(_))
        || !matches!(callable.error().channel(), ErrorChannel::None)
    {
        return Err(
            "KMP function body emission supports only infallible synchronous free functions"
                .to_string(),
        );
    }

    let mut param_names = BTreeSet::new();
    let params = callable
        .params()
        .iter()
        .map(|param| {
            let IncomingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Primitive(primitive),
                receive,
            }) = param.payload()
            else {
                return Err(
                    "KMP function body emission supports only direct primitive parameters"
                        .to_string(),
                );
            };
            if *receive == Receive::ByMutRef {
                return Err(
                    "mutable direct parameter writeback is not planned for KMP functions"
                        .to_string(),
                );
            }
            if !primitive_has_direct_jvm_carrier(*primitive) {
                return Err(format!(
                    "unsigned primitive JNI carrier is not planned for KMP functions: {primitive:?}"
                ));
            }
            let name = names::param_name(param.name());
            if !names::is_valid_identifier(&name) {
                return Err(format!("invalid Kotlin parameter name {name}"));
            }
            if !param_names.insert(name.clone()) {
                return Err(format!("duplicate Kotlin parameter name {name}"));
            }
            Ok(KmpParamPlan::new(name, KmpTypePlan::Primitive(*primitive)))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let returns = match callable.returns().plan() {
        ReturnPlan::Void => None,
        ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::Primitive(primitive),
        } if primitive_has_direct_jvm_carrier(*primitive) => {
            Some(KmpTypePlan::Primitive(*primitive))
        }
        ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::Primitive(primitive),
        } => {
            return Err(format!(
                "unsigned primitive JNI carrier is not planned for KMP functions: {primitive:?}"
            ));
        }
        _ => {
            return Err(
                "KMP function body emission supports only direct primitive return slots"
                    .to_string(),
            );
        }
    };
    let name = names::callable_name(function.name());
    if !names::is_valid_identifier(&name) {
        return Err(format!("invalid Kotlin function name {name}"));
    }

    Ok(KmpFunctionPlan::new(
        name,
        function.symbol().name().as_str(),
        params,
        returns,
    ))
}

fn reserve_function_signature(
    function: &KmpFunctionPlan,
    function_signatures: &mut BTreeSet<String>,
) -> std::result::Result<(), String> {
    let signature = function_signature_key(function.name(), function.params());
    if !function_signatures.insert(signature.clone()) {
        return Err(format!("duplicate Kotlin function signature {signature}"));
    }

    Ok(())
}

fn function_signature_key(name: &str, params: &[KmpParamPlan]) -> String {
    let params = params
        .iter()
        .map(|param| type_signature_key(param.ty()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}({params})")
}

fn type_signature_key(ty: &KmpTypePlan) -> &'static str {
    match ty {
        KmpTypePlan::Primitive(primitive) => primitive_signature_key(*primitive),
    }
}

fn primitive_signature_key(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Bool => "Boolean",
        Primitive::I8 => "Byte",
        Primitive::I16 => "Short",
        Primitive::I32 => "Int",
        Primitive::I64 | Primitive::ISize => "Long",
        Primitive::F32 => "Float",
        Primitive::F64 => "Double",
        Primitive::U8 => "UByte",
        Primitive::U16 => "UShort",
        Primitive::U32 => "UInt",
        Primitive::U64 | Primitive::USize => "ULong",
        _ => "unsupported",
    }
}

fn primitive_has_direct_jvm_carrier(primitive: Primitive) -> bool {
    !matches!(
        primitive,
        Primitive::U8 | Primitive::U16 | Primitive::U32 | Primitive::U64 | Primitive::USize
    )
}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Decl, Native, lower as lower_bindings};

    use super::{
        super::plan::{KmpPlatform, KmpSupportMode},
        KmpLowerError, KmpLowerer, KmpLoweringOptions,
    };
    use crate::target::kmp::{
        KmpApiBody, KmpJvmDelegateFunction, KmpJvmDelegateOutput, KmpTypePlan,
    };

    fn bindings(source: &str) -> Bindings<Native> {
        bindings_for_package("demo", source)
    }

    fn bindings_for_package(package_name: &str, source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source fixture"),
            PackageInfo::new(package_name, None),
        )
        .expect("source should scan");
        lower_bindings::<Native>(&source).expect("source should lower")
    }

    fn unsupported_report(error: KmpLowerError) -> super::super::plan::KmpSupportReport {
        match error {
            KmpLowerError::UnsupportedApis { report } => report,
            other => panic!("unexpected KMP lower error: {other:?}"),
        }
    }

    fn add_delegate_with_signature(
        native_symbol: &str,
        param_types: Vec<KmpTypePlan>,
    ) -> KmpJvmDelegateOutput {
        add_delegate_with_signature_and_jni_glue(
            native_symbol,
            "add",
            param_types,
            "/* delegated JNI glue */\n",
        )
    }

    fn add_delegate_with_signature_and_jni_glue(
        native_symbol: &str,
        kotlin_name: &str,
        param_types: Vec<KmpTypePlan>,
        jni_glue_source: &str,
    ) -> KmpJvmDelegateOutput {
        KmpJvmDelegateOutput::new(
            "com.example.boltffi.jvm",
            "",
            vec![KmpJvmDelegateFunction::new(
                native_symbol,
                kotlin_name,
                param_types,
                Some(KmpTypePlan::Primitive(boltffi_binding::Primitive::I32)),
                jni_glue_source,
            )],
        )
    }

    fn add_delegate_with_native_symbol(native_symbol: &str) -> KmpJvmDelegateOutput {
        add_delegate_with_signature(
            native_symbol,
            vec![
                KmpTypePlan::Primitive(boltffi_binding::Primitive::I32),
                KmpTypePlan::Primitive(boltffi_binding::Primitive::I32),
            ],
        )
    }

    fn add_delegate() -> KmpJvmDelegateOutput {
        add_delegate_with_native_symbol("boltffi_function_demo_add")
    }

    fn unary_i32_delegate(native_symbol: &str) -> KmpJvmDelegateOutput {
        unary_i32_delegates(&[native_symbol])
    }

    fn unary_i32_delegates(native_symbols: &[&str]) -> KmpJvmDelegateOutput {
        KmpJvmDelegateOutput::new(
            "com.example.boltffi.jvm",
            "",
            native_symbols
                .iter()
                .map(|native_symbol| {
                    KmpJvmDelegateFunction::new(
                        *native_symbol,
                        "pingPong",
                        vec![KmpTypePlan::Primitive(boltffi_binding::Primitive::I32)],
                        Some(KmpTypePlan::Primitive(boltffi_binding::Primitive::I32)),
                        "/* delegated JNI glue */\n",
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn strict_lowerer_rejects_sync_function_until_jvm_jni_glue_is_delegated() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("sync functions need real JVM/Android JNI glue before commonMain exposure");
        let report = unsupported_report(error);

        assert_eq!(report.rejected_apis().len(), 1);
        assert_eq!(report.rejected_apis()[0].kind(), "function");
        assert_eq!(report.rejected_apis()[0].name(), "add");
        assert!(
            report.rejected_apis()[0]
                .reason()
                .expect("rejection reason")
                .contains("JNI glue emission")
        );
    }

    #[test]
    fn strict_lowerer_admits_sync_function_covered_by_jvm_delegate() {
        let module = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(add_delegate()))
            .lower(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("covered primitive sync function should be admitted");

        assert_eq!(module.common().apis().len(), 1);
        assert_eq!(module.common().apis()[0].kind(), "function");
        assert_eq!(module.common().apis()[0].name(), "add");
        assert_eq!(module.support_report().admitted_apis().len(), 1);
        assert!(module.support_report().rejected_apis().is_empty());
        assert!(module.jvm_delegate().is_some());
    }

    #[test]
    fn native_function_plan_replaces_hyphenated_package_segments() {
        let bindings = bindings_for_package(
            "my-crate",
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        );
        let function = bindings
            .decls()
            .iter()
            .find_map(|decl| match decl {
                Decl::Function(function) => Some(function.as_ref()),
                _ => None,
            })
            .expect("function should lower");
        let function_plan =
            super::lower_native_function_plan(function).expect("primitive sync function");

        assert_eq!(
            function_plan.native_symbol(),
            "boltffi_function_my_crate_add"
        );
    }

    #[test]
    fn strict_lowerer_rejects_delegate_with_mismatched_native_symbol() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new().jvm_delegate(add_delegate_with_native_symbol("add")),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("delegates must cover the same native symbol");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("JNI glue emission")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_delegate_with_mismatched_parameter_types() {
        let error = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(
            add_delegate_with_signature(
                "boltffi_function_demo_add",
                vec![
                    KmpTypePlan::Primitive(boltffi_binding::Primitive::I64),
                    KmpTypePlan::Primitive(boltffi_binding::Primitive::I32),
                ],
            ),
        ))
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("delegates must cover the same parameter types");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("JNI glue emission")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_delegate_with_empty_jni_glue() {
        let error = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(
            add_delegate_with_signature_and_jni_glue(
                "boltffi_function_demo_add",
                "add",
                vec![
                    KmpTypePlan::Primitive(boltffi_binding::Primitive::I32),
                    KmpTypePlan::Primitive(boltffi_binding::Primitive::I32),
                ],
                "  \n",
            ),
        ))
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("delegates without JNI glue must not admit APIs");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("JNI glue emission")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_invalid_kotlin_function_names() {
        let error = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(
            add_delegate_with_signature_and_jni_glue(
                "boltffi_function_demo__2d",
                "2d",
                vec![KmpTypePlan::Primitive(boltffi_binding::Primitive::I32)],
                "/* delegated JNI glue */\n",
            ),
        ))
        .lower(&bindings(
            r#"
            #[export]
            pub fn _2d(value: i32) -> i32 {
                value
            }
            "#,
        ))
        .expect_err("invalid Kotlin function names must be rejected before emission");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "2d"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("invalid Kotlin function name")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_invalid_kotlin_parameter_names() {
        let error = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(
            add_delegate_with_signature_and_jni_glue(
                "boltffi_function_demo_add",
                "add",
                vec![KmpTypePlan::Primitive(boltffi_binding::Primitive::I32)],
                "/* delegated JNI glue */\n",
            ),
        ))
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(_2d: i32) -> i32 {
                _2d
            }
            "#,
        ))
        .expect_err("invalid Kotlin parameter names must be rejected before emission");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("invalid Kotlin parameter name")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_unsigned_primitives_until_jni_carrier_plan_exists() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub fn round_trip(value: u32) -> u32 {
                value
            }
            "#,
        ))
        .expect_err("unsigned primitive function should fail before commonMain emission");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "round::trip"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unsigned primitive JNI carrier")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_mutable_direct_params_until_writeback_plan_exists() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub fn bump(value: &mut i32) {
                *value += 1;
            }
            "#,
        ))
        .expect_err("mutable direct primitive function should fail before commonMain emission");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "bump"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("mutable direct parameter writeback")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_duplicate_kotlin_function_signatures() {
        let error = KmpLowerer::new(KmpLoweringOptions::new().jvm_delegate(unary_i32_delegates(
            &[
                "boltffi_function_demo_ping_pong",
                "boltffi_function_demo_ping__pong",
            ],
        )))
        .lower(&bindings(
            r#"
                #[export]
                pub fn ping_pong(value: i32) -> i32 {
                    value
                }

                #[export]
                pub fn ping__pong(value: i32) -> i32 {
                    value
                }
                "#,
        ))
        .expect_err("duplicate Kotlin callable signatures should fail closed");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "ping::pong"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("duplicate Kotlin function signature")
        }));
    }

    #[test]
    fn preview_prune_does_not_reserve_signature_for_delegate_missing_function() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new()
                .support_mode(KmpSupportMode::PreviewPruneUnsupported)
                .jvm_delegate(unary_i32_delegate("boltffi_function_demo_ping__pong")),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn ping_pong(value: i32) -> i32 {
                value
            }

            #[export]
            pub fn ping__pong(value: i32) -> i32 {
                value
            }
            "#,
        ))
        .expect("preview pruning should admit the covered duplicate sibling");

        assert_eq!(module.common().apis().len(), 1);
        let KmpApiBody::Function(function) = module.common().apis()[0].body() else {
            panic!("expected admitted function body");
        };
        assert_eq!(function.native_symbol(), "boltffi_function_demo_ping__pong");
    }

    #[test]
    fn strict_lowerer_rejects_duplicate_kotlin_parameter_names() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub fn add(foo_bar: i32, foo__bar: i32) -> i32 {
                foo_bar + foo__bar
            }
            "#,
        ))
        .expect_err("duplicate normalized Kotlin parameter names should fail closed");
        let report = unsupported_report(error);

        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("duplicate Kotlin parameter name")
        }));
    }

    #[test]
    fn preview_prune_omits_unrenderable_functions_and_reports_reasons() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }

            #[export]
            pub fn round_trip(value: u32) -> u32 {
                value
            }
            "#,
        ))
        .expect("preview pruning should omit unrenderable functions");

        assert!(module.common().apis().is_empty());
        assert!(module.support_report().rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("JNI glue emission")
        }));
        assert!(module.support_report().rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "round::trip"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unsigned primitive JNI carrier")
        }));
    }

    #[test]
    fn preview_prune_omits_unrenderable_record_bodies_and_reports_reasons() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }
            "#,
        ))
        .expect("preview pruning should omit unrenderable record bodies");

        assert!(module.common().apis().is_empty());
        assert!(module.support_report().rejected_apis().iter().any(|api| {
            api.kind() == "record"
                && api.name() == "point"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("KMP declaration body emission")
        }));
    }

    #[test]
    fn lowerer_rejects_api_missing_from_any_selected_platform() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new()
                .selected_platforms(vec![KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64]),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("iOS simulator is not admitted yet");

        let report = unsupported_report(error);
        assert_eq!(
            report.selected_platforms(),
            &[KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64]
        );
        assert_eq!(report.rejected_apis().len(), 1);
        assert_eq!(report.rejected_apis()[0].kind(), "function");
        assert_eq!(report.rejected_apis()[0].name(), "add");
        assert!(
            report.rejected_apis()[0]
                .reason()
                .expect("rejection reason")
                .contains("synchronous callables on iosSimulatorArm64")
        );
    }

    #[test]
    fn preview_prune_records_rejected_class_surface_without_common_api() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new() -> Self {
                    Engine
                }

                pub fn version(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        let rejected = module.support_report().rejected_apis();
        assert!(module.common().apis().is_empty());
        assert_eq!(
            module.support_report().mode(),
            KmpSupportMode::PreviewPruneUnsupported
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class" && api.name() == "engine")
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class initializer" && api.name() == "engine::new")
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class method" && api.name() == "engine::version")
        );
    }

    #[test]
    fn strict_lowerer_rejects_unsupported_members_on_admitted_parent() {
        let error = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub async fn load(&self) -> i32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("async record methods are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record"
                && api.name() == "point"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("KMP declaration body emission")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "point::load"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("asynchronous callables on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_mutating_record_and_enum_methods() {
        let error = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn translate(&mut self, dx: i32) {
                    self.x += dx;
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn flip(&mut self) {
                    *self = Mode::Slow;
                }
            }
            "#,
        ))
        .expect_err("mutating record and enum receivers are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record"
                && api.name() == "point"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("KMP declaration body emission")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "enum"
                && api.name() == "mode"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("KMP declaration body emission")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "point::translate"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("mutating receivers on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "enum method"
                && api.name() == "mode::flip"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("mutating receivers on jvm")
        }));
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| { matches!(api.name(), "point::translate" | "mode::flip") })
        );
    }

    #[test]
    fn preview_prune_omits_mutating_record_and_enum_methods() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn translate(&mut self, dx: i32) {
                    self.x += dx;
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn flip(&mut self) {
                    *self = Mode::Slow;
                }
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(module.common().apis().is_empty());
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "record" && api.name() == "point" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "enum" && api.name() == "mode" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "record method" && api.name() == "point::translate" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "enum method" && api.name() == "mode::flip" })
        );
    }

    #[test]
    fn strict_lowerer_rejects_supported_record_and_enum_members_until_bodies_are_ported() {
        let error = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn stable(&self) -> u32 {
                    1
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("supported record and enum members still need body emission");

        let report = unsupported_report(error);
        for (kind, name) in [
            ("record", "point"),
            ("record method", "point::stable"),
            ("enum", "mode"),
            ("enum method", "mode::stable"),
        ] {
            assert!(
                report.rejected_apis().iter().any(|api| {
                    api.kind() == kind
                        && api.name() == name
                        && api
                            .reason()
                            .expect("rejection reason")
                            .contains("KMP declaration body emission")
                }),
                "{:#?}",
                report.rejected_apis()
            );
        }
        assert!(report.admitted_apis().is_empty());
    }

    #[test]
    fn strict_lowerer_rejects_encoded_record_with_unsupported_field_type() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }
            "#,
        ))
        .expect_err("encoded record callback fields are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "record"
                    && api.name() == "bad::record"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_data_enum_with_unsupported_payload_type() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }
            "#,
        ))
        .expect_err("data enum callback payloads are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "enum"
                    && api.name() == "bad::enum"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_functions_using_rejected_records_and_enums() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub fn use_record(value: BadRecord) -> BadRecord {
                value
            }

            #[export]
            pub fn use_enum(value: BadEnum) -> BadEnum {
                value
            }
            "#,
        ))
        .expect_err("dependent APIs must reject when their record or enum type is rejected");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "use::record"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "use::enum"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(!report.admitted_apis().iter().any(|api| {
            api.kind() == "function" && matches!(api.name(), "use::record" | "use::enum")
        }));
    }

    #[test]
    fn preview_prune_omits_functions_using_rejected_records_and_enums() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub fn use_record(value: BadRecord) -> BadRecord {
                value
            }

            #[export]
            pub fn use_enum(value: BadEnum) -> BadEnum {
                value
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(!module.common().apis().iter().any(|api| {
            api.kind() == "function" && matches!(api.name(), "use::record" | "use::enum")
        }));
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "function" && api.name() == "use::record" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "function" && api.name() == "use::enum" })
        );
    }

    #[test]
    fn strict_lowerer_cascades_rejected_record_owner_to_members() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data(impl)]
            impl BadRecord {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("members of rejected records must be rejected too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "bad::record::stable"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(
            !report.admitted_apis().iter().any(|api| {
                api.kind() == "record method" && api.name() == "bad::record::stable"
            })
        );
    }

    #[test]
    fn strict_lowerer_cascades_rejected_enum_owner_to_members() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[data(impl)]
            impl BadEnum {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("members of rejected enums must be rejected too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "enum method"
                && api.name() == "bad::enum::stable"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "enum method" && api.name() == "bad::enum::stable")
        );
    }

    #[test]
    fn strict_lowerer_rejects_empty_platform_matrix() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new().selected_platforms(Vec::<KmpPlatform>::new()),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("KMP planning needs at least one selected platform");

        let KmpLowerError::InvalidPlatformMatrix { reason, report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(reason.contains("no selected KMP platforms"));
        assert_eq!(report.selected_platforms(), &[]);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("no selected KMP platforms")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_empty_platform_matrix_without_apis() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new().selected_platforms(Vec::<KmpPlatform>::new()),
        )
        .lower(&bindings(""))
        .expect_err("KMP planning needs at least one selected platform even for empty bindings");

        let KmpLowerError::InvalidPlatformMatrix { reason, report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(reason.contains("no selected KMP platforms"));
        assert_eq!(report.selected_platforms(), &[]);
        assert!(report.rejected_apis().is_empty());
    }

    #[test]
    fn strict_lowerer_rejects_inline_constant_using_rejected_enum() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                Good,
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub const DEFAULT_BAD: BadEnum = BadEnum::Good;
            "#,
        ))
        .expect_err("inline constants must reject when their declared type is rejected");

        let KmpLowerError::UnsupportedApis { report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "constant"
                    && api.name() == "default::bad"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "constant" && api.name() == "default::bad")
        );
    }

    #[test]
    fn preview_prune_omits_inline_constant_using_rejected_enum() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                Good,
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub const DEFAULT_BAD: BadEnum = BadEnum::Good;
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(
            !module
                .common()
                .apis()
                .iter()
                .any(|api| { api.kind() == "constant" && api.name() == "default::bad" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "constant" && api.name() == "default::bad" }),
            "{:#?}",
            module.support_report().rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_unsupported_fallible_error_payload() {
        let error = super::lower(&bindings(
            r#"
            use std::time::Duration;

            #[export]
            pub fn load() -> Result<String, Duration> {
                Ok(String::new())
            }
            "#,
        ))
        .expect_err("builtin error payloads are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "load"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_unmodeled_typeref_families() {
        let error = super::lower(&bindings(
            r#"
            use std::collections::HashMap;

            #[export]
            pub fn echo_map(value: HashMap<String, String>) -> HashMap<String, String> {
                value
            }
            "#,
        ))
        .expect_err("maps are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "echo::map"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_custom_type_with_unsupported_representation() {
        let error = super::lower(&bindings(
            r#"
            use std::time::Duration;

            custom_type!(
                BadDuration,
                remote = RemoteDuration,
                repr = Duration,
                into_ffi = |_value: &RemoteDuration| Duration::from_secs(0),
                try_from_ffi = |_value: Duration| Ok(RemoteDuration),
            );

            #[export]
            pub fn echo_bad(value: RemoteDuration) -> RemoteDuration {
                value
            }
            "#,
        ))
        .expect_err("custom type representations must be admitted too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "custom type"
                && api.name() == "bad::duration"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "echo::bad"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }
}
