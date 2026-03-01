use super::types::{Deprecation, Type};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncIteratorMethod {
    pub name: String,
    pub item_type: Type,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl AsyncIteratorMethod {
    pub fn new(name: impl Into<String>, item_type: Type) -> Self {
        Self {
            name: name.into(),
            item_type,
            doc: None,
            deprecated: None,
        }
    }

    pub fn maybe_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamMode {
    #[default]
    Async,
    Batch,
    Callback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMethod {
    pub name: String,
    pub item_type: Type,
    pub mode: StreamMode,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
}

impl StreamMethod {
    pub fn new(name: impl Into<String>, item_type: Type) -> Self {
        Self {
            name: name.into(),
            item_type,
            mode: StreamMode::default(),
            doc: None,
            deprecated: None,
        }
    }

    pub fn with_mode(mut self, mode: StreamMode) -> Self {
        self.mode = mode;
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

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }
}
