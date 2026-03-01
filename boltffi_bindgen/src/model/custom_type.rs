use super::types::Type;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomType {
    pub name: String,
    pub repr: Type,
}

impl CustomType {
    pub fn new(name: impl Into<String>, repr: Type) -> Self {
        Self { name: name.into(), repr }
    }
}
