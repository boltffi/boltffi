#![allow(improper_ctypes_definitions)]
#![allow(clippy::unused_unit)]
#![allow(clippy::too_many_arguments)]

use boltffi::*;

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FixturePoint {
    pub x: f64,
    pub y: f64,
}

pub use __boltffi_ir_expansion::*;

mod asynchronous;
mod bytes;
mod callbacks;
mod classes;
mod closures;
mod collections;
mod constants;
pub mod contract;
mod customs;
mod enums;
mod options;
mod primitives;
#[cfg(boltffi_pending_closure_return)]
mod quarantine;
mod records_direct;
mod records_encoded;
mod results;
mod streams;
mod strings;
mod vectors;

pub use asynchronous::*;
pub use bytes::*;
pub use callbacks::*;
pub use classes::*;
pub use closures::*;
pub use collections::*;
pub use constants::*;
pub use customs::*;
pub use enums::*;
pub use options::*;
pub use primitives::*;
#[cfg(boltffi_pending_closure_return)]
pub use quarantine::*;
pub use records_direct::*;
pub use records_encoded::*;
pub use results::*;
pub use streams::*;
pub use strings::*;
pub use vectors::*;
