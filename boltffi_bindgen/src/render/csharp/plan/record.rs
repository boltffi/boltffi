use super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpComment, CSharpExpression, CSharpIdentity,
    CSharpMethodName, CSharpParamName, CSharpType,
};
use super::{CSharpFieldPlan, CSharpMethodPlan};

/// A Rust struct exposed as a C# `readonly record struct`, emitted to its own `.cs` file.
///
/// Examples:
/// ```csharp
/// // Blittable record: crosses P/Invoke by value
/// [StructLayout(LayoutKind.Sequential)]
/// public readonly record struct Point(double X, double Y);
///
/// // Non-blittable record: travels as a wire-encoded buffer
/// public readonly record struct Person(string Name, int Age)
/// {
///     internal static Person Decode(WireReader reader) => ...;
///     internal int WireEncodedSize() => ...;
///     internal void WireEncodeTo(WireWriter wire) { ... }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CSharpRecordPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// Class name (e.g., `"Point"`).
    pub class_name: CSharpClassName,
    /// The record's fields, in declaration order.
    pub fields: Vec<CSharpFieldPlan>,
    /// Whether the record is blittable: `#[repr(C)]` Rust layout with all
    /// blittable fields. Blittable records get `[StructLayout(LayoutKind.Sequential)]`
    /// and cross P/Invoke by value; otherwise the record carries
    /// `Decode`/`WireEncodedSize`/`WireEncodeTo` and travels as a wire buffer.
    pub is_blittable: bool,
    /// `#[data(impl)]` constructors and methods, merged into one list
    /// since at the C# call site they're both static or instance members
    /// on the record struct. Empty when the record has no `impl` block.
    /// Constructors lower to `Static` factory methods; instance methods
    /// lower to `InstanceNative` whose call shape is driven by
    /// [`CSharpMethodPlan::owner_is_blittable`].
    pub methods: Vec<CSharpMethodPlan>,
    /// Whether the record is marked `#[error]` on the Rust side. Drives
    /// emission of a companion `<Name>Exception` class in the same file
    /// so callers can `catch` a typed exception that wraps the record.
    pub is_error: bool,
}

impl CSharpRecordPlan {
    /// True for records with no fields. The template uses this to short-circuit
    /// `WireEncodedSize()` to `0` instead of emitting an empty sum.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Whether any field's type contains a string at any depth. Gates the
    /// `using System.Text;` import in the record template.
    pub fn has_string_fields(&self) -> bool {
        self.fields.iter().any(|f| f.csharp_type.contains_string())
    }

    /// Whether any record method needs `using System.Runtime.CompilerServices`
    /// for `Unsafe.SizeOf<T>()` in a pinned-array argument length expression.
    pub fn has_pinned_params(&self) -> bool {
        self.methods.iter().any(CSharpMethodPlan::has_pinned_params)
    }

    /// Whether this record file needs `using System.Text;`. String fields use
    /// `Encoding.UTF8.GetByteCount` in `WireEncodedSize`; string-bearing method
    /// params need it for UTF-8 setup or wire-writer size expressions.
    pub fn needs_system_text(&self) -> bool {
        self.has_string_fields()
            || self
                .methods
                .iter()
                .any(|m| m.params.iter().any(|p| p.csharp_type.contains_string()))
    }

    /// Whether any method on this record returns `Result<_, _>`. Used
    /// by the module-level predicate that decides whether to emit the
    /// runtime `BoltException` class.
    pub fn has_throwing_methods(&self) -> bool {
        self.methods.iter().any(|m| m.return_kind.is_result())
    }

    /// Expression passed as the `Exception.Message` base argument when
    /// emitting the typed exception class for an `is_error` record. By
    /// convention, if the record has a field named `Message` of type
    /// `string`, its value forwards straight through (so consumers see
    /// a focused message instead of the verbose default record
    /// formatting); otherwise `error.ToString()` is used.
    pub(crate) fn exception_message_expr(&self) -> CSharpExpression {
        let error = error_param_expr();
        match self
            .fields
            .iter()
            .find(|f| f.name.as_str() == "Message" && matches!(f.csharp_type, CSharpType::String))
        {
            Some(field) => CSharpExpression::MemberAccess {
                receiver: Box::new(error),
                name: field.name.clone(),
            },
            None => to_string_call(error),
        }
    }
}

/// `error`: the constructor parameter the typed-exception template
/// binds to the wrapped value. Both record and enum exception-message
/// expressions are evaluated against this name.
pub(crate) fn error_param_expr() -> CSharpExpression {
    CSharpExpression::Identity(CSharpIdentity::Param(CSharpParamName::new("error")))
}

