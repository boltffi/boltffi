use std::collections::HashMap;

use boltffi_backend::target::kmp::lower::lower_native_function_plan;
use boltffi_backend::target::kmp::{
    KmpFunctionPlan, KmpJvmDelegateFunction, KmpJvmDelegateOutput, KmpTypePlan,
};
use boltffi_binding::{
    Bindings, CanonicalName as BindingName, Decl, DirectValueType, ErrorChannel, ExecutionDecl,
    FunctionDecl as BindingFunctionDecl, IncomingParam, Native, ParamPlan,
    Primitive as BackendPrimitive, Receive, ReturnPlan,
};
use boltffi_ffi_rules::{callable::ExecutionKind, naming};

use crate::ir::abi::{AbiContract, CallId, CallMode};
use crate::ir::definitions::{FunctionDef, ParamDef, ParamPassing, ReturnDef};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{FfiContract, Lowerer, PackageInfo, TypeCatalog};
use crate::render::jni::{JniEmitter, JniFunction, JniLowerer, JniModule, JvmBindingStyle};
use crate::render::kmp::{
    KmpSurfaceSupport, filter_abi_for_kmp_surface, filter_contract_for_kmp_surface,
    kmp_generated_c_header_include,
};
use crate::render::kotlin::{
    KotlinEmitter, KotlinFunction, KotlinLowerer, KotlinModule, KotlinOptions,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum KmpJvmDelegateAdapterError {
    #[error("Kotlin/JNI delegate source did not contain the Native object")]
    MissingNativeObject,
    #[error("JNI function source for {jni_name} was not isolated from shared source")]
    MissingJniFunction { jni_name: String },
}

pub(crate) struct KmpJvmDelegateAdapter {
    package_name: String,
    module_name: String,
    kotlin_options: KotlinOptions,
}

impl KmpJvmDelegateAdapter {
    pub(crate) fn new(
        package_name: impl Into<String>,
        module_name: impl Into<String>,
        kotlin_options: KotlinOptions,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            module_name: module_name.into(),
            kotlin_options,
        }
    }

    pub(crate) fn adapt(
        &self,
        contract: &FfiContract,
        abi: &AbiContract,
    ) -> Result<KmpJvmDelegateOutput, KmpJvmDelegateAdapterError> {
        self.adapt_with_delegate_entries(contract, abi, &HashMap::new())
    }

    pub(crate) fn adapt_bindings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<KmpJvmDelegateOutput, KmpJvmDelegateAdapterError> {
        let (contract, delegate_entries_by_legacy_symbol) = legacy_contract_for_bindings(bindings);
        let abi = Lowerer::new(&contract).to_abi_contract();
        self.adapt_with_delegate_entries(&contract, &abi, &delegate_entries_by_legacy_symbol)
    }

    fn adapt_with_delegate_entries(
        &self,
        contract: &FfiContract,
        abi: &AbiContract,
        delegate_entries_by_legacy_symbol: &HashMap<String, KmpFunctionPlan>,
    ) -> Result<KmpJvmDelegateOutput, KmpJvmDelegateAdapterError> {
        let internal_package = format!("{}.jvm", self.package_name);
        let support = KmpSurfaceSupport::for_contract(contract);
        let internal_contract = filter_contract_for_kmp_delegate_surface(contract, &support);
        let internal_abi = filter_abi_for_kmp_surface(&internal_contract, abi, &support);

        let kotlin_module = KotlinLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package.clone(),
            self.module_name.clone(),
            self.kotlin_options.clone(),
        )
        .lower();
        let jni_module = JniLowerer::new(
            &internal_contract,
            &internal_abi,
            internal_package.clone(),
            self.module_name.clone(),
        )
        .with_jvm_binding_style(JvmBindingStyle::Kotlin)
        .with_header_include(kmp_generated_c_header_include(
            &internal_contract.package.name,
        ))
        .lower();

        let runtime_source = native_runtime_members(&kotlin_module)?;
        let shared_jni_source = shared_jni_source(&jni_module);
        let functions = delegate_functions(
            &internal_contract,
            &internal_abi,
            &kotlin_module,
            &jni_module,
            &shared_jni_source,
            delegate_entries_by_legacy_symbol,
        )?;

        Ok(
            KmpJvmDelegateOutput::new(internal_package, runtime_source, functions)
                .with_shared_jni_source(shared_jni_source),
        )
    }
}

