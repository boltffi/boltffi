use std::fmt;

use crate::ir::types::PrimitiveType;
use boltffi_ffi_rules::naming::{LibraryName, Name};

/// Represents a lowered C# module, containing everything the templates need
/// to render a `.cs` file.
#[derive(Debug, Clone)]
pub struct CSharpModule {
    /// C# namespace for the generated file (e.g., `"MyApp"`).
    pub namespace: String,
    /// Top-level class name (e.g., `"MyApp"`).
    pub class_name: String,
    /// Native library name used in `[DllImport("...")]` declarations.
    pub lib_name: Name<LibraryName>,
    /// FFI symbol prefix (e.g., `"boltffi"`).
    pub prefix: String,
    /// Records exposed by the module. Each record is rendered to its own
    /// `.cs` file as a `readonly record struct`.
    pub records: Vec<CSharpRecord>,
    /// Enums exposed by the module. Each enum is rendered to its own `.cs`
    /// file — C-style as a native `enum`, data-carrying as an
    /// `abstract record` with nested `sealed record` variants.
    pub enums: Vec<CSharpEnum>,
    /// Top-level primitive functions. Used by both the public wrapper class
    /// and the `[DllImport]` native declarations — C# P/Invoke passes
    /// primitives directly, so one struct serves both layers.
    pub functions: Vec<CSharpFunction>,
}

impl CSharpModule {
    pub fn has_functions(&self) -> bool {
        !self.functions.is_empty()
    }

    /// Whether the shared runtime helpers need `System.Text`.
    ///
    /// Top-level string params use `Encoding.UTF8.GetBytes` in the wrapper,
    /// and `WireWriter` uses `Encoding.UTF8.GetByteCount` / `GetBytes` when
    /// encoding string-bearing params (including `Vec<String>` / nested
    /// string vecs) or string fields of a record. Decoding no longer needs
    /// `System.Text` — `WireReader` reads strings through
    /// `Marshal.PtrToStringUTF8`.
    pub fn needs_system_text(&self) -> bool {
        self.functions
            .iter()
            .any(|f| f.params.iter().any(|p| p.csharp_type.contains_string()))
            || self.records.iter().any(CSharpRecord::has_string_fields)
    }

    /// Whether any function takes a wire-encoded record param. Blittable
    /// record params pass through the CLR as direct struct values and do
    /// not contribute here.
    pub fn has_wire_params(&self) -> bool {
        self.functions.iter().any(|f| !f.wire_writers.is_empty())
    }

    /// Whether any function returns through an `FfiBuf` — a wire-decoded
    /// string or non-blittable record. Blittable records come back as
    /// direct struct values and do not count here.
    pub fn has_ffi_buf_returns(&self) -> bool {
        self.functions
            .iter()
            .any(|f| f.return_kind.native_returns_ffi_buf())
    }

    /// Whether the `FfiBuf` struct and `FreeBuf` DllImport are emitted.
    /// Needed for wire-encoded returns, and pulled in whenever a record or
    /// enum exists so the `WireReader` (which takes `FfiBuf`) compiles.
    pub fn needs_ffi_buf(&self) -> bool {
        self.has_ffi_buf_returns() || !self.records.is_empty() || !self.enums.is_empty()
    }

    /// Whether the stateful `WireReader` helper is emitted. Needed for
    /// wire-decoded returns, for any record's `Decode` method, and for the
    /// enum wire helpers (`StatusWire.Decode`, `Shape.Decode`).
    pub fn needs_wire_reader(&self) -> bool {
        self.has_ffi_buf_returns() || !self.records.is_empty() || !self.enums.is_empty()
    }

    /// Whether the `WireWriter` helper is emitted. Needed for wire-encoded
    /// params, for any record's `WireEncodeTo` method, and for the enum
    /// encode helpers.
    pub fn needs_wire_writer(&self) -> bool {
        self.has_wire_params() || !self.records.is_empty() || !self.enums.is_empty()
    }
}

/// A C# type keyword. Includes `Void` so return types and value types share
/// one enum; params never carry `Void` because the lowerer rejects it before
/// constructing a [`CSharpParam`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpType {
    Void,
    Bool,
    SByte,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    NInt,
    NUInt,
    Float,
    Double,
    String,
    /// A user-defined record, identified by its rendered PascalCase class
    /// name (e.g., `"Point"`).
    Record(String),
    /// A user-defined C-style enum (all variants are unit). Renders as a
    /// C# `enum` with an `int` backing type. Blittable — passes directly
    /// across P/Invoke as its underlying integer, and stays blittable when
    /// embedded in a `[StructLayout(Sequential)]` record.
    CStyleEnum(String),
    /// A user-defined data enum (at least one variant carries a payload).
    /// Renders as an `abstract record` with nested `sealed record` variants.
    /// Always wire-encoded — never blittable — because variant payloads
    /// are variable-width.
    DataEnum(String),
    /// A `Vec<T>` projected into the C# surface as a `T[]` jagged array.
    /// Uniform representation across every element kind — primitives ride
    /// the blittable bulk-copy path, composites walk element-by-element.
    /// Nested vecs fall out naturally via recursive `Array(Array(...))`.
    Array(Box<CSharpType>),
}

