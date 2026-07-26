use askama::Template as AskamaTemplate;
use boltffi_binding::CanonicalName;

use crate::core::{AuxChunk, HelperId, Result, TextChunk};

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/value_identity.java", escape = "none")]
struct IdentityTemplate;

pub struct ValueIdentity;

impl ValueIdentity {
    pub fn helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_value_identity")),
            text: TextChunk::new(IdentityTemplate.render()?),
        })
    }
}