fn delegate_functions(
    contract: &FfiContract,
    abi: &AbiContract,
    kotlin_module: &KotlinModule,
    jni_module: &JniModule,
    shared_jni_source: &str,
    delegate_entries_by_legacy_symbol: &HashMap<String, KmpFunctionPlan>,
) -> Result<Vec<KmpJvmDelegateFunction>, KmpJvmDelegateAdapterError> {
    let function_defs = function_defs_by_symbol(contract, abi);
    let native_functions = kotlin_module
        .native
        .functions
        .iter()
        .filter(|function| !function.is_async())
        .map(|function| (function.ffi_name.as_str(), function))
        .collect::<HashMap<_, _>>();
    let jni_functions = jni_module
        .functions
        .iter()
        .map(|function| (function.ffi_name.as_str(), function))
        .collect::<HashMap<_, _>>();
    let mut delegates = Vec::new();

    for kotlin_function in &kotlin_module.functions {
        if kotlin_function.is_async() {
            continue;
        }
        let Some(function_def) = function_defs.get(kotlin_function.ffi_name.as_str()) else {
            continue;
        };
        if !native_functions.contains_key(kotlin_function.ffi_name.as_str()) {
            continue;
        }
        let Some(jni_function) = jni_functions.get(kotlin_function.ffi_name.as_str()) else {
            continue;
        };
        let legacy_symbol = kotlin_function.ffi_name.as_str();
        let (native_symbol, kotlin_name, param_types, param_names, returns) =
            if let Some(function_plan) = delegate_entries_by_legacy_symbol.get(legacy_symbol) {
                (
                    function_plan.native_symbol().to_string(),
                    function_plan.name().to_string(),
                    function_plan_param_types(function_plan),
                    Some(function_plan_param_names(function_plan)),
                    function_plan.returns().cloned(),
                )
            } else {
                let Some((param_types, returns)) = kmp_function_signature(function_def) else {
                    continue;
                };
                (
                    kmp_native_function_symbol(contract, function_def),
                    kotlin_function.func_name.clone(),
                    param_types,
                    None,
                    returns,
                )
            };
        let internal_kotlin_source = delegated_kotlin_function_source(
            kotlin_function,
            &kotlin_name,
            &native_symbol,
            param_names.as_deref(),
        );
        delegates.push(
            KmpJvmDelegateFunction::new(
                native_symbol.clone(),
                kotlin_name,
                param_types,
                returns,
                jni_function_source(jni_module, jni_function, &native_symbol, shared_jni_source)?,
            )
            .with_internal_kotlin_source(internal_kotlin_source),
        );
    }

    Ok(delegates)
}

fn delegated_kotlin_function_source(
    function: &KotlinFunction,
    kotlin_name: &str,
    native_symbol: &str,
    param_names: Option<&[String]>,
) -> String {
    let mut delegated = function.clone();
    delegated.func_name = kotlin_name.to_string();
    delegated.ffi_name = native_symbol.to_string();
    if let Some(param_names) = param_names {
        delegated
            .signature_params
            .iter_mut()
            .zip(param_names)
            .for_each(|(param, name)| param.name = name.clone());
        delegated.native_args = param_names.to_vec();
    }
    KotlinEmitter::emit_function(&delegated)
}

fn function_plan_param_types(function_plan: &KmpFunctionPlan) -> Vec<KmpTypePlan> {
    function_plan
        .params()
        .iter()
        .map(|param| param.ty().clone())
        .collect()
}

fn function_plan_param_names(function_plan: &KmpFunctionPlan) -> Vec<String> {
    function_plan
        .params()
        .iter()
        .map(|param| param.name().to_string())
        .collect()
}