impl CSharpType {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    /// Whether this type contains `string` at any nesting depth. Used for
    /// import decisions where `string[]` / `string[][]` still require
    /// `System.Text` because their encode path calls `Encoding.UTF8`.
    pub fn contains_string(&self) -> bool {
        match self {
            Self::String => true,
            Self::Array(inner) => inner.contains_string(),
            _ => false,
        }
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnum(_))
    }

    pub fn is_data_enum(&self) -> bool {
        matches!(self, Self::DataEnum(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    /// If this is `Array(inner)`, returns `Some(inner)`; otherwise `None`.
    pub fn array_element(&self) -> Option<&CSharpType> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    /// If `self` is a user-defined named type (record or enum) whose
    /// class name is shadowed by an enclosing scope, return a variant
    /// wrapping the fully-qualified `global::{namespace}.{ClassName}`.
    /// Primitives and unnamed types pass through unchanged. The
    /// `global::` prefix dodges both nested-type shadowing *and* any
    /// same-named class in the current namespace (the generated
    /// top-level wrapper class, typically).
    pub fn qualify_if_shadowed(
        self,
        shadowed: &std::collections::HashSet<String>,
        namespace: &str,
    ) -> Self {
        match self {
            Self::Record(n) if shadowed.contains(&n) => {
                Self::Record(format!("global::{}.{}", namespace, n))
            }
            Self::CStyleEnum(n) if shadowed.contains(&n) => {
                Self::CStyleEnum(format!("global::{}.{}", namespace, n))
            }
            Self::DataEnum(n) if shadowed.contains(&n) => {
                Self::DataEnum(format!("global::{}.{}", namespace, n))
            }
            Self::Array(inner) => {
                Self::Array(Box::new((*inner).qualify_if_shadowed(shadowed, namespace)))
            }
            other => other,
        }
    }

    /// Whether this type is blittable in the CLR's sense — i.e. it can
    /// live inside a `[StructLayout(Sequential)]` record field and pass
    /// across P/Invoke without wire encoding. Primitives all qualify;
    /// `bool` does not (P/Invoke defaults to a 4-byte Win32 BOOL, so
    /// records with bool fields go through the wire path today). Strings
    /// and data enums are always wire-encoded. C-style enums are `int`
    /// underneath and ride the zero-copy path. Records are blittable or
    /// not based on their own field contents, decided elsewhere — this
    /// predicate only answers "is this *leaf* type blittable?".
    pub fn is_blittable_leaf(&self) -> bool {
        match self {
            Self::SByte
            | Self::Byte
            | Self::Short
            | Self::UShort
            | Self::Int
            | Self::UInt
            | Self::Long
            | Self::ULong
            | Self::NInt
            | Self::NUInt
            | Self::Float
            | Self::Double
            | Self::CStyleEnum(_) => true,
            Self::Void
            | Self::Bool
            | Self::String
            | Self::Record(_)
            | Self::DataEnum(_)
            | Self::Array(_) => false,
        }
    }
}

impl fmt::Display for CSharpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => f.write_str("void"),
            Self::Bool => f.write_str("bool"),
            Self::SByte => f.write_str("sbyte"),
            Self::Byte => f.write_str("byte"),
            Self::Short => f.write_str("short"),
            Self::UShort => f.write_str("ushort"),
            Self::Int => f.write_str("int"),
            Self::UInt => f.write_str("uint"),
            Self::Long => f.write_str("long"),
            Self::ULong => f.write_str("ulong"),
            Self::NInt => f.write_str("nint"),
            Self::NUInt => f.write_str("nuint"),
            Self::Float => f.write_str("float"),
            Self::Double => f.write_str("double"),
            Self::String => f.write_str("string"),
            Self::Record(name) | Self::CStyleEnum(name) | Self::DataEnum(name) => f.write_str(name),
            Self::Array(inner) => write!(f, "{inner}[]"),
        }
    }
}

/// A record (Rust struct) exposed as a C# `readonly record struct`.
///
/// Each record is emitted to its own `.cs` file. Blittable records (all
/// fields are primitives, layout matches Rust's `#[repr(C)]`) get a
/// `[StructLayout(LayoutKind.Sequential)]` attribute so the CLR passes
/// them directly across the P/Invoke boundary by value — no wire encoding
/// needed. Non-blittable records carry `Decode` / `WireEncodedSize` /
/// `WireEncodeTo` members and travel as wire-encoded buffers.
#[derive(Debug, Clone)]
pub struct CSharpRecord {
    /// PascalCase class name (e.g., `"Point"`).
    pub class_name: String,
    /// The record's fields, in declaration order.
    pub fields: Vec<CSharpRecordField>,
    /// Whether the record can cross the P/Invoke boundary as a direct
    /// `[StructLayout(Sequential)]` value. True when the Rust type is
    /// `#[repr(C)]` with blittable fields only.
    pub is_blittable: bool,
}

impl CSharpRecord {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Wire helpers are only needed for non-blittable records. Blittable
    /// records skip wire encoding entirely.
    pub fn needs_wire_helpers(&self) -> bool {
        !self.is_blittable
    }

    /// Whether the record has at least one string-typed field. Used by the
    /// record template to decide whether to import `System.Text` (for
    /// `Encoding.UTF8.GetByteCount`). Required because
    /// `TreatWarningsAsErrors` flags unused usings.
    pub fn has_string_fields(&self) -> bool {
        self.fields.iter().any(|f| f.csharp_type.is_string())
    }
}

/// A field on a [`CSharpRecord`]. All wire expressions are pre-rendered by
/// the lowerer so the template can paste them verbatim.
#[derive(Debug, Clone)]
pub struct CSharpRecordField {
    /// PascalCase property name (e.g., `"X"`). Records use PascalCase
    /// property names, not camelCase, matching idiomatic C# record syntax.
    pub name: String,
    /// C# type of the field.
    pub csharp_type: CSharpType,
    /// Expression that decodes this field from a `WireReader`
    /// (e.g., `"reader.ReadF64()"` or `"Point.Decode(reader)"`).
    pub wire_decode_expr: String,
    /// Expression that produces the wire-encoded byte size of this field
    /// (e.g., `"8"`, `"WireWriter.StringWireSize(this.Name)"`).
    pub wire_size_expr: String,
    /// Statement that writes this field to a `WireWriter` named `wire`
    /// (e.g., `"wire.WriteF64(this.X)"`).
    pub wire_encode_expr: String,
}

/// A Rust enum lifted into the C# type surface. C-style enums (all unit
/// variants) render as native `enum` declarations and ride the CLR's
/// transparent int-marshaling; data enums render as `abstract record`
/// hierarchies and travel wire-encoded.
#[derive(Debug, Clone)]
pub struct CSharpEnum {
    /// PascalCase class name (e.g., `"Shape"`, `"Status"`).
    pub class_name: String,
    /// Whether this is a C-style or data enum — drives the rendering shape.
    pub kind: CSharpEnumKind,
    /// For C-style enums, the declared integral repr primitive. `None` for
    /// data enums, whose public surface is always a reference type and whose
    /// wire tag stays an implementation detail of the codec.
    pub c_style_tag_type: Option<PrimitiveType>,
    /// Variants, in declaration order. The wire tag is the variant's index
    /// in this list (per `EnumTagStrategy::OrdinalIndex`), so order is
    /// load-bearing.
    pub variants: Vec<CSharpEnumVariant>,
    /// Methods and factory constructors declared via `#[data(impl)]`. For
    /// C-style enums these render as a companion `{Name}Methods` static
    /// class; for data enums they go directly on the abstract record.
    /// The Rust IR separates constructors from methods, but at the C#
    /// call site they're both just static or instance methods — merged
    /// into one list here.
    pub methods: Vec<CSharpMethod>,
}

