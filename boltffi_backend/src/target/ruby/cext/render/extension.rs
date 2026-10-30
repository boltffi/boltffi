//! The extension source file and its `Init_` entry point.

use askama::Template;

use crate::{
    bridge::c::{Identifier, Literal},
    core::{GeneratedFile, Result},
    target::ruby::{name_style::extension_directory, syntax::ConstantPath},
};

#[derive(Template)]
#[template(path = "target/ruby/extension.c", escape = "none")]
struct ExtensionTemplate {
    init: Identifier,
    modules: Vec<Literal>,
}

/// Renders the extension source file, `<stem>.c`.
pub fn render(gem: &str, stem: &str, module: &ConstantPath) -> Result<GeneratedFile> {
    let source = ExtensionTemplate {
        init: Identifier::parse(format!("Init_{stem}"))?,
        modules: module
            .segments()
            .iter()
            .map(|constant| Literal::string(constant.as_str()))
            .collect(),
    }
    .render()?;
    super::file(format!("{}/{stem}.c", extension_directory(gem)), source)
}
