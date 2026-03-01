use super::{
    method::Parameter,
    types::{Deprecation, ReturnType, Type},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub inputs: Vec<Parameter>,
    pub returns: ReturnType,
    pub is_async: bool,
    pub wire_encoded: bool,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl Function {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            returns: ReturnType::Void,
            is_async: false,
            wire_encoded: false,
            doc: None,
            deprecated: None,
        }
    }

    pub fn with_wire_encoded(mut self) -> Self {
        self.wire_encoded = true;
        self
    }

    pub fn with_param(mut self, param: Parameter) -> Self {
        self.inputs.push(param);
        self
    }

    pub fn with_return(mut self, returns: ReturnType) -> Self {
        self.returns = returns;
        self
    }

    pub fn with_output(mut self, ty: Type) -> Self {
        self.returns = ReturnType::value(ty);
        self
    }

    pub fn make_async(mut self) -> Self {
        self.is_async = true;
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn maybe_doc(self, doc: Option<String>) -> Self {
        match doc {
            Some(d) => self.with_doc(d),
            None => self,
        }
    }

    pub fn maybe_async(self, is_async: bool) -> Self {
        if is_async { self.make_async() } else { self }
    }

    pub fn maybe_return(self, returns: Option<ReturnType>) -> Self {
        match returns {
            Some(r) => self.with_return(r),
            None => self,
        }
    }

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn throws(&self) -> bool {
        self.returns.throws()
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    pub fn has_return_value(&self) -> bool {
        self.returns.has_return_value()
    }

    pub fn has_callbacks(&self) -> bool {
        self.inputs.iter().any(|p| matches!(p.param_type, Type::Closure(_)))
    }

    pub fn callback_params(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs.iter().filter(|p| matches!(p.param_type, Type::Closure(_)))
    }

    pub fn non_callback_params(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs.iter().filter(|p| !matches!(p.param_type, Type::Closure(_)))
    }
}
