//! Typed identifier vocabulary for the C# backend. One newtype per
//! role so the plan captures what each string actually is (a class
//! name vs. a method name vs. a local binding), and the compiler
//! catches category errors at construction sites.
//!
//! Absorbs the logic that used to live in `names.rs`: the
//! source-to-C# transformation (snake_case → PascalCase / camelCase)
//! and the `@`-escape for C# keyword collisions. The distinction
//! between C# types (prefixed `CSharp*`) and C-side types (prefixed
//! `C*`) at the type-name level tells the reader which side of the
//! ABI boundary each value sits on.

use std::collections::HashSet;
use std::fmt;

use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::naming::{GlobalSymbol, Name};

use crate::ir::ids::{EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId, VariantName};

/// A C# class or struct name (e.g., `"Point"`, `"Shape"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpClassName(String);

impl CSharpClassName {
    /// Builds from a snake_case source name. `my_record` → `"MyRecord"`.
    pub fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    /// `{base}Wire`: the companion static class hosting a C-style
    /// enum's wire codec helpers.
    pub fn wire_helper(base: &CSharpClassName) -> Self {
        Self(format!("{}Wire", base.0))
    }

    /// `{base}Methods`: the companion static class hosting a C-style
    /// enum's methods, since C# enums can't carry members directly.
    pub fn methods_companion(base: &CSharpClassName) -> Self {
        Self(format!("{}Methods", base.0))
    }