/// The two flavors the enum renderer knows how to produce. The `#[repr]`
/// type could inform the C# backing type of a C-style enum, but for now
/// we always use `int` — matches the i32 wire tag and keeps the DllImport
/// signatures uniform with the free-function enum param/return shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpEnumKind {
    /// Every variant is a unit variant. Renders as `public enum Name : int`
    /// plus a `NameWire` static helper class with `Decode` and a
    /// `WireEncodeTo` extension method for when the enum embeds inside a
    /// wire-encoded record.
    CStyle,
    /// At least one variant carries fields. Renders as
    /// `public abstract record Name` with nested `sealed record` variants
    /// and switch-expression wire codec.
    Data,
}

/// One variant of a [`CSharpEnum`]. For C-style enums, `fields` is always
/// empty; for data enums, a unit variant also has empty `fields` (and
/// renders as `sealed record Name() : Enum`).
#[derive(Debug, Clone)]
pub struct CSharpEnumVariant {
    /// PascalCase variant name (e.g., `"Circle"`, `"Active"`).
    pub name: String,
    /// Numeric value rendered in the *public* surface. For C-style enums
    /// this is the Rust discriminant — `HttpCode.NotFound = 404` — so
    /// client code reading or comparing the enum sees real values, not
    /// ordinals. For data-enum variants this equals `wire_tag`; their
    /// public surface is a `sealed record`, not a numbered enum member,
    /// and only the codec uses the value.
    pub tag: i32,
    /// Ordinal index on the wire (0, 1, 2…), matching
    /// `EnumTagStrategy::OrdinalIndex`. Every boltffi backend wire-encodes
    /// C-style and data enums alike as a 4-byte little-endian `i32` of
    /// this tag — so C# must too, even for enums whose public `tag`
    /// diverges from their declaration order (gapped or negative
    /// discriminants). Keeping `wire_tag` separate from `tag` makes the
    /// two concepts explicit instead of hoping they'll always match.
    pub wire_tag: i32,
    /// Variant fields. Empty for unit variants and for every C-style
    /// variant. Reuses [`CSharpRecordField`] because variant payloads are
    /// structurally identical to record fields — same name, type, and
    /// pre-rendered wire expressions.
    pub fields: Vec<CSharpRecordField>,
}

impl CSharpEnum {
    pub fn is_c_style(&self) -> bool {
        self.kind == CSharpEnumKind::CStyle
    }

    pub fn is_data(&self) -> bool {
        self.kind == CSharpEnumKind::Data
    }

    pub fn has_methods(&self) -> bool {
        !self.methods.is_empty()
    }

    fn c_style_tag_type(&self) -> PrimitiveType {
        self.c_style_tag_type
            .expect("c-style enum helpers only apply to C-style enums")
    }

    /// The C# enum backing type keyword (`byte`, `short`, `int`, `long`,
    /// etc.). C# does not permit `nint` / `nuint` enum base types, so those
    /// reprs are filtered out before a plan is ever constructed.
    pub fn c_style_backing_type(&self) -> &'static str {
        match self.c_style_tag_type() {
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::Bool
            | PrimitiveType::ISize
            | PrimitiveType::USize
            | PrimitiveType::F32
            | PrimitiveType::F64 => panic!("unsupported C# enum backing type"),
        }
    }

    /// Whether any variant payload field is a `string`. Drives the
    /// `using System.Text;` import in the data enum template — needed
    /// because string-valued wire-size expressions call
    /// `Encoding.UTF8.GetByteCount(...)`, which lives in `System.Text`.
    pub fn has_string_fields(&self) -> bool {
        self.variants
            .iter()
            .flat_map(|v| v.fields.iter())
            .any(|f| f.csharp_type.is_string())
    }
}

impl CSharpEnumVariant {
    /// Whether this variant carries no payload. True for every C-style
    /// variant, and for data enum "unit" variants like `Shape::Point`.
    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }
}

/// A method or factory constructor on a value type — today always an
/// enum, eventually also records. Renders as a static method, a C#
/// extension method (for C-style enum instance methods, since C# enums
/// can't have members), or a native instance method on the owning type.
/// The dispatch is driven by [`CSharpReceiver`].
#[derive(Debug, Clone)]
pub struct CSharpMethod {
    /// PascalCase method name as it appears on the owning type's public
    /// API (e.g., `"Opposite"`, `"UnitCircle"`).
    pub name: String,
    /// Name used for this method's DllImport entry inside the shared
    /// `NativeMethods` class. Prefixed with the owning class name (e.g.,
    /// `"DirectionOpposite"`, `"ShapeArea"`) because two types may
    /// declare methods of the same name, and the DllImport class is
    /// flat.
    pub native_method_name: String,
    /// The C FFI symbol implementing this method (e.g.,
    /// `"boltffi_direction_opposite"`).
    pub ffi_name: String,
    /// How `self` (if any) participates in the call.
    pub receiver: CSharpReceiver,
    /// Explicit params — does not include `self` for instance methods.
    pub params: Vec<CSharpParam>,
    /// C# return type of the public-facing method.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI.
    pub return_kind: CSharpReturnKind,
    /// For each non-blittable record/data-enum param, the setup block
    /// that wire-encodes it into a `byte[]` before the native call.
    pub wire_writers: Vec<CSharpWireWriter>,
}

/// How a method's receiver (`self`) participates in the rendered C#.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReceiver {
    /// Static method — no `self`. Lives on whichever container the
    /// owning type uses: a companion `{Name}Methods` class for C-style
    /// enums, the abstract record for data enums, the record struct for
    /// records. Renders as `public static {ReturnType} {Name}({params})`.
    Static,
    /// Instance method on a C-style enum. Renders as a C# *extension*
    /// method `public static {ReturnType} {Name}(this {EnumType} self,
    /// {params})` in the companion class — giving `d.Name(args)` call
    /// syntax without requiring members on the enum itself. `self`
    /// passes directly to the DllImport since the CLR marshals the enum
    /// as its declared backing integral type.
    InstanceExtension,
    /// Instance method on a type that can hold its own members — data
    /// enums (on the abstract record) and records. Renders as a native
    /// method: `public {ReturnType} {Name}({params})`. When the owning
    /// type is wire-encoded (data enums, non-blittable records), the
    /// body wire-encodes `this` into a `byte[]` before the native call;
    /// blittable records pass `this` by value through P/Invoke.
    InstanceNative,
}

