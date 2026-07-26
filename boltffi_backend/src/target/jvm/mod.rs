//! Shared JVM target configuration.

mod library;
pub(crate) mod method;
pub(crate) mod resource;

pub use library::{DesktopLoader, LibraryName, NativeLibraries};
