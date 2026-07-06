//! Gradle project file rendering for KMP emission.

use askama::Template as AskamaTemplate;

use crate::core::Result;

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/build.gradle.kts", escape = "none")]
struct BuildGradleTemplate<'options> {
    package_name: &'options str,
    min_sdk: u32,
}

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/settings.gradle.kts", escape = "none")]
struct SettingsGradleTemplate<'options> {
    module_name: &'options str,
}

pub(crate) fn render_build_gradle(package_name: &str, min_sdk: u32) -> Result<String> {
    Ok(BuildGradleTemplate {
        package_name,
        min_sdk,
    }
    .render()?)
}

pub(crate) fn render_settings_gradle(module_name: &str) -> Result<String> {
    Ok(SettingsGradleTemplate { module_name }.render()?)
}