impl CSharpReceiver {
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static)
    }

    pub fn is_instance_extension(&self) -> bool {
        matches!(self, Self::InstanceExtension)
    }

    pub fn is_instance_native(&self) -> bool {
        matches!(self, Self::InstanceNative)
    }
}

impl CSharpMethod {
    pub fn is_void(&self) -> bool {
        matches!(self.return_kind, CSharpReturnKind::Void)
    }

    /// Comma-joined param declarations for the method signature —
    /// excludes `self`, which the template handles separately based on
    /// the receiver kind.
    pub fn wrapper_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::wrapper_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined call arguments for the native DllImport invocation,
    /// excluding `self`. Matches [`CSharpFunction::native_call_args`].
    pub fn native_call_args(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_call_arg)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The return type used in the DllImport signature. Wire-decoded
    /// returns (strings, non-blittable records, data enums) come back
    /// as an `FfiBuf`; everything else uses the C# type directly.
    pub fn native_return_type(&self) -> String {
        if self.return_kind.native_returns_ffi_buf() {
            "FfiBuf".to_string()
        } else {
            self.return_type.to_string()
        }
    }

    /// Declarations for nested `fixed` statements pinning every
    /// [`CSharpParamKind::PinnedArray`] param in the signature.
    pub fn pinned_fixed_args(&self) -> Vec<String> {
        pinned_fixed_args(&self.params)
    }

    pub fn has_pinned_params(&self) -> bool {
        !self.pinned_fixed_args().is_empty()
    }

    /// Param list used in the DllImport signature, including the
    /// receiver-dependent self declaration prepended when the method is
    /// an instance method:
    /// - `InstanceExtension` — prepends `{OwnerClass} self`, relying on
    ///   the CLR to marshal the enum as its declared backing integral type.
    /// - `InstanceNative` — prepends `byte[] self, UIntPtr selfLen` for
    ///   wire-encoded `this`; passes `{OwnerClass} self` for blittable
    ///   types.
    /// - `Static` — no self declaration.
    ///
    /// `owner_is_blittable` distinguishes the two `InstanceNative` sub-
    /// cases. For wire-encoded owners it's `false`; for blittable
    /// records it will be `true` once record instance methods land.
    pub fn native_param_list(&self, owner_class_name: &str, owner_is_blittable: bool) -> String {
        let explicit: Vec<String> = self
            .params
            .iter()
            .map(CSharpParam::native_declaration)
            .collect();
        let self_decl: Option<String> = match self.receiver {
            CSharpReceiver::Static => None,
            CSharpReceiver::InstanceExtension => Some(format!("{} self", owner_class_name)),
            CSharpReceiver::InstanceNative if owner_is_blittable => {
                Some(format!("{} self", owner_class_name))
            }
            CSharpReceiver::InstanceNative => Some("byte[] self, UIntPtr selfLen".to_string()),
        };
        match self_decl {
            Some(d) => std::iter::once(d)
                .chain(explicit)
                .collect::<Vec<_>>()
                .join(", "),
            None => explicit.join(", "),
        }
    }

    /// Comma-joined call arguments *including* the receiver's
    /// self-argument where the receiver needs one. Extension methods
    /// prepend the bound `self` local; data-enum instance methods
    /// prepend the pre-encoded `_selfBytes, (UIntPtr)_selfBytes.Length`
    /// pair that the surrounding method body set up.
    pub fn full_native_call_args(&self) -> String {
        let explicit = self.native_call_args();
        let self_prefix: &str = match self.receiver {
            CSharpReceiver::Static => "",
            CSharpReceiver::InstanceExtension => "self",
            CSharpReceiver::InstanceNative => "_selfBytes, (UIntPtr)_selfBytes.Length",
        };
        match (self_prefix.is_empty(), explicit.is_empty()) {
            (true, _) => explicit,
            (false, true) => self_prefix.to_string(),
            (false, false) => format!("{self_prefix}, {explicit}"),
        }
    }
}

/// A primitive function binding. Serves double duty: the template uses `name`
/// and C# types for the public static method, and `ffi_name` for the
/// `[DllImport]` entry point.
#[derive(Debug, Clone)]
pub struct CSharpFunction {
    /// PascalCase method name (e.g., `"EchoI32"`).
    pub name: String,
    /// Parameters with C# types.
    pub params: Vec<CSharpParam>,
    /// C# return type as it appears in the public wrapper signature.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI. Drives how the wrapper body
    /// decodes the native return and what the `[DllImport]` signature looks
    /// like.
    pub return_kind: CSharpReturnKind,
    /// The C symbol name (e.g., `"boltffi_echo_i32"`).
    pub ffi_name: String,
    /// For each non-blittable record param, the setup code that wire-encodes
    /// it into a `byte[]` before the native call. Empty if the function has
    /// no wire-encoded params (blittable record params count as direct and
    /// do not appear here).
    pub wire_writers: Vec<CSharpWireWriter>,
}

impl CSharpFunction {
    pub fn is_void(&self) -> bool {
        matches!(self.return_kind, CSharpReturnKind::Void)
    }

    /// Comma-joined param declarations as they appear in the public
    /// wrapper signature.
    pub fn wrapper_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::wrapper_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined param declarations as they appear in the
    /// `[DllImport]` native signature.
    pub fn native_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined call arguments handed to the native invocation.
    pub fn native_call_args(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_call_arg)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The return type used in the `[DllImport]` signature. Wire-encoded
    /// returns come back as an `FfiBuf`; everything else (primitives,
    /// bools, blittable records) uses the C# type directly.
    pub fn native_return_type(&self) -> String {
        if self.return_kind.native_returns_ffi_buf() {
            "FfiBuf".to_string()
        } else {
            self.return_type.to_string()
        }
    }

    /// Declarations for nested `fixed` statements pinning every
    /// [`CSharpParamKind::PinnedArray`] param in the signature.
    ///
    /// Rendered shape for a function with two pinned params:
    ///
    /// ```ignore
    /// [
    ///   "Location* _locationsPtr = locations",
    ///   "Trade* _tradesPtr = trades",
    /// ]
    /// ```
    ///
    /// The template wraps the call in `unsafe { fixed (...) { fixed (...)
    /// { ... } } }` so Rust reads directly from the C# heap without the
    /// GC relocating either managed array during the call.
    pub fn pinned_fixed_args(&self) -> Vec<String> {
        pinned_fixed_args(&self.params)
    }

