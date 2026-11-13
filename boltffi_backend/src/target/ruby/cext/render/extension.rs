//! The extension source file and its `Init_` entry point.

use askama::Template;

use crate::{
    bridge::c::{Identifier, Literal},
    core::{GeneratedFile, Result},
    target::ruby::syntax::ConstantPath,
};

#[derive(Template)]
#[template(path = "target/ruby/extension.c", escape = "none")]
struct ExtensionTemplate {
    init: Identifier,
    modules: Vec<Literal>,
}

/// The directory that holds the extension sources, such as `ext/my_lib`.
pub fn directory(stem: &str) -> String {
    format!("ext/{stem}")
}

/// Renders the extension source file, `<stem>.c`.
pub fn render(stem: &str, module: &ConstantPath) -> Result<GeneratedFile> {
    let source = ExtensionTemplate {
        init: Identifier::parse(format!("Init_{stem}"))?,
        modules: module
            .segments()
            .iter()
            .map(|constant| Literal::string(constant.as_str()))
            .collect(),
    }
    .render()?;
    super::file(format!("{}/{stem}.c", directory(stem)), source)
}
