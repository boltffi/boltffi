//! Ruby target (experimental).
//!
//! Each Ruby runtime has its own host submodule. `cext` is the host for a C
//! extension that calls the C ABI through the Ruby C API. The naming rules and
//! the Ruby syntax fragments serve every runtime.

mod cext;
/// Ruby spellings of binding names.
pub mod name_style;
/// Ruby syntax fragments.
pub mod syntax;

pub use cext::RubyCExtHost;