    pub fn has_pinned_params(&self) -> bool {
        !self.pinned_fixed_args().is_empty()
    }
}

/// How a function's return value is delivered across the ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReturnKind {
    /// No return value.
    Void,
    /// Returned directly. Primitives, bools, and blittable records all
    /// share this path — the CLR already knows how to marshal them.
    Direct,
    /// The native function returns an `FfiBuf`. The wrapper copies the
    /// bytes into a managed `string` via `WireReader.ReadString` and
    /// frees the buffer.
    WireDecodeString,
    /// The native function returns an `FfiBuf` carrying a wire-encoded
    /// value with a static `Decode(WireReader)` method. The wrapper wraps
    /// it in a `WireReader` and calls `{class_name}.Decode(reader)` to
    /// reconstruct the value. Used for non-blittable records and data
    /// enums, whose rendered C# types both expose the same `Decode` API
    /// at the call site.
    WireDecodeObject { class_name: String },
    /// The native function returns an `FfiBuf` carrying a wire-encoded
    /// `Vec<T>`. The wrapper wraps it in a `WireReader` and invokes
    /// `reader_call` on the reader to reconstruct the managed `T[]`.
    /// `reader_call` is the full method invocation without the receiver,
    /// e.g. `ReadBlittableArray<int>()` for `Vec<i32>` or
    /// `ReadBoolArray()` for `Vec<bool>`.
    WireDecodeArray { reader_call: String },
}

impl CSharpReturnKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn is_wire_decode_string(&self) -> bool {
        matches!(self, Self::WireDecodeString)
    }

    pub fn is_wire_decode_object(&self) -> bool {
        matches!(self, Self::WireDecodeObject { .. })
    }

    pub fn is_wire_decode_array(&self) -> bool {
        matches!(self, Self::WireDecodeArray { .. })
    }

    /// Whether the native (DllImport) signature returns an `FfiBuf`.
    pub fn native_returns_ffi_buf(&self) -> bool {
        matches!(
            self,
            Self::WireDecodeString | Self::WireDecodeObject { .. } | Self::WireDecodeArray { .. }
        )
    }

    /// For `WireDecodeObject`, the decoded C# class name (e.g., `"Point"`
    /// for a record, `"Shape"` for a data enum); `None` for every other
    /// kind. Templates use this to emit `{class_name}.Decode`.
    pub fn decode_class_name(&self) -> Option<&str> {
        match self {
            Self::WireDecodeObject { class_name } => Some(class_name),
            _ => None,
        }
    }

    /// The `return` statement that goes inside the `try` block of a
    /// wire-decoded call body. `buf_var` is the local name holding the
    /// `FfiBuf` from the native call. Returns `None` for non-wire-decoded
    /// kinds so callers cannot misuse an empty-string fallback as valid
    /// generated code.
    pub fn wire_decode_return(&self, buf_var: &str) -> Option<String> {
        match self {
            Self::WireDecodeString => {
                Some(format!("return new WireReader({}).ReadString();", buf_var))
            }
            Self::WireDecodeObject { class_name } => Some(format!(
                "return {}.Decode(new WireReader({}));",
                class_name, buf_var
            )),
            Self::WireDecodeArray { reader_call } => Some(format!(
                "return new WireReader({}).{};",
                buf_var, reader_call
            )),
            _ => None,
        }
    }
}

/// A parameter in a C# function.
#[derive(Debug, Clone)]
pub struct CSharpParam {
    /// camelCase parameter name, keyword-escaped with `@` if needed.
    pub name: String,
    /// C# type as it appears in the public wrapper signature.
    pub csharp_type: CSharpType,
    /// How the parameter crosses the ABI.
    pub kind: CSharpParamKind,
}

impl CSharpParam {
    /// Declaration as it appears in the public wrapper signature,
    /// e.g. `"int value"`, `"string v"`, `"Point point"`.
    pub fn wrapper_declaration(&self) -> String {
        format!("{} {}", self.csharp_type, self.name)
    }

    /// Declaration as it appears in the `[DllImport]` signature — this
    /// is where the different marshalling paths diverge:
    /// - Primitives and blittable records pass through directly.
    /// - Bool needs the `[MarshalAs(UnmanagedType.I1)]` attribute
    ///   because P/Invoke defaults to the 4-byte Win32 BOOL.
    /// - Strings and wire-encoded records are split into
    ///   `(byte[] x, UIntPtr xLen)`.
    pub fn native_declaration(&self) -> String {
        match &self.kind {
            CSharpParamKind::Utf8Bytes | CSharpParamKind::WireEncoded { .. } => {
                format!("byte[] {name}, UIntPtr {name}Len", name = self.name)
            }
            CSharpParamKind::Direct if self.csharp_type.is_bool() => {
                format!("[MarshalAs(UnmanagedType.I1)] bool {}", self.name)
            }
            CSharpParamKind::Direct => {
                format!("{} {}", self.csharp_type, self.name)
            }
            CSharpParamKind::DirectArray => {
                let element = self
                    .csharp_type
                    .array_element()
                    .expect("DirectArray param must carry an Array type");
                let decl = format!("{element}[] {name}, UIntPtr {name}Len", name = self.name);
                if matches!(element, CSharpType::Bool) {
                    format!(
                        "[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)] {decl}"
                    )
                } else {
                    decl
                }
            }
            // The wrapper's `fixed` block takes the managed array and
            // hands the native side a raw pointer, so the DllImport sees
            // only `IntPtr` and a length — no element type, no P/Invoke
            // marshaling.
            CSharpParamKind::PinnedArray { .. } => {
                format!("IntPtr {name}, UIntPtr {name}Len", name = self.name)
            }
        }
    }