fn legacy_contract_for_bindings(
    bindings: &Bindings<Native>,
) -> (FfiContract, HashMap<String, KmpFunctionPlan>) {
    let mut delegate_entries_by_legacy_symbol = HashMap::new();
    let mut functions = Vec::new();
    for declaration in bindings.decls() {
        let Decl::Function(function) = declaration else {
            continue;
        };
        let Ok(function_plan) = lower_native_function_plan(function) else {
            continue;
        };
        let legacy_id = legacy_function_id(function_plan.native_symbol());
        let Some(function_def) = legacy_function_for_binding(function, &legacy_id) else {
            continue;
        };
        let legacy_symbol = naming::function_ffi_name(function_def.id.as_str()).into_string();
        delegate_entries_by_legacy_symbol.insert(legacy_symbol, function_plan);
        functions.push(function_def);
    }

    (
        FfiContract {
            package: PackageInfo {
                name: binding_name_identifier(bindings.package().name()),
                version: bindings.package().version().map(ToOwned::to_owned),
            },
            catalog: TypeCatalog::new(),
            functions,
        },
        delegate_entries_by_legacy_symbol,
    )
}

fn legacy_function_id(native_symbol: &str) -> String {
    format!("kmp_delegate_{native_symbol}")
}

fn legacy_function_for_binding(
    function: &BindingFunctionDecl<Native>,
    legacy_id: &str,
) -> Option<FunctionDef> {
    let callable = function.callable();
    if callable.receiver().is_some()
        || !matches!(callable.execution(), ExecutionDecl::Synchronous(_))
        || !matches!(callable.error().channel(), ErrorChannel::None)
    {
        return None;
    }

    let params = callable
        .params()
        .iter()
        .map(|param| {
            let IncomingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Primitive(primitive),
                receive,
            }) = param.payload()
            else {
                return None;
            };
            Some(ParamDef {
                name: binding_name_identifier(param.name()).into(),
                type_expr: TypeExpr::Primitive(legacy_primitive(*primitive)?),
                passing: legacy_param_passing(*receive)?,
                doc: None,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let returns = match callable.returns().plan() {
        ReturnPlan::Void => ReturnDef::Void,
        ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::Primitive(primitive),
        } => ReturnDef::Value(TypeExpr::Primitive(legacy_primitive(*primitive)?)),
        _ => return None,
    };

    Some(FunctionDef {
        id: legacy_id.into(),
        params,
        returns,
        execution_kind: ExecutionKind::Sync,
        doc: function.meta().doc().map(|doc| doc.as_str().to_string()),
        deprecated: None,
    })
}

fn binding_name_identifier(name: &BindingName) -> String {
    name.parts()
        .iter()
        .map(|part| part.as_str())
        .collect::<Vec<_>>()
        .join("_")
}

fn legacy_param_passing(receive: Receive) -> Option<ParamPassing> {
    match receive {
        Receive::ByValue => Some(ParamPassing::Value),
        Receive::ByRef => Some(ParamPassing::Ref),
        Receive::ByMutRef => Some(ParamPassing::RefMut),
        _ => None,
    }
}

fn legacy_primitive(primitive: BackendPrimitive) -> Option<PrimitiveType> {
    match primitive {
        BackendPrimitive::Bool => Some(PrimitiveType::Bool),
        BackendPrimitive::I8 => Some(PrimitiveType::I8),
        BackendPrimitive::U8 => Some(PrimitiveType::U8),
        BackendPrimitive::I16 => Some(PrimitiveType::I16),
        BackendPrimitive::U16 => Some(PrimitiveType::U16),
        BackendPrimitive::I32 => Some(PrimitiveType::I32),
        BackendPrimitive::U32 => Some(PrimitiveType::U32),
        BackendPrimitive::I64 => Some(PrimitiveType::I64),
        BackendPrimitive::U64 => Some(PrimitiveType::U64),
        BackendPrimitive::ISize => Some(PrimitiveType::ISize),
        BackendPrimitive::USize => Some(PrimitiveType::USize),
        BackendPrimitive::F32 => Some(PrimitiveType::F32),
        BackendPrimitive::F64 => Some(PrimitiveType::F64),
        _ => None,
    }
}

fn filter_contract_for_kmp_delegate_surface(
    contract: &FfiContract,
    support: &KmpSurfaceSupport,
) -> FfiContract {
    let mut contract = filter_contract_for_kmp_surface(contract, support);
    contract
        .functions
        .retain(|function| kmp_function_signature(function).is_some());
    contract
}

fn function_defs_by_symbol<'contract>(
    contract: &'contract FfiContract,
    abi: &'contract AbiContract,
) -> HashMap<&'contract str, &'contract FunctionDef> {
    contract
        .functions
        .iter()
        .filter_map(|function| {
            let symbol = abi
                .calls
                .iter()
                .find_map(|call| match (&call.id, &call.mode) {
                    (CallId::Function(id), CallMode::Sync) if id == &function.id => {
                        Some(call.symbol.as_str())
                    }
                    _ => None,
                })?;
            Some((symbol, function))
        })
        .collect()
}

