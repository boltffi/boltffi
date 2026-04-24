//! Pre-rendered C# code snippets. Two newtypes that differ only in
//! how the generator uses them: expressions go in contexts that expect
//! a value (right-hand side of an assignment, argument of a call);
//! statements are effects (a call with side effects, a control flow
//! element). Same string contents, different semantic slots.
//!
//! The lowerer builds these at plan-construction time so templates can
//! paste them verbatim without inspecting or rewriting. The trait
//! [`CSharpToken`] gives templates a uniform surface (`as_str` /
//! `emit`) across both kinds.
//!
//! One step up from opaque snippets, [`CSharpLocalDecl`] captures the
//! structure of a local variable declaration
//! (`{type} {name} = {rhs};`) without pulling the RHS apart — the
//! middle ground between free-form strings and a full C# AST.

use std::fmt;

use super::{CSharpLocalName, CSharpType};

/// A pre-rendered C# source snippet. Implementors are produced by the
/// lowerer and consumed by templates.
pub trait CSharpToken: fmt::Display {
    fn as_str(&self) -> &str;

    /// Alias for [`Self::as_str`]. Read as "the C# code this token
    /// emits." Templates may call either — the intent is to let the
    /// generator's calling site pick the name that reads best in
    /// context.
    fn emit(&self) -> &str {
        self.as_str()
    }
}

/// A C# expression — evaluates to a value, no trailing semicolon.
/// Examples: `"reader.ReadF64()"`, `"(int?)null"`, `"this.Name"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpExpression(String);

impl CSharpExpression {
    /// Wraps an already-rendered C# expression. The lowerer is trusted
    /// to have produced syntactically valid C# — no validation here.
    pub fn new(rendered: String) -> Self {
        Self(rendered)
    }
}

impl CSharpToken for CSharpExpression {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A C# statement — an effect, may or may not carry a trailing
/// semicolon depending on the template context. Examples:
/// `"wire.WriteF64(this.X)"`, `"byte[] _vBytes = Encoding.UTF8.GetBytes(v);"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpStatement(String);

impl CSharpStatement {
    /// Wraps an already-rendered C# statement. The lowerer is trusted
    /// to have produced syntactically valid C# — no validation here.
    pub fn new(rendered: String) -> Self {
        Self(rendered)
    }
}

impl CSharpToken for CSharpStatement {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A C# local variable declaration with initializer, rendered as
/// `{declared_type} {name} = {rhs};`. Structural: each piece is
/// typed. The RHS expression stays opaque — not modeling the full C#
/// expression grammar.
#[derive(Debug, Clone)]
pub struct CSharpLocalDecl {
    pub declared_type: CSharpType,
    pub name: CSharpLocalName,
    pub rhs: CSharpExpression,
}

impl fmt::Display for CSharpLocalDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} = {};", self.declared_type, self.name, self.rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_round_trips_as_str() {
        let expr = CSharpExpression::new("reader.ReadF64()".to_string());
        assert_eq!(expr.as_str(), "reader.ReadF64()");
        assert_eq!(expr.to_string(), "reader.ReadF64()");
        assert_eq!(expr.emit(), "reader.ReadF64()");
    }

    #[test]
    fn statement_round_trips_as_str() {
        let stmt = CSharpStatement::new("wire.WriteF64(this.X)".to_string());
        assert_eq!(stmt.as_str(), "wire.WriteF64(this.X)");
        assert_eq!(stmt.to_string(), "wire.WriteF64(this.X)");
        assert_eq!(stmt.emit(), "wire.WriteF64(this.X)");
    }

    #[test]
    fn local_decl_renders_as_typed_assignment() {
        let param = super::super::CSharpParamName::from_source("v");
        let decl = CSharpLocalDecl {
            declared_type: CSharpType::Array(Box::new(CSharpType::Byte)),
            name: CSharpLocalName::for_bytes(&param),
            rhs: CSharpExpression::new("Encoding.UTF8.GetBytes(v)".to_string()),
        };
        assert_eq!(
            decl.to_string(),
            "byte[] _vBytes = Encoding.UTF8.GetBytes(v);"
        );
    }
}