    /// The argument expression to hand to the native call — either the
    /// raw param, or the pre-encoded byte array plus its length.
    pub fn native_call_arg(&self) -> String {
        match &self.kind {
            CSharpParamKind::Direct => self.name.clone(),
            CSharpParamKind::Utf8Bytes => {
                let buf = format!("_{}Bytes", self.name);
                format!("{buf}, (UIntPtr){buf}.Length")
            }
            CSharpParamKind::WireEncoded { binding_name } => {
                format!("{binding_name}, (UIntPtr){binding_name}.Length")
            }
            CSharpParamKind::DirectArray => {
                format!("{name}, (UIntPtr){name}.Length", name = self.name)
            }
            // `_{name}Ptr` is the pointer local introduced by the
            // enclosing `fixed` statement; see `pinned_fixed_args`. The
            // cast to `IntPtr` matches the DllImport signature.
            //
            // The Rust FFI shim for `Vec<Passable>` takes a raw byte
            // length and divides by `size_of::<T>()` to recover the
            // element count — the opposite of `Vec<Primitive>`, which
            // takes element count directly. The primitive path and this
            // path therefore send different numbers across the same
            // `UIntPtr` slot. `Unsafe.SizeOf<T>()` is a JIT-time constant
            // for `unmanaged` struct types, so the multiply folds away.
            CSharpParamKind::PinnedArray { element_type } => {
                let ptr_name = self
                    .pinned_ptr_name()
                    .expect("PinnedArray params must have a pointer local");
                format!(
                    "(IntPtr){ptr_name}, (UIntPtr)({name}.Length * Unsafe.SizeOf<{element_type}>())",
                    ptr_name = ptr_name,
                    name = self.name,
                )
            }
        }
    }

    /// The one-line setup statement that prepares this param before the
    /// native call, or `None` when the param passes through directly.
    /// UTF-8 encoding is the only inline setup; record wire encoding
    /// needs a `using` block and is handled separately via
    /// [`CSharpFunction::wire_writers`].
    pub fn setup_statement(&self) -> Option<String> {
        match &self.kind {
            CSharpParamKind::Utf8Bytes => Some(format!(
                "byte[] _{name}Bytes = Encoding.UTF8.GetBytes({name});",
                name = self.name
            )),
            _ => None,
        }
    }

    pub fn pinned_fixed_arg(&self) -> Option<String> {
        match &self.kind {
            CSharpParamKind::PinnedArray { element_type } => Some(format!(
                "{element_type}* {ptr_name} = {name}",
                ptr_name = self
                    .pinned_ptr_name()
                    .expect("PinnedArray params must have a pointer local"),
                name = self.name,
            )),
            _ => None,
        }
    }

    fn pinned_ptr_name(&self) -> Option<String> {
        match self.kind {
            CSharpParamKind::PinnedArray { .. } => {
                let base_name = self.name.strip_prefix('@').unwrap_or(&self.name);
                Some(format!("_{base_name}Ptr"))
            }
            _ => None,
        }
    }
}

fn pinned_fixed_args(params: &[CSharpParam]) -> Vec<String> {
    params
        .iter()
        .filter_map(CSharpParam::pinned_fixed_arg)
        .collect()
}

/// How a parameter is marshalled across the C# / C ABI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpParamKind {
    /// Passed directly as a primitive (bool, int, double, etc.).
    Direct,
    /// A managed `string` that must be UTF-8 encoded into a `byte[]`
    /// and passed as `(byte[], UIntPtr)` to the native call.
    Utf8Bytes,
    /// A record that must be wire-encoded into a `byte[]` by a
    /// `WireWriter` and passed as `(byte[], UIntPtr)`. `binding_name`
    /// is the local variable holding the encoded byte array.
    WireEncoded { binding_name: String },
    /// A managed array of a blittable primitive element type, passed
    /// directly as `(T[], UIntPtr)` without any wire encoding. The CLR's
    /// default P/Invoke marshaller pins the array and hands the native
    /// side a pointer to the element buffer. `bool[]` gets an explicit
    /// `[MarshalAs(LPArray, ArraySubType = U1)]` override so CLR emits
    /// one byte per element instead of the 4-byte Win32 BOOL default.
    DirectArray,
    /// A managed array of a blittable record element type, pinned with
    /// a `fixed` statement so Rust can read directly from the C# heap.
    ///
    /// The struct layout of a blittable record matches Rust's `#[repr(C)]`
    /// exactly, so a pointer to the first element plus an element count
    /// is everything Rust needs. Primitive arrays can use the CLR's
    /// built-in direct-array path, but record arrays are trickier once
    /// the element type stops being blittable to the marshaller (for
    /// example because it contains `bool` or `char`): P/Invoke may
    /// marshal through a temporary native buffer instead of exposing the
    /// managed array in place. With the right field-level marshalling
    /// that copy can still be layout-compatible, but it is no longer the
    /// zero-copy contract this fast path wants. The wrapper sidesteps the
    /// marshaller entirely by taking a raw pointer with `fixed (T* _xPtr
    /// = x)`, which pins the array in place for the duration of the
    /// native call and passes the pointer as `IntPtr`. C# and Rust then
    /// read the same block of managed heap memory.
    ///
    /// `element_type` is the C# type literal for `T` (e.g., `"Location"`)
    /// — threaded here so `pinned_fixed_args` can render
    /// `Location* _xPtr = x` without re-deriving from `csharp_type`.
    PinnedArray { element_type: String },
}

