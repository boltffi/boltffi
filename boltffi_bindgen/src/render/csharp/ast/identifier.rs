//! C# identifier vocabulary, with one newtype per role (class name,
//! method name, parameter name, etc.) so call sites can't cross
//! categories.

use std::collections::HashSet;
use std::fmt;

use boltffi_ffi_rules::naming;

use crate::ir::ids::{EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId, VariantName};

/// A C# class, struct, or record name in PascalCase.
///
/// Examples:
/// ```csharp
/// Point
/// Status
/// MyRecord
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpClassName(String);

impl CSharpClassName {
    /// Builds from a snake_case source name. `my_record` → `"MyRecord"`.
    pub(crate) fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    /// Wraps a pre-formed PascalCase class name. Used for runtime-
    /// library or built-in type references whose C# spelling is fixed
    /// (`Encoding`, `WireWriter`, `WireReader`) and so doesn't round-
    /// trip through the snake_case convention.
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// `{base}Wire`: the companion static class hosting a C-style
    /// enum's wire codec helpers.
    pub(crate) fn wire_helper(base: &CSharpClassName) -> Self {
        Self(format!("{}Wire", base.0))
    }

    /// `{base}Methods`: the companion static class hosting a C-style
    /// enum's methods, since C# enums can't carry members directly.
    pub(crate) fn methods_companion(base: &CSharpClassName) -> Self {
        Self(format!("{}Methods", base.0))
    }

    /// `{base}Exception`: the generated exception class wrapping a
    /// `#[error]` enum or record on the throwing-method path. The
    /// trailing `Exception` keeps the type discoverable in IDE catch
    /// lists and avoids colliding with the underlying value type.
    pub(crate) fn exception_for(base: &CSharpClassName) -> Self {
        Self(format!("{}Exception", base.0))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpClassName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&RecordId> for CSharpClassName {
    fn from(id: &RecordId) -> Self {
        Self::from_source(id.as_str())
    }
}

impl From<&EnumId> for CSharpClassName {
    fn from(id: &EnumId) -> Self {
        Self::from_source(id.as_str())
    }
}

impl From<&VariantName> for CSharpClassName {
    fn from(name: &VariantName) -> Self {
        Self::from_source(name.as_str())
    }
}

/// A reference to a user-defined C# type, either by its bare name or
/// fully qualified through the `global::` namespace alias.
///
/// Examples:
/// ```csharp
/// Point
/// global::Demo.Point
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CSharpTypeReference {
    Plain(CSharpClassName),
    Qualified {
        namespace: CSharpNamespace,
        name: CSharpClassName,
    },
}

impl CSharpTypeReference {
    /// Promote `Plain(name)` to the qualified form when `name` is in
    /// `shadowed`. `Qualified` passes through untouched; once a
    /// reference has been qualified, re-qualification is a no-op.
    pub(crate) fn qualify_if_shadowed(
        self,
        shadowed: &HashSet<CSharpClassName>,
        namespace: &CSharpNamespace,
    ) -> Self {
        match self {
            Self::Plain(name) if shadowed.contains(&name) => Self::Qualified {
                namespace: namespace.clone(),
                name,
            },
            other => other,
        }
    }

    /// Apply [`Self::qualify_if_shadowed`] when `shadowed` is `Some`;
    /// pass through when `None`. Mirrors
    /// [`CSharpType::qualify_if_shadowed_opt`](super::CSharpType::qualify_if_shadowed_opt)
    /// for callers that may or may not be inside a shadowing scope.
    pub(crate) fn qualify_if_shadowed_opt(
        self,
        shadowed: Option<&HashSet<CSharpClassName>>,
        namespace: &CSharpNamespace,
    ) -> Self {
        match shadowed {
            Some(sh) => self.qualify_if_shadowed(sh, namespace),
            None => self,
        }
    }
}

impl fmt::Display for CSharpTypeReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(name) => write!(f, "{name}"),
            Self::Qualified { namespace, name } => write!(f, "global::{namespace}.{name}"),
        }
    }
}

impl From<CSharpClassName> for CSharpTypeReference {
    fn from(name: CSharpClassName) -> Self {
        Self::Plain(name)
    }
}

