mod loader;
#[cfg(test)]
mod tests;

use crate::target::jvm::NativeLibraries;

use super::syntax::Identifier;

#[derive(Clone, Debug)]
pub struct Loader<'libraries> {
    owner: Identifier,
    runtime_owner: Identifier,
    libraries: &'libraries NativeLibraries,
}