/// Bookkeeping for a single record param that must be wire-encoded into a
/// `byte[]` before the native call. The template wraps these setup lines
/// in a `using` block so each `WireWriter` is disposed (and its rented
/// buffer recycled) even if the native call throws.
#[derive(Debug, Clone)]
pub struct CSharpWireWriter {
    /// The `_wire_foo` local name for the `WireWriter` instance.
    pub binding_name: String,
    /// The `_fooBytes` local name for the resulting `byte[]`.
    pub bytes_binding_name: String,
    /// The original (camelCase) param name, used to find the corresponding
    /// `CSharpParam` at render time.
    pub param_name: String,
    /// Expression rendered against the param that returns its wire-encoded
    /// byte size (e.g., `"point.WireEncodedSize()"`).
    pub size_expr: String,
    /// Statement that writes the param's contents into the `WireWriter`
    /// named by `binding_name` (e.g., `"point.WireEncodeTo(_wire_point)"`).
    pub encode_expr: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn function_with_return(
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunction {
        CSharpFunction {
            name: "Test".to_string(),
            params: vec![],
            return_type,
            return_kind,
            ffi_name: "boltffi_test".to_string(),
            wire_writers: vec![],
        }
    }

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParam {
        CSharpParam {
            name: name.to_string(),
            csharp_type,
            kind,
        }
    }

    #[rstest]
    #[case::void(CSharpType::Void, CSharpReturnKind::Void, true)]
    #[case::int(CSharpType::Int, CSharpReturnKind::Direct, false)]
    #[case::bool(CSharpType::Bool, CSharpReturnKind::Direct, false)]
    #[case::double(CSharpType::Double, CSharpReturnKind::Direct, false)]
    fn is_void(
        #[case] return_type: CSharpType,
        #[case] return_kind: CSharpReturnKind,
        #[case] expected: bool,
    ) {
        assert_eq!(
            function_with_return(return_type, return_kind).is_void(),
            expected
        );
    }

    #[test]
    fn record_type_display_uses_class_name() {
        let ty = CSharpType::Record("Point".to_string());
        assert_eq!(ty.to_string(), "Point");
        assert!(ty.is_record());
    }

    #[test]
    fn c_style_enum_type_display_uses_class_name() {
        let ty = CSharpType::CStyleEnum("Status".to_string());
        assert_eq!(ty.to_string(), "Status");
        assert!(ty.is_c_style_enum());
        assert!(!ty.is_data_enum());
    }

    #[test]
    fn data_enum_type_display_uses_class_name() {
        let ty = CSharpType::DataEnum("Shape".to_string());
        assert_eq!(ty.to_string(), "Shape");
        assert!(ty.is_data_enum());
        assert!(!ty.is_c_style_enum());
    }

    /// A variant with no payload fields is a unit — true for every C-style
    /// variant and for data-enum unit variants like `Shape::Point`.
    #[test]
    fn variant_with_empty_fields_is_unit() {
        let variant = CSharpEnumVariant {
            name: "Active".to_string(),
            tag: 0,
            wire_tag: 0,
            fields: vec![],
        };
        assert!(variant.is_unit());
    }

    /// A variant with at least one payload field is not a unit — the
    /// renderer emits a positional `sealed record Foo(double Radius)`
    /// rather than the empty-paren `sealed record Foo()` shape.
    #[test]
    fn variant_with_payload_is_not_unit() {
        let variant = CSharpEnumVariant {
            name: "Circle".to_string(),
            tag: 0,
            wire_tag: 0,
            fields: vec![CSharpRecordField {
                name: "Radius".to_string(),
                csharp_type: CSharpType::Double,
                wire_decode_expr: "reader.ReadF64()".to_string(),
                wire_size_expr: "8".to_string(),
                wire_encode_expr: "wire.WriteF64(this.Radius)".to_string(),
            }],
        };
        assert!(!variant.is_unit());
    }

    #[test]
    fn c_style_kind_is_c_style_and_not_data() {
        let enumeration = CSharpEnum {
            class_name: "Status".to_string(),
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(PrimitiveType::I32),
            variants: vec![],
            methods: vec![],
        };
        assert!(enumeration.is_c_style());
        assert!(!enumeration.is_data());
    }

    #[test]
    fn data_kind_is_data_and_not_c_style() {
        let enumeration = CSharpEnum {
            class_name: "Shape".to_string(),
            kind: CSharpEnumKind::Data,
            c_style_tag_type: None,
            variants: vec![],
            methods: vec![],
        };
        assert!(enumeration.is_data());
        assert!(!enumeration.is_c_style());
    }

    /// `c_style_backing_type` drives only the public enum declaration
    /// (`public enum LogLevel : byte`). The wire codec is width-fixed at
    /// 4 bytes across every boltffi backend, so there is no per-backing-
    /// type read/write method to resolve — the template hardcodes
    /// `ReadI32`/`WriteI32` around an ordinal-tag switch.
    #[test]
    fn c_style_backing_type_maps_primitive_to_csharp_keyword() {
        let enumeration = CSharpEnum {
            class_name: "LogLevel".to_string(),
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(PrimitiveType::U8),
            variants: vec![],
            methods: vec![],
        };

        assert_eq!(enumeration.c_style_backing_type(), "byte");
    }

    /// C-style enums ride P/Invoke as their declared backing integral type,
    /// so they count as blittable leaves alongside the numeric primitives.
    /// Data enums never do — their payloads are variable-width and must
    /// wire-encode.
    #[rstest]
    #[case::int(CSharpType::Int, true)]
    #[case::double(CSharpType::Double, true)]
    #[case::cstyle_enum(CSharpType::CStyleEnum("Status".to_string()), true)]
    #[case::bool(CSharpType::Bool, false)]
    #[case::string(CSharpType::String, false)]
    #[case::record(CSharpType::Record("Point".to_string()), false)]
    #[case::data_enum(CSharpType::DataEnum("Shape".to_string()), false)]
    fn is_blittable_leaf_matches_marshaling_story(#[case] ty: CSharpType, #[case] expected: bool) {
        assert_eq!(ty.is_blittable_leaf(), expected);
    }

    // ----- CSharpParam render helpers -----

    #[test]
    fn wrapper_declaration_puts_type_before_name() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.wrapper_declaration(), "int value");
    }

    #[test]
    fn wrapper_declaration_uses_record_class_name() {
        let p = param(
            "point",
            CSharpType::Record("Point".to_string()),
            CSharpParamKind::Direct,
        );
        assert_eq!(p.wrapper_declaration(), "Point point");
    }

    /// Direct primitives pass through the native declaration unchanged.
    #[test]
    fn native_declaration_direct_primitive_matches_wrapper() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.native_declaration(), "int value");
    }

    /// P/Invoke marshals `bool` as a 4-byte Win32 BOOL by default, but the
    /// C ABI uses a 1-byte native bool, so the `DllImport` signature must
    /// force `UnmanagedType.I1`. The public wrapper side stays plain.
    #[test]
    fn native_declaration_bool_gets_marshal_attribute() {
        let p = param("flag", CSharpType::Bool, CSharpParamKind::Direct);
        assert_eq!(
            p.native_declaration(),
            "[MarshalAs(UnmanagedType.I1)] bool flag"
        );
    }

    /// Blittable record params use `Direct` kind and pass by value, so the
    /// native declaration is just the struct name — no byte[] split.
    #[test]
    fn native_declaration_blittable_record_passes_by_value() {
        let p = param(
            "point",
            CSharpType::Record("Point".to_string()),
            CSharpParamKind::Direct,
        );
        assert_eq!(p.native_declaration(), "Point point");
    }

    /// String params split into two arguments to match the C ABI
    /// `(const uint8_t* ptr, uintptr_t len)`.
    #[test]
    fn native_declaration_string_splits_into_bytes_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(p.native_declaration(), "byte[] v, UIntPtr vLen");
    }

    /// Wire-encoded record params use the same `byte[] + UIntPtr` split
    /// as strings because the C ABI signature is identical.
    #[test]
    fn native_declaration_wire_encoded_record_splits_into_bytes_and_length() {
        let p = param(
            "person",
            CSharpType::Record("Person".to_string()),
            CSharpParamKind::WireEncoded {
                binding_name: "_personBytes".to_string(),
            },
        );
        assert_eq!(p.native_declaration(), "byte[] person, UIntPtr personLen");
    }

    #[test]
    fn native_call_arg_direct_passes_name() {
        let p = param("value", CSharpType::Int, CSharpParamKind::Direct);
        assert_eq!(p.native_call_arg(), "value");
    }

    #[test]
    fn native_call_arg_utf8_bytes_passes_buffer_and_length() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(p.native_call_arg(), "_vBytes, (UIntPtr)_vBytes.Length");
    }

    #[test]
    fn native_call_arg_wire_encoded_uses_binding_name() {
        let p = param(
            "person",
            CSharpType::Record("Person".to_string()),
            CSharpParamKind::WireEncoded {
                binding_name: "_personBytes".to_string(),
            },
        );
        assert_eq!(
            p.native_call_arg(),
            "_personBytes, (UIntPtr)_personBytes.Length"
        );
    }

    /// Only UTF-8 string params have an inline setup statement. Direct
    /// params need no prep; wire-encoded records use a `using` block
    /// that is emitted around the call, not as a flat setup line.
    #[rstest]
    #[case::direct(CSharpParamKind::Direct, None)]
    #[case::wire_encoded(
        CSharpParamKind::WireEncoded { binding_name: "_personBytes".to_string() },
        None,
    )]
    fn setup_statement_non_string_has_none(
        #[case] kind: CSharpParamKind,
        #[case] expected: Option<&str>,
    ) {
        let p = param("x", CSharpType::Int, kind);
        assert_eq!(p.setup_statement().as_deref(), expected);
    }

    #[test]
    fn setup_statement_utf8_bytes_encodes_string() {
        let p = param("v", CSharpType::String, CSharpParamKind::Utf8Bytes);
        assert_eq!(
            p.setup_statement().as_deref(),
            Some("byte[] _vBytes = Encoding.UTF8.GetBytes(v);"),
        );
    }

    // ----- CSharpFunction render helpers -----

    fn function_with_params(
        params: Vec<CSharpParam>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunction {
        CSharpFunction {
            name: "Test".to_string(),
            params,
            return_type,
            return_kind,
            ffi_name: "boltffi_test".to_string(),
            wire_writers: vec![],
        }
    }

    #[test]
    fn wrapper_param_list_joins_with_comma_space() {
        let f = function_with_params(
            vec![
                param("a", CSharpType::Int, CSharpParamKind::Direct),
                param("b", CSharpType::String, CSharpParamKind::Utf8Bytes),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(f.wrapper_param_list(), "int a, string b");
    }

    #[test]
    fn wrapper_param_list_empty_for_no_params() {
        let f = function_with_params(vec![], CSharpType::Void, CSharpReturnKind::Void);
        assert_eq!(f.wrapper_param_list(), "");
    }

    /// The native param list exposes each slot's marshalling shape — a
    /// string expands to a pair, bool gets a MarshalAs, and primitives
    /// stay bare. This is the one place the different shapes must line
    /// up, so we pin it with a mixed-shape case.
    #[test]
    fn native_param_list_expands_each_slot_by_kind() {
        let f = function_with_params(
            vec![
                param("flag", CSharpType::Bool, CSharpParamKind::Direct),
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
                param(
                    "person",
                    CSharpType::Record("Person".to_string()),
                    CSharpParamKind::WireEncoded {
                        binding_name: "_personBytes".to_string(),
                    },
                ),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_param_list(),
            "[MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, UIntPtr vLen, uint count, byte[] person, UIntPtr personLen",
        );
    }

    #[test]
    fn native_call_args_mirror_param_shapes() {
        let f = function_with_params(
            vec![
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_call_args(),
            "_vBytes, (UIntPtr)_vBytes.Length, count",
        );
    }

    /// Wire-encoded returns (string, non-blittable record) come back as
    /// an `FfiBuf` in the native signature regardless of the wrapper's
    /// public return type.
    #[rstest]
    #[case::void(CSharpType::Void, CSharpReturnKind::Void, "void")]
    #[case::primitive(CSharpType::Int, CSharpReturnKind::Direct, "int")]
    #[case::blittable_record(
        CSharpType::Record("Point".to_string()),
        CSharpReturnKind::Direct,
        "Point",
    )]
    #[case::string(CSharpType::String, CSharpReturnKind::WireDecodeString, "FfiBuf")]
    #[case::wire_record(
        CSharpType::Record("Person".to_string()),
        CSharpReturnKind::WireDecodeObject { class_name: "Person".to_string() },
        "FfiBuf",
    )]
    fn native_return_type_reflects_ffi_buf_paths(
        #[case] return_type: CSharpType,
        #[case] return_kind: CSharpReturnKind,
        #[case] expected: &str,
    ) {
        assert_eq!(
            function_with_return(return_type, return_kind).native_return_type(),
            expected
        );
    }

    #[test]
    fn wire_decode_return_for_string_uses_read_string() {
        let kind = CSharpReturnKind::WireDecodeString;
        assert_eq!(
            kind.wire_decode_return("_buf").as_deref(),
            Some("return new WireReader(_buf).ReadString();"),
        );
    }

    #[test]
    fn wire_decode_return_for_object_calls_decode() {
        let kind = CSharpReturnKind::WireDecodeObject {
            class_name: "Person".to_string(),
        };
        assert_eq!(
            kind.wire_decode_return("_buf").as_deref(),
            Some("return Person.Decode(new WireReader(_buf));"),
        );
    }

    #[rstest]
    #[case::void(CSharpReturnKind::Void)]
    #[case::direct(CSharpReturnKind::Direct)]
    fn wire_decode_return_none_for_non_wire_kinds(#[case] kind: CSharpReturnKind) {
        assert_eq!(kind.wire_decode_return("_buf"), None);
    }

    #[test]
    fn decode_class_name_some_only_for_wire_decode_object() {
        assert_eq!(
            CSharpReturnKind::WireDecodeObject {
                class_name: "Point".to_string()
            }
            .decode_class_name(),
            Some("Point"),
        );
        assert_eq!(CSharpReturnKind::WireDecodeString.decode_class_name(), None);
        assert_eq!(CSharpReturnKind::Void.decode_class_name(), None);
        assert_eq!(CSharpReturnKind::Direct.decode_class_name(), None);
    }
}
