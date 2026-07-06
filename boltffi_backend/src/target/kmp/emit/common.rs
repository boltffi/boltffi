//! commonMain Kotlin source rendering for KMP emission.

use askama::Template as AskamaTemplate;
use boltffi_binding::Primitive;

use crate::core::{Error, Result};

use super::super::plan::{KmpApiBody, KmpFunctionPlan, KmpModule, KmpParamPlan, KmpTypePlan};

#[derive(AskamaTemplate)]
#[template(path = "target/kmp/common_module.kt", escape = "none")]
struct CommonModuleTemplate<'module> {
    package_name: &'module str,
    functions: Vec<RenderedFunction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedFunction {
    name: String,
    native_symbol: String,
    params: Vec<RenderedParam>,
    returns: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedParam {
    name: String,
    ty: String,
}

pub(crate) fn render_common_module(module: &KmpModule, package_name: &str) -> Result<String> {
    Ok(CommonModuleTemplate {
        package_name,
        functions: render_functions(module)?,
    }
    .render()?)
}

pub(crate) fn render_functions(module: &KmpModule) -> Result<Vec<RenderedFunction>> {
    module
        .common()
        .apis()
        .iter()
        .map(|api| match api.body() {
            KmpApiBody::Function(function) => RenderedFunction::from_plan(function),
            KmpApiBody::Unsupported => Err(unsupported_body_emission()),
        })
        .collect()
}

pub(crate) fn render_type(ty: &KmpTypePlan) -> Result<String> {
    match ty {
        KmpTypePlan::Primitive(primitive) => render_primitive_type(*primitive),
    }
}

pub(crate) fn unsupported_body_emission() -> Error {
    Error::UnsupportedTarget {
        target: "kotlin_multiplatform",
        shape: "KMP declaration body emission",
    }
}

fn render_primitive_type(primitive: Primitive) -> Result<String> {
    let name = match primitive {
        Primitive::Bool => "Boolean",
        Primitive::I8 => "Byte",
        Primitive::I16 => "Short",
        Primitive::I32 => "Int",
        Primitive::I64 | Primitive::ISize => "Long",
        Primitive::F32 => "Float",
        Primitive::F64 => "Double",
        _ => return Err(unsupported_body_emission()),
    };
    Ok(name.to_string())
}

impl RenderedFunction {
    fn from_plan(function: &KmpFunctionPlan) -> Result<Self> {
        Ok(Self {
            name: function.name().to_string(),
            native_symbol: function.native_symbol().to_string(),
            params: function
                .params()
                .iter()
                .map(RenderedParam::from_plan)
                .collect::<Result<Vec<_>>>()?,
            returns: function.returns().map(render_type).transpose()?,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn native_symbol(&self) -> &str {
        &self.native_symbol
    }

    pub(crate) fn params(&self) -> &[RenderedParam] {
        &self.params
    }

    pub(crate) fn returns(&self) -> Option<&str> {
        self.returns.as_deref()
    }

    pub(crate) fn returns_value(&self) -> bool {
        self.returns.is_some()
    }
}

impl RenderedParam {
    fn from_plan(param: &KmpParamPlan) -> Result<Self> {
        Ok(Self {
            name: param.name().to_string(),
            ty: render_type(param.ty())?,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn ty(&self) -> &str {
        &self.ty
    }
}