fn kmp_function_signature(
    function: &FunctionDef,
) -> Option<(Vec<KmpTypePlan>, Option<KmpTypePlan>)> {
    if function.is_async() {
        return None;
    }
    let params = function
        .params
        .iter()
        .map(|param| match param.passing {
            ParamPassing::Value | ParamPassing::Ref => kmp_type_for_type_expr(&param.type_expr),
            ParamPassing::RefMut | ParamPassing::ImplTrait | ParamPassing::BoxedDyn => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let returns = match &function.returns {
        ReturnDef::Void => None,
        ReturnDef::Value(ty) => Some(kmp_type_for_type_expr(ty)?),
        ReturnDef::Result { .. } => return None,
    };

    Some((params, returns))
}

fn kmp_type_for_type_expr(ty: &TypeExpr) -> Option<KmpTypePlan> {
    let TypeExpr::Primitive(primitive) = ty else {
        return None;
    };
    let primitive = match primitive {
        PrimitiveType::Bool => BackendPrimitive::Bool,
        PrimitiveType::I8 => BackendPrimitive::I8,
        PrimitiveType::I16 => BackendPrimitive::I16,
        PrimitiveType::I32 => BackendPrimitive::I32,
        PrimitiveType::I64 => BackendPrimitive::I64,
        PrimitiveType::ISize => BackendPrimitive::ISize,
        PrimitiveType::F32 => BackendPrimitive::F32,
        PrimitiveType::F64 => BackendPrimitive::F64,
        PrimitiveType::U8
        | PrimitiveType::U16
        | PrimitiveType::U32
        | PrimitiveType::U64
        | PrimitiveType::USize => return None,
    };
    Some(KmpTypePlan::Primitive(primitive))
}

fn kmp_native_function_symbol(contract: &FfiContract, function: &FunctionDef) -> String {
    let source_id = if function.id.as_str().contains("::") {
        function.id.as_str().to_string()
    } else {
        format!("{}::{}", contract.package.name, function.id.as_str())
    };
    format!("boltffi_function_{}", source_id_to_symbol_path(&source_id))
}

fn source_id_to_symbol_path(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_")
}

fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    chars
        .iter()
        .enumerate()
        .fold(String::new(), |mut result, (index, &character)| {
            if character.is_uppercase() && index > 0 {
                let previous = chars[index - 1];
                let next = chars.get(index + 1).copied();
                let previous_is_word = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_word_break = previous.is_uppercase()
                    && next.is_some_and(|character| character.is_lowercase());
                if previous_is_word || acronym_word_break {
                    result.push('_');
                }
            }
            result.extend(character.to_lowercase());
            result
        })
}

fn native_runtime_members(
    kotlin_module: &KotlinModule,
) -> Result<String, KmpJvmDelegateAdapterError> {
    let mut runtime_module = kotlin_module.clone();
    runtime_module.functions.clear();
    runtime_module.classes.clear();
    runtime_module.callbacks.clear();
    runtime_module.native.functions.clear();
    runtime_module.native.wire_functions.clear();
    runtime_module.native.classes.clear();
    runtime_module.native.async_callback_invokers.clear();

    let source = KotlinEmitter::emit(&runtime_module);
    extract_native_object_members(&source)
}

fn extract_native_object_members(source: &str) -> Result<String, KmpJvmDelegateAdapterError> {
    let marker = "private object Native {";
    let start = source
        .find(marker)
        .map(|index| index + marker.len())
        .ok_or(KmpJvmDelegateAdapterError::MissingNativeObject)?;
    let body = &source[start..];
    let end = body
        .rfind("\n}")
        .ok_or(KmpJvmDelegateAdapterError::MissingNativeObject)?;
    Ok(unindent_kotlin_members(&body[..end]))
}

fn unindent_kotlin_members(source: &str) -> String {
    let mut out = source
        .lines()
        .map(|line| line.strip_prefix("    ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    out.push('\n');
    out
}

fn shared_jni_source(jni_module: &JniModule) -> String {
    let module = shared_jni_module(jni_module);
    JniEmitter::emit(&module)
}

fn jni_function_source(
    jni_module: &JniModule,
    function: &JniFunction,
    native_symbol: &str,
    shared_jni_source: &str,
) -> Result<String, KmpJvmDelegateAdapterError> {
    let mut module = shared_jni_module(jni_module);
    let renamed_function = renamed_jni_function(function, &module.jni_prefix, native_symbol);
    let jni_name = renamed_function.jni_name.clone();
    module.functions = vec![renamed_function];
    let source = JniEmitter::emit(&module);
    let function_source = source
        .strip_prefix(shared_jni_source)
        .ok_or_else(|| KmpJvmDelegateAdapterError::MissingJniFunction {
            jni_name: jni_name.clone(),
        })?
        .trim();
    if function_source.is_empty() || !function_source.contains(&jni_name) {
        return Err(KmpJvmDelegateAdapterError::MissingJniFunction { jni_name });
    }
    Ok(format!("{function_source}\n"))
}

fn renamed_jni_function(
    function: &JniFunction,
    jni_prefix: &str,
    native_symbol: &str,
) -> JniFunction {
    let mut function = function.clone();
    function.ffi_name = native_symbol.to_string();
    function.jni_name = jni_export_name(jni_prefix, native_symbol);
    function
}

fn jni_export_name(jni_prefix: &str, native_symbol: &str) -> String {
    format!(
        "Java_{}_Native_{}",
        jni_prefix,
        native_symbol.replace('_', "_1")
    )
}

fn shared_jni_module(jni_module: &JniModule) -> JniModule {
    let mut module = jni_module.clone();
    module.functions.clear();
    module.wire_functions.clear();
    module.async_functions.clear();
    module.classes.clear();
    module.callback_traits.clear();
    module.async_callback_invokers.clear();
    module.closure_trampolines.clear();
    module
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceCanonicalName, DocComment, FunctionDef as SourceFunctionDef,
        FunctionId as SourceFunctionId, PackageInfo as SourcePackageInfo,
        ParameterDef as SourceParameterDef, Primitive as SourcePrimitive,
        ReturnDef as SourceReturnDef, SourceContract, SourceName, TypeExpr as SourceTypeExpr,
    };
    use boltffi_backend::target::kmp::{KmpFunctionPlan, KmpParamPlan};

    use super::*;
    use crate::ir::definitions::{ParamDef, ParamPassing};
    use crate::ir::{Lowerer, PackageInfo, TypeCatalog};
    use boltffi_ffi_rules::callable::ExecutionKind;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo".to_string(),
                version: None,
            },
            catalog: TypeCatalog::new(),
            functions: Vec::new(),
        }
    }

    fn sync_primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        primitive_function(id, params, returns, ExecutionKind::Sync)
    }

    fn async_primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        primitive_function(id, params, returns, ExecutionKind::Async)
    }

    fn primitive_function(
        id: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
        execution_kind: ExecutionKind,
    ) -> FunctionDef {
        FunctionDef {
            id: id.into(),
            params: params
                .into_iter()
                .map(|(name, primitive)| ParamDef {
                    name: name.into(),
                    type_expr: TypeExpr::Primitive(primitive),
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind,
            doc: None,
            deprecated: None,
        }
    }

    fn adapt(contract: &FfiContract) -> KmpJvmDelegateOutput {
        let abi = Lowerer::new(contract).to_abi_contract();
        KmpJvmDelegateAdapter::new("com.example.demo", "Demo", KotlinOptions::default())
            .adapt(contract, &abi)
            .expect("delegate adapter should render")
    }

    fn bindings_for_functions(functions: Vec<SourceFunctionDef>) -> Bindings<Native> {
        let mut source = SourceContract::new(SourcePackageInfo::new("demo", None));
        source.functions = functions;
        boltffi_binding::lower::<Native>(&source).expect("function should lower")
    }

    fn source_name(part: &str) -> SourceName {
        SourceName::from_canonical(SourceCanonicalName::single(part))
    }

    fn source_primitive_function(
        id: &str,
        name: &str,
        params: Vec<(&str, SourcePrimitive)>,
        returns: SourcePrimitive,
    ) -> SourceFunctionDef {
        let mut function = SourceFunctionDef::new(SourceFunctionId::new(id), source_name(name));
        function.parameters = params
            .into_iter()
            .map(|(name, primitive)| {
                SourceParameterDef::value(source_name(name), SourceTypeExpr::Primitive(primitive))
            })
            .collect();
        function.returns = SourceReturnDef::value(SourceTypeExpr::Primitive(returns));
        function
    }

    #[test]
    fn adapter_builds_delegate_for_sync_primitive_function() {
        let mut contract = empty_contract();
        let mut add = sync_primitive_function(
            "add",
            vec![("left", PrimitiveType::I32), ("right", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        );
        add.doc = Some("Doc mentions Native.boltffi_add(left, right).".to_string());
        contract.functions.push(add);

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("primitive function should be covered by the adapter");

        assert_eq!(delegate.internal_package(), "com.example.demo.jvm");
        assert!(
            delegate
                .internal_kotlin_runtime_source()
                .contains("System.loadLibrary(androidLibrary)")
        );
        assert!(
            !delegate
                .internal_kotlin_runtime_source()
                .contains("private object Native")
        );
        assert!(delegate.shared_jni_source().contains("#include <jni.h>"));
        assert!(
            !delegate
                .shared_jni_source()
                .contains("JNIEXPORT jint JNICALL")
        );
        assert!(function.jni_glue_source().contains(
            "JNIEXPORT jint JNICALL Java_com_example_demo_jvm_Native_boltffi_1function_1demo_1add"
        ));
        assert!(
            function
                .jni_glue_source()
                .contains("_result = boltffi_function_demo_add(left, right);")
        );
        let internal_kotlin_source = function
            .internal_kotlin_source()
            .expect("adapter should provide delegated Kotlin source");
        assert!(internal_kotlin_source.contains("fun add(left: Int, right: Int): Int"));
        assert!(
            internal_kotlin_source.contains("return Native.boltffi_function_demo_add(left, right)")
        );
        assert!(internal_kotlin_source.contains("Doc mentions Native.boltffi_add(left, right)."));
        assert!(
            !function
                .jni_glue_source()
                .contains("Java_com_example_demo_jvm_Native_boltffi_1add")
        );
        assert!(
            !function
                .jni_glue_source()
                .contains("_result = boltffi_add(left, right);")
        );
        assert!(!function.jni_glue_source().contains("#include <jni.h>"));
    }

    #[test]
    fn adapter_preserves_binding_docs_when_adapting_production_bindings() {
        let mut add = source_primitive_function(
            "demo::add",
            "add",
            vec![
                ("left", SourcePrimitive::I32),
                ("right", SourcePrimitive::I32),
            ],
            SourcePrimitive::I32,
        );
        add.doc = Some(DocComment::new(
            "Adds two values through the production binding path.",
        ));
        let bindings = bindings_for_functions(vec![add]);

        let delegate =
            KmpJvmDelegateAdapter::new("com.example.demo", "Demo", KotlinOptions::default())
                .adapt_bindings(&bindings)
                .expect("delegate adapter should render production bindings");
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("production primitive function should be covered by the adapter");

        assert!(
            function
                .internal_kotlin_source()
                .expect("adapter should provide delegated Kotlin source")
                .contains("Adds two values through the production binding path.")
        );
    }

    #[test]
    fn adapter_sanitizes_binding_docs_in_internal_kotlin_source() {
        let mut add = source_primitive_function(
            "demo::add",
            "add",
            vec![
                ("left", SourcePrimitive::I32),
                ("right", SourcePrimitive::I32),
            ],
            SourcePrimitive::I32,
        );
        add.doc = Some(DocComment::new(
            "Safe docs */ fun injected() {} /* still docs",
        ));
        let bindings = bindings_for_functions(vec![add]);

        let delegate =
            KmpJvmDelegateAdapter::new("com.example.demo", "Demo", KotlinOptions::default())
                .adapt_bindings(&bindings)
                .expect("delegate adapter should render production bindings");
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let internal_kotlin_source = delegate
            .function_for(&function_plan)
            .expect("production primitive function should be covered by the adapter")
            .internal_kotlin_source()
            .expect("adapter should provide delegated Kotlin source");

        assert!(!internal_kotlin_source.contains("*/ fun injected()"));
        assert!(!internal_kotlin_source.contains("/* still docs"));
        assert!(internal_kotlin_source.contains("Safe docs * / fun injected() {} / * still docs"));
    }

    #[test]
    fn adapter_uses_backend_planned_parameter_names_in_internal_kotlin_source() {
        let echo = source_primitive_function(
            "demo::echo",
            "echo",
            vec![("HTTPHeader", SourcePrimitive::I32)],
            SourcePrimitive::I32,
        );
        let bindings = bindings_for_functions(vec![echo]);

        let delegate =
            KmpJvmDelegateAdapter::new("com.example.demo", "Demo", KotlinOptions::default())
                .adapt_bindings(&bindings)
                .expect("delegate adapter should render production bindings");
        let function_plan = KmpFunctionPlan::new(
            "echo",
            "boltffi_function_demo_echo",
            vec![KmpParamPlan::new(
                "httpheader",
                KmpTypePlan::Primitive(BackendPrimitive::I32),
            )],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("production primitive function should be covered by the adapter");
        let internal_kotlin_source = function
            .internal_kotlin_source()
            .expect("adapter should provide delegated Kotlin source");

        assert!(internal_kotlin_source.contains("fun echo(httpheader: Int): Int"));
        assert!(
            internal_kotlin_source.contains("return Native.boltffi_function_demo_echo(httpheader)")
        );
    }

    #[test]
    fn adapter_filters_async_functions_before_lowering_delegate_runtime() {
        let mut contract = empty_contract();
        contract.functions.push(sync_primitive_function(
            "add",
            vec![("left", PrimitiveType::I32), ("right", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));
        contract.functions.push(async_primitive_function(
            "spin",
            vec![("value", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "add",
            "boltffi_function_demo_add",
            vec![
                KmpParamPlan::new("left", KmpTypePlan::Primitive(BackendPrimitive::I32)),
                KmpParamPlan::new("right", KmpTypePlan::Primitive(BackendPrimitive::I32)),
            ],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );

        assert!(delegate.covers_function(&function_plan));
        assert!(!delegate.shared_jni_source().contains("JNI_OnLoad"));
        assert!(!delegate.shared_jni_source().contains("g_jvm"));
        assert!(!delegate.shared_jni_source().contains("spin"));
        assert!(
            !delegate
                .internal_kotlin_runtime_source()
                .contains("boltffiFutureContinuationCallback")
        );
        assert!(!delegate.internal_kotlin_runtime_source().contains("spin"));
    }

    #[test]
    fn adapter_covers_immutable_primitive_ref_params_as_direct_primitives() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: "read".into(),
            params: vec![ParamDef {
                name: "value".into(),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                passing: ParamPassing::Ref,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new(
            "read",
            "boltffi_function_demo_read",
            vec![KmpParamPlan::new(
                "value",
                KmpTypePlan::Primitive(BackendPrimitive::I32),
            )],
            Some(KmpTypePlan::Primitive(BackendPrimitive::I32)),
        );
        let function = delegate
            .function_for(&function_plan)
            .expect("immutable primitive ref should be covered as a direct primitive");

        assert!(function.jni_glue_source().contains(
            "JNIEXPORT jint JNICALL Java_com_example_demo_jvm_Native_boltffi_1function_1demo_1read"
        ));
        assert!(
            function
                .jni_glue_source()
                .contains("_result = boltffi_function_demo_read(value);")
        );
    }

    #[test]
    fn adapter_does_not_cover_non_primitive_functions_until_conversion_plan_exists() {
        let mut contract = empty_contract();
        contract.functions.push(FunctionDef {
            id: "load".into(),
            params: vec![ParamDef {
                name: "name".into(),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: boltffi_ffi_rules::callable::ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let delegate = adapt(&contract);
        let function_plan = KmpFunctionPlan::new("load", "boltffi_load", Vec::new(), None);

        assert!(!delegate.covers_function(&function_plan));
        assert!(delegate.shared_jni_source().contains("#include <jni.h>"));
    }
}
