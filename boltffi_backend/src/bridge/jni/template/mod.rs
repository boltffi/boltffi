//! Askama rendering for the generated JNI C source.
//!
//! The contract layer decides what exists and what each piece means. This layer
//! turns that contract into source-shaped template data: declarations, local
//! variables, JNI calls, cleanup labels, and return expressions. Large generated
//! C syntax stays in Askama templates. Rust prepares typed values before they
//! reach those templates.
//!
//! This split keeps rendering honest. Template views may format a callback
//! method, stream helper, closure trampoline, or native method, but they do not
//! decide whether a value is encoded, direct, async, fallible, or borrowed.
//! Those decisions already live in the JNI contract. If a template view needs a
//! new fact, the contract should expose that fact as a typed value rather than
//! letting the template layer infer it from strings.

mod borrow_wrapper;
mod callback;
mod closure;
mod method;
mod source;
mod stream;

pub use self::source::SourceFile;
