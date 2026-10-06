//! Which lowered shapes the Ruby target can move.

use crate::core::{Error, Result};

/// Returns the typed error a renderer uses to skip one declaration.
pub fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: "ruby",
        shape,
    })
}