/// A C# method name in PascalCase.
///
/// Examples:
/// ```csharp
/// Decode
/// EchoI32
/// WireEncodeTo
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpMethodName(String);

impl CSharpMethodName {
    /// Builds from a snake_case source name. `do_thing` → `"DoThing"`.
    pub(crate) fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    /// Wraps a pre-formed PascalCase method name. Used for runtime
    /// library methods whose C# spelling doesn't round-trip through
    /// the snake_case convention, typically those embedding an
    /// acronym that resists the usual splitter (e.g.,
    /// `WriteNIntArray` where `NInt` wants a capital `I` in the
    /// middle of a segment).
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// `{owner}{method}`: the DllImport entry name used inside the
    /// shared `NativeMethods` class. Two types may declare methods of
    /// the same name, and the DllImport class is flat, so the owner
    /// class name is prefixed to disambiguate.
    pub(crate) fn native_for_owner(owner: &CSharpClassName, method: &CSharpMethodName) -> Self {
        Self(format!("{}{}", owner.as_str(), method.as_str()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpMethodName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&FunctionId> for CSharpMethodName {
    fn from(id: &FunctionId) -> Self {
        Self::from_source(id.as_str())
    }
}

impl From<&MethodId> for CSharpMethodName {
    fn from(id: &MethodId) -> Self {
        Self::from_source(id.as_str())
    }
}

/// A C# property name in PascalCase.
///
/// Examples:
/// ```csharp
/// X
/// Radius
/// MyProp
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpPropertyName(String);

impl CSharpPropertyName {
    /// Builds from a snake_case source name. `my_prop` → `"MyProp"`.
    pub(crate) fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpPropertyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&FieldName> for CSharpPropertyName {
    fn from(name: &FieldName) -> Self {
        Self::from_source(name.as_str())
    }
}

/// A C# parameter name in camelCase. Reserved keywords are
/// `@`-escaped to make them legal identifiers.
///
/// Examples:
/// ```csharp
/// value
/// myParam
/// @class
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpParamName(String);

impl CSharpParamName {
    /// Builds from a snake_case source name. `my_param` → `"myParam"`.
    pub(crate) fn from_source(source: &str) -> Self {
        Self(escape_if_keyword(naming::snake_to_camel(source)))
    }

    /// Wraps a pre-formed param name. Used to build derived names
    /// without re-running the snake-case transform (e.g.
    /// `"{name}Len"` length params on the DllImport side).
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The param name with any leading `@` escape stripped. Used when
    /// building derived local names so that a `@class` param becomes
    /// `_classBytes` rather than `_@classBytes` (the latter is not a
    /// valid C# identifier).
    fn stripped(&self) -> &str {
        self.0.strip_prefix('@').unwrap_or(&self.0)
    }
}

impl fmt::Display for CSharpParamName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&ParamName> for CSharpParamName {
    fn from(name: &ParamName) -> Self {
        Self::from_source(name.as_str())
    }
}

/// A C# identifier introduced inside a method body. Covers four
/// roles that the C# spec names separately but that share one
/// syntactic position: a local variable, a `foreach` iteration
/// variable, a lambda parameter, and an `is`-pattern variable.
///
/// Examples:
/// ```csharp
/// // Local variable
/// byte[] _personBytes = Encoding.UTF8.GetBytes(person);
/// //     ^^^^^^^^^^^^
///
/// // foreach iteration variable
/// foreach (string item0 in items) { ... }
/// //               ^^^^^
///
/// // Lambda parameter
/// items.Select(r0 => r0.Decode());
/// //           ^^
///
/// // is-pattern variable
/// if (this.Name is { } opt0) { ... }
/// //                   ^^^^
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpLocalName(String);

impl CSharpLocalName {
    /// Wraps a pre-formed local-variable name. Used for fixed-vocabulary
    /// locals that the lowerer doesn't synthesize from a counter or
    /// transform from a source name (`reader`, `wire`, IR-supplied
    /// rebinding placeholders).
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// `_wire_{param}`: the `WireWriter` instance a record param is
    /// encoded into. `@`-escape is stripped from `param` so the
    /// produced name stays a valid C# identifier.
    pub(crate) fn for_wire_writer(param: &CSharpParamName) -> Self {
        Self(format!("_wire_{}", param.stripped()))
    }

    /// `_{param}Bytes`: the `byte[]` holding the encoded payload of a
    /// string or wire-encoded-record param. `@`-escape is stripped
    /// from `param` for the same reason.
    pub(crate) fn for_bytes(param: &CSharpParamName) -> Self {
        Self(format!("_{}Bytes", param.stripped()))
    }

    /// `_{param}Ptr`: the pointer local introduced by the `fixed`
    /// statement that pins a `PinnedArray` param. `@`-escape is
    /// stripped for the same identifier-validity reason as
    /// [`Self::for_bytes`].
    pub(crate) fn for_pinned_ptr(param: &CSharpParamName) -> Self {
        Self(format!("_{}Ptr", param.stripped()))
    }

    /// `sizeOpt{n}`: the pattern binding introduced inside a
    /// `SizeExpr::OptionSize` ternary so the option's non-null payload
    /// can be referenced while summing its byte size. The prefix is
    /// distinct from the write-side `opt{n}` because pattern variables
    /// leak into the enclosing method scope and the size ternary plus
    /// the write `if` statement coexist in the same method body.
    pub(crate) fn size_option_binding(n: usize) -> Self {
        Self(format!("sizeOpt{n}"))
    }

    /// `sizeItem{n}`: the per-iteration loop variable inside
    /// `WireWriter.EncodedArraySize`'s size-lambda. Distinct from the
    /// write-side `item{n}` for the same method-scope reason.
    pub(crate) fn size_loop_var(n: usize) -> Self {
        Self(format!("sizeItem{n}"))
    }

    /// `opt{n}`: the pattern binding introduced inside the encode-
    /// phase `if` statement that writes a `WriteOp::Option`. Distinct
    /// from the size-phase `sizeOpt{n}` so the two emissions can
    /// coexist in one method body without redeclaring the same local.
    pub(crate) fn encode_option_binding(n: usize) -> Self {
        Self(format!("opt{n}"))
    }

    /// `item{n}`: the per-iteration loop variable inside the encode-
    /// phase `foreach` block for an encoded vec. Distinct from the
    /// size-phase `sizeItem{n}` for the same method-scope reason.
    pub(crate) fn encode_loop_var(n: usize) -> Self {
        Self(format!("item{n}"))
    }

    /// `r{n}`: the lambda parameter inside the decode-phase
    /// `ReadEncodedArray<T>(r{n} => ...)` call. Each nested encoded
    /// vec introduces its own lambda, so siblings need distinct
    /// counter values to avoid shadowing inside the enclosing method
    /// body.
    pub(crate) fn decode_closure_var(n: usize) -> Self {
        Self(format!("r{n}"))
    }
}

impl fmt::Display for CSharpLocalName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A C# namespace name in PascalCase, used after the `namespace`
/// keyword.
///
/// Examples:
/// ```csharp
/// namespace DemoLib { ... }
/// namespace MyCompany.Generated { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CSharpNamespace(String);

impl CSharpNamespace {
    /// Builds from a snake_case source name (typically the crate name).
    /// `demo_lib` → `"DemoLib"`.
    pub(crate) fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }
}

impl fmt::Display for CSharpNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn escape_if_keyword(name: String) -> String {
    if is_csharp_keyword(&name) {
        format!("@{}", name)
    } else {
        name
    }
}

fn is_csharp_keyword(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "as"
            | "base"
            | "bool"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "checked"
            | "class"
            | "const"
            | "continue"
            | "decimal"
            | "default"
            | "delegate"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "event"
            | "explicit"
            | "extern"
            | "false"
            | "finally"
            | "fixed"
            | "float"
            | "for"
            | "foreach"
            | "goto"
            | "if"
            | "implicit"
            | "in"
            | "int"
            | "interface"
            | "internal"
            | "is"
            | "lock"
            | "long"
            | "namespace"
            | "new"
            | "null"
            | "object"
            | "operator"
            | "out"
            | "override"
            | "params"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "ref"
            | "return"
            | "sbyte"
            | "sealed"
            | "short"
            | "sizeof"
            | "stackalloc"
            | "static"
            | "string"
            | "struct"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "uint"
            | "ulong"
            | "unchecked"
            | "unsafe"
            | "ushort"
            | "using"
            | "virtual"
            | "void"
            | "volatile"
            | "while"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csharp_class_name_from_snake_case_produces_pascal_case() {
        let name = CSharpClassName::from_source("my_record");
        assert_eq!(name.as_str(), "MyRecord");
        assert_eq!(name.to_string(), "MyRecord");
    }

    #[test]
    fn csharp_class_name_wire_helper_appends_wire_suffix() {
        let base = CSharpClassName::from_source("status");
        let helper = CSharpClassName::wire_helper(&base);
        assert_eq!(helper.as_str(), "StatusWire");
    }

    #[test]
    fn csharp_class_name_methods_companion_appends_methods_suffix() {
        let base = CSharpClassName::from_source("log_level");
        let companion = CSharpClassName::methods_companion(&base);
        assert_eq!(companion.as_str(), "LogLevelMethods");
    }

    #[test]
    fn csharp_class_name_from_record_id_converts_through_source() {
        let id = RecordId::new("point");
        let name: CSharpClassName = (&id).into();
        assert_eq!(name.as_str(), "Point");
    }

    /// Typed exception classes are named `<Name>Exception` so they
    /// stay discoverable in IDE catch lists alongside other
    /// `*Exception` types and don't collide with the underlying value
    /// type.
    #[test]
    fn csharp_class_name_exception_for_appends_exception_suffix() {
        let base = CSharpClassName::from_source("math_error");
        let exception = CSharpClassName::exception_for(&base);
        assert_eq!(exception.as_str(), "MathErrorException");
    }

    #[test]
    fn csharp_method_name_from_snake_case_produces_pascal_case() {
        let name = CSharpMethodName::from_source("do_thing");
        assert_eq!(name.as_str(), "DoThing");
    }

    #[test]
    fn csharp_method_name_from_function_id_converts_through_source() {
        let id = FunctionId::new("echo_i32");
        let name: CSharpMethodName = (&id).into();
        assert_eq!(name.as_str(), "EchoI32");
    }

    #[test]
    fn csharp_property_name_from_snake_case_produces_pascal_case() {
        let name = CSharpPropertyName::from_source("my_prop");
        assert_eq!(name.to_string(), "MyProp");
    }

    #[test]
    fn csharp_property_name_from_field_name_converts_through_source() {
        let field = FieldName::new("radius");
        let name: CSharpPropertyName = (&field).into();
        assert_eq!(name.to_string(), "Radius");
    }

    #[test]
    fn csharp_param_name_from_snake_case_produces_camel_case() {
        let name = CSharpParamName::from_source("my_param");
        assert_eq!(name.as_str(), "myParam");
    }

    #[test]
    fn csharp_param_name_escapes_csharp_keyword() {
        let name = CSharpParamName::from_source("class");
        assert_eq!(name.as_str(), "@class");
    }

    #[test]
    fn csharp_param_name_from_ir_param_name_converts_through_source() {
        let param = ParamName::new("my_param");
        let name: CSharpParamName = (&param).into();
        assert_eq!(name.as_str(), "myParam");
    }

    #[test]
    fn csharp_local_name_for_wire_writer_prefixes_param() {
        let param = CSharpParamName::from_source("point");
        let local = CSharpLocalName::for_wire_writer(&param);
        assert_eq!(local.to_string(), "_wire_point");
    }

    /// A `@`-escaped param name must not carry the `@` into a derived
    /// local: `_@classBytes` is not a valid C# identifier, but
    /// `_classBytes` is.
    #[test]
    fn csharp_local_name_for_bytes_strips_keyword_escape() {
        let param = CSharpParamName::from_source("class");
        let local = CSharpLocalName::for_bytes(&param);
        assert_eq!(local.to_string(), "_classBytes");
    }

    #[test]
    fn csharp_local_name_for_bytes_uses_param_directly_when_not_escaped() {
        let param = CSharpParamName::from_source("value");
        let local = CSharpLocalName::for_bytes(&param);
        assert_eq!(local.to_string(), "_valueBytes");
    }

    #[rstest::rstest]
    #[case::first(0, "sizeOpt0")]
    #[case::second(1, "sizeOpt1")]
    #[case::later(3, "sizeOpt3")]
    fn csharp_local_name_size_option_binding_uses_sizeopt_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(
            CSharpLocalName::size_option_binding(n).to_string(),
            expected
        );
    }

    #[rstest::rstest]
    #[case::first(0, "sizeItem0")]
    #[case::second(1, "sizeItem1")]
    #[case::later(2, "sizeItem2")]
    fn csharp_local_name_size_loop_var_uses_sizeitem_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::size_loop_var(n).to_string(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "opt0")]
    #[case::second(1, "opt1")]
    #[case::later(5, "opt5")]
    fn csharp_local_name_encode_option_binding_uses_opt_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(
            CSharpLocalName::encode_option_binding(n).to_string(),
            expected
        );
    }

    #[rstest::rstest]
    #[case::first(0, "item0")]
    #[case::second(1, "item1")]
    #[case::later(4, "item4")]
    fn csharp_local_name_encode_loop_var_uses_item_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::encode_loop_var(n).to_string(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "r0")]
    #[case::second(1, "r1")]
    #[case::later(3, "r3")]
    fn csharp_local_name_decode_closure_var_uses_r_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::decode_closure_var(n).to_string(), expected);
    }

    #[test]
    fn csharp_method_name_new_wraps_pre_formed_name_verbatim() {
        assert_eq!(
            CSharpMethodName::new("WriteNIntArray").as_str(),
            "WriteNIntArray"
        );
    }

    #[test]
    fn csharp_namespace_from_snake_case_produces_pascal_case() {
        let ns = CSharpNamespace::from_source("demo_lib");
        assert_eq!(ns.to_string(), "DemoLib");
    }

    mod csharp_type_reference {
        use super::*;

        fn shadowed(names: &[&str]) -> HashSet<CSharpClassName> {
            names
                .iter()
                .map(|n| CSharpClassName::from_source(n))
                .collect()
        }

        #[test]
        fn plain_display_uses_bare_class_name() {
            let r = CSharpTypeReference::Plain(CSharpClassName::from_source("point"));
            assert_eq!(r.to_string(), "Point");
        }

        #[test]
        fn qualified_display_uses_global_prefix() {
            let r = CSharpTypeReference::Qualified {
                namespace: CSharpNamespace::from_source("demo"),
                name: CSharpClassName::from_source("point"),
            };
            assert_eq!(r.to_string(), "global::Demo.Point");
        }

        #[test]
        fn qualify_promotes_plain_when_name_is_in_shadowed_set() {
            let r: CSharpTypeReference = CSharpClassName::from_source("point").into();
            let ns = CSharpNamespace::from_source("demo");
            let qualified = r.qualify_if_shadowed(&shadowed(&["point"]), &ns);
            assert_eq!(qualified.to_string(), "global::Demo.Point");
        }

        #[test]
        fn qualify_leaves_plain_when_name_is_not_shadowed() {
            let r: CSharpTypeReference = CSharpClassName::from_source("point").into();
            let ns = CSharpNamespace::from_source("demo");
            let qualified = r.qualify_if_shadowed(&shadowed(&["circle"]), &ns);
            assert_eq!(qualified.to_string(), "Point");
        }

        /// A reference already in qualified form does not get re-qualified
        /// when its class name happens to be in the shadow set. Qualified
        /// is terminal.
        #[test]
        fn qualify_is_a_no_op_on_already_qualified_reference() {
            let r = CSharpTypeReference::Qualified {
                namespace: CSharpNamespace::from_source("demo"),
                name: CSharpClassName::from_source("point"),
            };
            let ns = CSharpNamespace::from_source("demo");
            let qualified = r.qualify_if_shadowed(&shadowed(&["point"]), &ns);
            assert_eq!(qualified.to_string(), "global::Demo.Point");
        }

        #[test]
        fn from_csharp_class_name_produces_plain_variant() {
            let name = CSharpClassName::from_source("point");
            let r: CSharpTypeReference = name.into();
            assert!(matches!(r, CSharpTypeReference::Plain(_)));
        }
    }
}
