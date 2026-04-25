//! Lowers the language-agnostic IR (`FfiContract` + `AbiContract`) into
//! a [`CSharpModulePlan`](super::plan::CSharpModulePlan) that the C#
//! templates render.

mod abi;
mod admission;
mod decode;
mod encode;
mod enumerations;
mod functions;
mod lowerer;
mod predicates;
mod prefix;
mod records;
mod size;
#[cfg(test)]
mod test_support;
mod types;
mod value;
mod wire_writers;

pub use lowerer::CSharpLowerer;

#[cfg(test)]
pub(super) use wire_writers::self_wire_writer;
