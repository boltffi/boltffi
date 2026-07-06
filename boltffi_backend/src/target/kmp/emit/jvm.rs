//! JVM-family source-set file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::{Error, Result};

use super::{
    super::plan::KmpModule,
    common::{RenderedFunction, render_functions},
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
    functions: Vec<RenderedFunction>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/jni_glue.c", escape = "none")]
struct JniGlueTemplate;

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
    Ok(PlatformActualTemplate {
        package_name,
        internal_package,
        functions: render_functions(module)?,
    }
    .render()?)
}

pub(crate) fn render_internal_kotlin(module: &KmpModule, internal_package: &str) -> Result<String> {
    Ok(InternalKotlinTemplate {
        internal_package,
        functions: render_functions(module)?,
    }
    .render()?)
}

pub(crate) fn render_jni_glue(module: &KmpModule) -> Result<String> {
    if !render_functions(module)?.is_empty() {
        return Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP JNI glue emission",
        });
    }

    Ok(JniGlueTemplate.render()?)
}