    pub fn as_str(&self) -> &str {
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

/// A reference to a user-defined C# class, record, or enum at a render
/// site. Either bare (`Plain("Point")` → emits `"Point"`) or fully
/// qualified through the global namespace
/// (`Qualified { Demo, Point }` → emits `"global::Demo.Point"`).
///
/// Qualified is used when the bare name would resolve wrong at the
/// render site. Today that's inside a data enum's body, where nested
/// `sealed record` variants shadow module-level types of the same
/// name. [`Self::qualify_if_shadowed`] is the one decision point that
/// switches a `Plain` into a `Qualified`; plain is the default and
/// flows through every other path unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CSharpTypeReference {
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
    pub fn qualify_if_shadowed(
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

    /// The underlying class name, stripped of any qualification.
    /// Callers that care about identity (rather than render form) go
    /// through here.
    pub fn class_name(&self) -> &CSharpClassName {
        match self {
            Self::Plain(name) | Self::Qualified { name, .. } => name,
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

/// A C# method identifier (e.g., `"EchoI32"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpMethodName(String);

impl CSharpMethodName {
    /// Builds from a snake_case source name. `do_thing` → `"DoThing"`.
    pub fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    /// Wraps a pre-formed PascalCase method name. Used for runtime
    /// library methods whose C# spelling doesn't round-trip through
    /// the snake_case convention, typically those embedding an
    /// acronym that resists the usual splitter (e.g.,
    /// `WriteNIntArray` where `NInt` wants a capital `I` in the
    /// middle of a segment).
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// `{owner}{method}`: the DllImport entry name used inside the
    /// shared `NativeMethods` class. Two types may declare methods of
    /// the same name, and the DllImport class is flat, so the owner
    /// class name is prefixed to disambiguate.
    pub fn native_for_owner(owner: &CSharpClassName, method: &CSharpMethodName) -> Self {
        Self(format!("{}{}", owner.as_str(), method.as_str()))
    }

    pub fn as_str(&self) -> &str {
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

/// A C# property identifier on a record (e.g., `"X"`, `"Radius"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpPropertyName(String);

impl CSharpPropertyName {
    /// Builds from a snake_case source name. `my_prop` → `"MyProp"`.
    pub fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    pub fn as_str(&self) -> &str {
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

/// A C# parameter name as it appears in the public wrapper signature
/// (e.g., `"value"`, `"@class"`). `@`-escaped when the transformed
/// form collides with a reserved keyword.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpParamName(String);

impl CSharpParamName {
    /// Builds from a snake_case source name. `my_param` → `"myParam"`.
    pub fn from_source(source: &str) -> Self {
        Self(escape_if_keyword(naming::snake_to_camel(source)))
    }

    pub fn as_str(&self) -> &str {
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

/// A C# local variable synthesized by the lowerer
/// (e.g., `"_personBytes"`, `"_wire_point"`). Not derived from a user
/// source; constructed by the lowerer according to fixed conventions
/// for each role.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpLocalName(String);

impl CSharpLocalName {
    /// `_wire_{param}`: the `WireWriter` instance a record param is
    /// encoded into. `@`-escape is stripped from `param` so the
    /// produced name stays a valid C# identifier.
    pub fn for_wire_writer(param: &CSharpParamName) -> Self {
        Self(format!("_wire_{}", param.stripped()))
    }

    /// `_{param}Bytes`: the `byte[]` holding the encoded payload of a
    /// string or wire-encoded-record param. `@`-escape is stripped
    /// from `param` for the same reason.
    pub fn for_bytes(param: &CSharpParamName) -> Self {
        Self(format!("_{}Bytes", param.stripped()))
    }

    /// `sizeOpt{n}`: the pattern binding introduced inside a
    /// `SizeExpr::OptionSize` ternary so the option's non-null payload
    /// can be referenced while summing its byte size. The prefix is
    /// distinct from the write-side `opt{n}` because pattern variables
    /// leak into the enclosing method scope and the size ternary plus
    /// the write `if` statement coexist in the same method body.
    pub fn size_option_binding(n: usize) -> Self {
        Self(format!("sizeOpt{n}"))
    }

    /// `sizeItem{n}`: the per-iteration loop variable inside
    /// `WireWriter.EncodedArraySize`'s size-lambda. Distinct from the
    /// write-side `item{n}` for the same method-scope reason.
    pub fn size_loop_var(n: usize) -> Self {
        Self(format!("sizeItem{n}"))
    }

    /// `opt{n}`: the pattern binding introduced inside the encode-
    /// phase `if` statement that writes a `WriteOp::Option`. Distinct
    /// from the size-phase `sizeOpt{n}` so the two emissions can
    /// coexist in one method body without redeclaring the same local.
    pub fn encode_option_binding(n: usize) -> Self {
        Self(format!("opt{n}"))
    }

    /// `item{n}`: the per-iteration loop variable inside the encode-
    /// phase `foreach` block for an encoded vec. Distinct from the
    /// size-phase `sizeItem{n}` for the same method-scope reason.
    pub fn encode_loop_var(n: usize) -> Self {
        Self(format!("item{n}"))
    }

    /// `r{n}`: the lambda parameter inside the decode-phase
    /// `ReadEncodedArray<T>(r{n} => ...)` call. Each nested encoded
    /// vec introduces its own lambda, so siblings need distinct
    /// counter values to avoid shadowing inside the enclosing method
    /// body.
    pub fn decode_closure_var(n: usize) -> Self {
        Self(format!("r{n}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpLocalName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A C# namespace (e.g., `"DemoLib"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CSharpNamespace(String);

impl CSharpNamespace {
    /// Builds from a snake_case source name (typically the crate name).
    /// `demo_lib` → `"DemoLib"`.
    pub fn from_source(source: &str) -> Self {
        Self(naming::to_upper_camel_case(source))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CSharpNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of a C function exported from the native library
/// (e.g., `"boltffi_echo_i32"`, `"boltffi_free_buf"`). Goes inside a
/// `DllImport` `EntryPoint` attribute. The lowerer constructs the
/// complete symbol using the `naming::*` helpers and wraps it with
/// [`Self::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CFunctionName(String);

impl CFunctionName {
    pub fn new(name: String) -> Self {
        Self(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CFunctionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Name<GlobalSymbol>> for CFunctionName {
    fn from(symbol: Name<GlobalSymbol>) -> Self {
        Self(symbol.into_string())
    }
}

impl From<&Name<GlobalSymbol>> for CFunctionName {
    fn from(symbol: &Name<GlobalSymbol>) -> Self {
        Self(symbol.as_str().to_string())
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
        assert_eq!(name.as_str(), "MyProp");
    }

    #[test]
    fn csharp_property_name_from_field_name_converts_through_source() {
        let field = FieldName::new("radius");
        let name: CSharpPropertyName = (&field).into();
        assert_eq!(name.as_str(), "Radius");
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
        assert_eq!(local.as_str(), "_wire_point");
    }

    /// A `@`-escaped param name must not carry the `@` into a derived
    /// local: `_@classBytes` is not a valid C# identifier, but
    /// `_classBytes` is.
    #[test]
    fn csharp_local_name_for_bytes_strips_keyword_escape() {
        let param = CSharpParamName::from_source("class");
        let local = CSharpLocalName::for_bytes(&param);
        assert_eq!(local.as_str(), "_classBytes");
    }

    #[test]
    fn csharp_local_name_for_bytes_uses_param_directly_when_not_escaped() {
        let param = CSharpParamName::from_source("value");
        let local = CSharpLocalName::for_bytes(&param);
        assert_eq!(local.as_str(), "_valueBytes");
    }

    #[rstest::rstest]
    #[case::first(0, "sizeOpt0")]
    #[case::second(1, "sizeOpt1")]
    #[case::later(3, "sizeOpt3")]
    fn csharp_local_name_size_option_binding_uses_sizeopt_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::size_option_binding(n).as_str(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "sizeItem0")]
    #[case::second(1, "sizeItem1")]
    #[case::later(2, "sizeItem2")]
    fn csharp_local_name_size_loop_var_uses_sizeitem_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::size_loop_var(n).as_str(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "opt0")]
    #[case::second(1, "opt1")]
    #[case::later(5, "opt5")]
    fn csharp_local_name_encode_option_binding_uses_opt_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::encode_option_binding(n).as_str(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "item0")]
    #[case::second(1, "item1")]
    #[case::later(4, "item4")]
    fn csharp_local_name_encode_loop_var_uses_item_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::encode_loop_var(n).as_str(), expected);
    }

    #[rstest::rstest]
    #[case::first(0, "r0")]
    #[case::second(1, "r1")]
    #[case::later(3, "r3")]
    fn csharp_local_name_decode_closure_var_uses_r_prefix(
        #[case] n: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(CSharpLocalName::decode_closure_var(n).as_str(), expected);
    }

    #[test]
    fn csharp_method_name_new_wraps_pre_formed_name_verbatim() {
        assert_eq!(CSharpMethodName::new("WriteNIntArray").as_str(), "WriteNIntArray");
    }

    #[test]
    fn csharp_namespace_from_snake_case_produces_pascal_case() {
        let ns = CSharpNamespace::from_source("demo_lib");
        assert_eq!(ns.as_str(), "DemoLib");
    }

    #[test]
    fn c_function_name_wraps_complete_symbol() {
        let name = CFunctionName::new("boltffi_echo_i32".to_string());
        assert_eq!(name.as_str(), "boltffi_echo_i32");
        assert_eq!(name.to_string(), "boltffi_echo_i32");
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
        fn class_name_exposes_underlying_name_on_both_variants() {
            let plain: CSharpTypeReference = CSharpClassName::from_source("point").into();
            assert_eq!(plain.class_name().as_str(), "Point");

            let qualified = CSharpTypeReference::Qualified {
                namespace: CSharpNamespace::from_source("demo"),
                name: CSharpClassName::from_source("point"),
            };
            assert_eq!(qualified.class_name().as_str(), "Point");
        }

        #[test]
        fn from_csharp_class_name_produces_plain_variant() {
            let name = CSharpClassName::from_source("point");
            let r: CSharpTypeReference = name.into();
            assert!(matches!(r, CSharpTypeReference::Plain(_)));
        }
    }
}
