//! Ergonomic C rendering for the sync value surface.
//!
//! The C host renders directly over the C ABI contract (`CBridgeContract`).
//! Every renderer emits a `static inline` wrapper (and/or a `typedef`) into the
//! same header the bridge produced, so the ergonomic API lines up with the
//! already-locked ABI.

pub mod callable;
pub mod callback;
pub mod class;
pub mod constant;
pub mod enumeration;
pub mod function;
pub mod prefix;
pub mod record;
pub mod result;
pub mod surface;
pub mod wrapper;
