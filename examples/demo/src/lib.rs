#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod async_fns;
pub mod builtins;
pub mod bytes;
pub mod callbacks;
pub mod classes;
pub mod collections;
pub mod constants;
pub mod custom_types;
pub mod enums;
pub mod multicrate;
pub mod options;
pub mod primitives;
pub mod records;
pub mod results;
#[cfg(feature = "wasm-bench")]
pub mod wasm_bench;

pub use async_fns::*;
pub use builtins::*;
pub use bytes::*;
pub use callbacks::*;
pub use classes::*;
pub use collections::*;
pub use constants::*;
pub use custom_types::*;
pub use enums::*;
pub use multicrate::*;
pub use options::*;
pub use primitives::*;
pub use records::*;
pub use results::*;
