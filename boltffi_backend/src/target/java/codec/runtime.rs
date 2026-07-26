use askama::Template as AskamaTemplate;
use boltffi_binding::CanonicalName;

use crate::core::{AuxChunk, HelperId, Result, TextChunk};

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/wire.java", escape = "none")]
struct RuntimeTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/direct_vector.java", escape = "none")]
struct DirectVectorTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/async.java", escape = "none")]
struct AsyncTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/async_callback.java", escape = "none")]
struct AsyncCallbackTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/callback_failure.java", escape = "none")]
struct CallbackFailureTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/callback_future.java", escape = "none")]
struct CallbackFutureTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/stream.java", escape = "none")]
struct StreamTemplate {
    flow: bool,
}

pub struct Runtime;

impl Runtime {
    pub fn helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_wire_runtime")),
            text: TextChunk::new(RuntimeTemplate.render()?),
        })
    }

    pub fn direct_vector_helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_direct_vector_runtime")),
            text: TextChunk::new(DirectVectorTemplate.render()?),
        })
    }

    pub fn async_helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_async_runtime")),
            text: TextChunk::new(AsyncTemplate.render()?),
        })
    }

    pub fn async_callback() -> Result<AuxChunk> {
        Ok(AuxChunk::ForwardDecl(TextChunk::new(
            AsyncCallbackTemplate.render()?,
        )))
    }

    pub fn callback_failure_helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_callback_failure_runtime")),
            text: TextChunk::new(CallbackFailureTemplate.render()?),
        })
    }

    pub fn callback_future_helper() -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_callback_future_runtime")),
            text: TextChunk::new(CallbackFutureTemplate.render()?),
        })
    }

    pub fn stream_helper(flow: bool) -> Result<AuxChunk> {
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("java_stream_runtime")),
            text: TextChunk::new(StreamTemplate { flow }.render()?),
        })
    }
}
