use super::{
    method::Method,
    stream::{AsyncIteratorMethod, StreamMethod},
    types::Deprecation,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Class {
    pub name: String,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
    pub constructors: Vec<Constructor>,
    pub methods: Vec<Method>,
    pub streams: Vec<StreamMethod>,
    pub async_iterators: Vec<AsyncIteratorMethod>,
}

impl Class {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            doc: None,
            deprecated: None,
            constructors: Vec::new(),
            methods: Vec::new(),
            streams: Vec::new(),
            async_iterators: Vec::new(),
        }
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

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn with_constructor(mut self, constructor: Constructor) -> Self {
        self.constructors.push(constructor);
        self
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.methods.push(method);
        self
    }

    pub fn with_stream(mut self, stream: StreamMethod) -> Self {
        self.streams.push(stream);
        self
    }

    pub fn with_async_iterator(mut self, iter: AsyncIteratorMethod) -> Self {
        self.async_iterators.push(iter);
        self
    }

    pub fn has_constructors(&self) -> bool {
        !self.constructors.is_empty()
    }

    pub fn default_constructor(&self) -> Option<&Constructor> {
        self.constructors.iter().find(|ctor| ctor.inputs.is_empty())
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constructor {
    pub name: String,
    #[serde(default)]
    pub is_fallible: bool,
    pub inputs: Vec<ConstructorParam>,
    pub doc: Option<String>,
}

impl Constructor {
    pub fn new() -> Self {
        Self {
            name: "new".to_string(),
            is_fallible: false,
            inputs: Vec::new(),
            doc: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_fallible(mut self, is_fallible: bool) -> Self {
        self.is_fallible = is_fallible;
        self
    }

    pub fn with_param(mut self, param: ConstructorParam) -> Self {
        self.inputs.push(param);
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

    pub fn is_default(&self) -> bool {
        self.name == "new"
    }
}

impl Default for Constructor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructorParam {
    pub name: String,
    pub param_type: super::types::Type,
}

impl ConstructorParam {
    pub fn new(name: impl Into<String>, param_type: super::types::Type) -> Self {
        Self { name: name.into(), param_type }
    }
}
