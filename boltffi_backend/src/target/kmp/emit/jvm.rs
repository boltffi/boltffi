//! JVM-family source-set file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::{Error, Result};

use super::{
    super::plan::{KmpApiBody, KmpFunctionPlan, KmpJvmDelegateOutput, KmpModule},
    common::{RenderedFunction, unsupported_body_emission},
};

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/platform_actual.kt", escape = "none")]
struct PlatformActualTemplate<'module> {
    package_name: &'module str,
    internal_package: &'module str,
    functions: Vec<RenderedFunction>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/internal_kotlin.kt", escape = "none")]
struct InternalKotlinTemplate<'module> {
    internal_package: &'module str,
    runtime_lines: Vec<&'module str>,
    native_functions: Vec<RenderedFunction>,
    functions: Vec<RenderedInternalFunction<'module>>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/jni_glue.c", escape = "none")]
struct JniGlueTemplate<'module> {
    shared_source: Option<&'module str>,
    delegate_functions: Vec<&'module str>,
}

struct RenderedInternalFunction<'module> {
    function: RenderedFunction,
    source_lines: Vec<&'module str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KmpJvmAdapter {
    pub(crate) source_set: &'static str,
    pub(crate) actual_file_suffix: &'static str,
}

impl KmpJvmAdapter {
    pub(crate) const fn jvm() -> Self {
        Self {
            source_set: "jvmMain",
            actual_file_suffix: "JvmActual",
        }
    }

    pub(crate) const fn android() -> Self {
        Self {
            source_set: "androidMain",
            actual_file_suffix: "AndroidActual",
        }
    }
}

pub(crate) fn default_adapters() -> Vec<KmpJvmAdapter> {
    vec![KmpJvmAdapter::jvm(), KmpJvmAdapter::android()]
}

pub(crate) fn render_platform_actual(
    module: &KmpModule,
    package_name: &str,
    internal_package: &str,
) -> Result<String> {
    let functions = function_plans(module)?;
    if !functions.is_empty() {
        delegate_for_functions(module, &functions, Some(internal_package))?;
    }

    Ok(PlatformActualTemplate {
        package_name,
        internal_package,
        functions: rendered_functions(&functions)?,
    }
    .render()?)
}

pub(crate) fn render_internal_kotlin(module: &KmpModule, internal_package: &str) -> Result<String> {
    let functions = function_plans(module)?;
    let delegate = (!functions.is_empty())
        .then(|| delegate_for_functions(module, &functions, Some(internal_package)))
        .transpose()?;

    Ok(InternalKotlinTemplate {
        internal_package,
        runtime_lines: runtime_lines(delegate),
        native_functions: rendered_functions(&functions)?,
        functions: rendered_internal_functions(delegate, &functions)?,
    }
    .render()?)
}

pub(crate) fn render_jni_glue(module: &KmpModule) -> Result<String> {
    let functions = function_plans(module)?;
    let (shared_source, delegate_functions) = if functions.is_empty() {
        (None, Vec::new())
    } else {
        let delegate = delegate_for_functions(module, &functions, None)?;
        let shared_source =
            (!delegate.shared_jni_source().trim().is_empty()).then(|| delegate.shared_jni_source());
        let delegate_functions = functions
            .iter()
            .map(|function| {
                delegate_function_for(delegate, function).map(|function| function.jni_glue_source())
            })
            .collect::<Result<Vec<_>>>()?;
        (shared_source, delegate_functions)
    };

    Ok(JniGlueTemplate {
        shared_source,
        delegate_functions,
    }
    .render()?)
}

fn function_plans(module: &KmpModule) -> Result<Vec<&KmpFunctionPlan>> {
    module
        .common()
        .apis()
        .iter()
        .map(|api| match api.body() {
            KmpApiBody::Function(function) => Ok(function),
            KmpApiBody::Unsupported => Err(unsupported_body_emission()),
        })
        .collect()
}

fn rendered_functions(functions: &[&KmpFunctionPlan]) -> Result<Vec<RenderedFunction>> {
    functions
        .iter()
        .map(|function| RenderedFunction::from_plan(function))
        .collect()
}

fn rendered_internal_functions<'module>(
    delegate: Option<&'module KmpJvmDelegateOutput>,
    functions: &[&KmpFunctionPlan],
) -> Result<Vec<RenderedInternalFunction<'module>>> {
    functions
        .iter()
        .map(|function| {
            let source_lines = delegate
                .map(|delegate| delegate_function_for(delegate, function))
                .transpose()?
                .and_then(|function| function.internal_kotlin_source())
                .map(str::trim)
                .filter(|source| !source.is_empty())
                .map(|source| source.lines().collect())
                .unwrap_or_default();
            Ok(RenderedInternalFunction {
                function: RenderedFunction::from_plan(function)?,
                source_lines,
            })
        })
        .collect()
}

fn runtime_lines(delegate: Option<&KmpJvmDelegateOutput>) -> Vec<&str> {
    delegate
        .map(KmpJvmDelegateOutput::internal_kotlin_runtime_source)
        .unwrap_or_default()
        .trim()
        .lines()
        .collect()
}

fn delegate_for_functions<'module>(
    module: &'module KmpModule,
    functions: &[&KmpFunctionPlan],
    internal_package: Option<&str>,
) -> Result<&'module KmpJvmDelegateOutput> {
    let delegate = module.jvm_delegate().ok_or(Error::UnsupportedTarget {
        target: "kotlin_multiplatform",
        shape: "KMP JNI glue emission",
    })?;
    if internal_package.is_some_and(|expected| delegate.internal_package() != expected) {
        return Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        });
    }
    if functions
        .iter()
        .all(|function| delegate.covers_function(function))
    {
        Ok(delegate)
    } else {
        Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        })
    }
}

fn delegate_function_for<'delegate>(
    delegate: &'delegate KmpJvmDelegateOutput,
    function: &KmpFunctionPlan,
) -> Result<&'delegate super::super::plan::KmpJvmDelegateFunction> {
    delegate
        .function_for(function)
        .ok_or(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        })
}

impl RenderedInternalFunction<'_> {
    fn function(&self) -> &RenderedFunction {
        &self.function
    }

    fn source_lines(&self) -> &[&str] {
        &self.source_lines
    }

    fn has_source(&self) -> bool {
        !self.source_lines.is_empty()
    }
}