/// `{receiver}.ToString()`: the fallback message expression when no
/// dedicated `Message` field is forwarded. Shared between the record
/// fallback path and every enum exception-message expression.
pub(crate) fn to_string_call(receiver: CSharpExpression) -> CSharpExpression {
    CSharpExpression::MethodCall {
        receiver: Box::new(receiver),
        method: CSharpMethodName::new("ToString"),
        type_args: vec![],
        args: CSharpArgumentList::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::ast::{
        CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalName, CSharpMethodName,
        CSharpPropertyName, CSharpStatement, CSharpType,
    };
    use super::super::{
        CFunctionName, CSharpFieldPlan, CSharpMethodPlan, CSharpReceiver, CSharpReturnKind,
    };
    use super::*;

    fn primitive_field(name: &str, csharp_type: CSharpType) -> CSharpFieldPlan {
        CSharpFieldPlan {
            summary_doc: None,
            name: CSharpPropertyName::from_source(name),
            csharp_type,
            wire_decode_expr: CSharpExpression::Literal(CSharpLiteral::Int(0)),
            wire_size_expr: CSharpExpression::Literal(CSharpLiteral::Int(0)),
            wire_encode_stmts: vec![CSharpStatement::Expression(CSharpExpression::Literal(
                CSharpLiteral::Int(0),
            ))],
        }
    }

    fn empty_record(class_name: &str, fields: Vec<CSharpFieldPlan>) -> CSharpRecordPlan {
        CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source(class_name),
            is_blittable: false,
            fields,
            methods: vec![],
            is_error: false,
        }
    }

    fn dummy_throw_expr() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("placeholder")))
    }

    fn method_with_return_kind(return_kind: CSharpReturnKind) -> CSharpMethodPlan {
        CSharpMethodPlan {
            summary_doc: None,
            name: CSharpMethodName::from_source("test"),
            native_method_name: CSharpMethodName::from_source("OwnerTest"),
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            receiver: CSharpReceiver::InstanceNative,
            params: vec![],
            return_type: CSharpType::Void,
            return_kind,
            wire_writers: vec![],
            owner_is_blittable: false,
        }
    }

    /// A record with a `Message: string` field forwards through to it so
    /// `Exception.Message` shows just `"Invalid input"` instead of the
    /// auto-generated `AppError { Code = 400, Message = "Invalid input" }`
    /// you'd otherwise get from a record `.ToString()`.
    #[test]
    fn exception_message_expr_forwards_string_message_field() {
        let record = empty_record(
            "app_error",
            vec![
                primitive_field("Code", CSharpType::Int),
                primitive_field("Message", CSharpType::String),
            ],
        );
        assert_eq!(record.exception_message_expr().to_string(), "error.Message");
    }

    /// Without a `Message: string` field, the typed exception falls back
    /// to the wrapped value's `ToString()`. Any other field name (or a
    /// non-string `Message`) takes this branch — string is the shape we
    /// can pass through verbatim, anything else needs the record's own
    /// formatting.
    #[test]
    fn exception_message_expr_falls_back_to_to_string_when_no_message_field() {
        let record = empty_record(
            "boundary",
            vec![
                primitive_field("X", CSharpType::Double),
                primitive_field("Y", CSharpType::Double),
            ],
        );
        assert_eq!(
            record.exception_message_expr().to_string(),
            "error.ToString()",
        );
    }

    /// A non-string `Message` field doesn't qualify for forwarding —
    /// `Exception.Message` is `string`, so passing an `int` through would
    /// be a type error in the generated source.
    #[test]
    fn exception_message_expr_falls_back_when_message_field_is_not_string() {
        let record = empty_record("status", vec![primitive_field("Message", CSharpType::Int)]);
        assert_eq!(
            record.exception_message_expr().to_string(),
            "error.ToString()",
        );
    }

    /// A record with no methods (the only shape today: records currently
    /// can't hold throwing methods themselves, since they're value types
    /// without their own native call ABI for `Result` returns) reports
    /// no throwing methods.
    #[test]
    fn has_throwing_methods_is_false_for_record_without_methods() {
        let record = empty_record("point", vec![]);
        assert!(!record.has_throwing_methods());
    }

    /// A record method whose return_kind is `WireDecodeResult` flips
    /// `has_throwing_methods` so the module predicate emits the runtime
    /// `BoltException` class. Mirrors the Java backend's per-class
    /// throwing-methods check.
    #[test]
    fn has_throwing_methods_is_true_when_a_record_method_is_a_result() {
        let mut record = empty_record("dataset", vec![]);
        record.methods.push(method_with_return_kind(
            CSharpReturnKind::WireDecodeResult {
                ok_decode_expr: None,
                err_throw_expr: dummy_throw_expr(),
            },
        ));
        assert!(record.has_throwing_methods());
    }

    /// Non-result method return kinds don't flip the predicate. This
    /// pins the negative case so a future return-kind addition can't
    /// accidentally route a non-throwing method through the runtime
    /// exception-emission path.
    #[test]
    fn has_throwing_methods_is_false_when_no_record_method_is_a_result() {
        let mut record = empty_record("dataset", vec![]);
        record
            .methods
            .push(method_with_return_kind(CSharpReturnKind::Direct));
        record
            .methods
            .push(method_with_return_kind(CSharpReturnKind::WireDecodeString));
        assert!(!record.has_throwing_methods());
    }
}
