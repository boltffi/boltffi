use std::collections::BTreeSet;

use boltffi_ast::{BuiltinType, MapKind};
use serde::{Deserialize, Serialize};

use crate::{
    BinderId, CallbackId, ClassId, CustomTypeId, DeclarationId, ElementCount, EnumId, FieldKey, Op,
    Primitive, RecordId, ValueRef,
};

/// Instructions for reconstructing one value from its boundary bytes.
///
/// A read plan is tree-shaped because an encoded value can contain other
/// encoded values: a `Vec<UserProfile>` is a sequence whose element is a
/// record whose fields are themselves encoded. The plan names the tree
/// once and every reader walks the same shape.
///
/// # Example
///
/// A `Vec<String>` is described by a [`CodecNode::Sequence`] whose element
/// is [`CodecNode::String`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReadPlan {
    root: CodecNode,
}

impl ReadPlan {
    pub(crate) fn new(root: CodecNode) -> Self {
        Self { root }
    }

    /// Creates a write plan for the same lowered codec tree.
    ///
    /// The returned plan binds the tree to `ValueRef::self_value()`. The
    /// transport shape stays fixed; only the boundary direction changes.
    pub fn write_self_value(&self) -> WritePlan {
        WritePlan::new(ValueRef::self_value(), self.root.clone())
    }

    /// Returns the root codec node.
    pub fn root(&self) -> &CodecNode {
        &self.root
    }

    /// Renders this read plan through the shared codec walker.
    pub fn render_with<R>(&self, renderer: &mut R) -> R::Expr
    where
        R: CodecRead,
    {
        CodecWalker::read(&self.root, renderer)
    }

    /// Returns whether this read plan includes a result container.
    pub fn uses_result(&self) -> bool {
        self.root.uses_result()
    }

    /// Returns whether this read plan includes the given builtin value.
    pub fn uses_builtin(&self, kind: BuiltinType) -> bool {
        self.root.uses_builtin(kind)
    }

    /// Appends every family-tagged declaration referenced by this read plan.
    pub fn append_referenced_declarations(&self, references: &mut BTreeSet<DeclarationId>) {
        self.root.append_referenced_declarations(references);
    }

    /// Returns whether this read plan references a declaration.
    pub fn references_declaration(&self, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        self.append_referenced_declarations(&mut references);
        references.contains(&declaration)
    }

    /// Returns whether this read plan includes a tagged interned string.
    pub fn uses_interned_string(&self) -> bool {
        self.root.contains_interned_string()
    }
}

/// Instructions for emitting one value as boundary bytes.
///
/// Mirror of [`ReadPlan`] for the encode direction. The value reference
/// names which already-bound value the plan consumes, so generated code
/// does not have to invent a path expression by string convention.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WritePlan {
    value: ValueRef,
    root: CodecNode,
}

impl WritePlan {
    pub(crate) fn new(value: ValueRef, root: CodecNode) -> Self {
        Self { value, root }
    }

    /// Creates a read plan for the same lowered codec tree.
    pub fn read_plan(&self) -> ReadPlan {
        ReadPlan::new(self.root.clone())
    }

    /// Returns the value the plan consumes.
    pub fn value(&self) -> &ValueRef {
        &self.value
    }

    /// Returns the root codec node.
    pub fn root(&self) -> &CodecNode {
        &self.root
    }

    /// Renders this write plan through the shared codec walker.
    pub fn render_with<W>(&self, renderer: &mut W) -> Vec<W::Stmt>
    where
        W: CodecWrite,
    {
        CodecWalker::write(&self.root, &self.value, renderer)
    }

    /// Renders the encoded byte count through the shared codec walker.
    pub fn size_with<S>(&self, renderer: &mut S) -> S::Expr
    where
        S: CodecSize,
    {
        CodecWalker::size(&self.root, &self.value, renderer)
    }

    /// Returns whether this write plan includes a result container.
    pub fn uses_result(&self) -> bool {
        self.root.uses_result()
    }

    /// Returns whether this write plan includes the given builtin value.
    pub fn uses_builtin(&self, kind: BuiltinType) -> bool {
        self.root.uses_builtin(kind)
    }

    /// Appends every family-tagged declaration referenced by this write plan.
    pub fn append_referenced_declarations(&self, references: &mut BTreeSet<DeclarationId>) {
        self.root.append_referenced_declarations(references);
    }

    /// Returns whether this write plan references a declaration.
    pub fn references_declaration(&self, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        self.append_referenced_declarations(&mut references);
        references.contains(&declaration)
    }

    /// Returns whether this write plan includes a tagged interned string.
    pub fn uses_interned_string(&self) -> bool {
        self.root.contains_interned_string()
    }
}

/// Bidirectional codec selected for one encoded value.
///
/// Encoded records and data enums always need both directions. Keeping the
/// pair together prevents construction sites from passing unrelated read and
/// write plans for the same declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CodecPlan {
    read: ReadPlan,
    write: WritePlan,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
#[doc(hidden)]
#[allow(missing_docs)]
pub enum OwnedWireEncoding {
    Generic,
    String,
    Utf8String,
    Bytes,
}

impl CodecPlan {
    pub(crate) fn new(read: ReadPlan, write: WritePlan) -> Self {
        Self { read, write }
    }

    /// Returns the plan used to read the encoded value.
    pub fn read(&self) -> &ReadPlan {
        &self.read
    }

    /// Returns the plan used to write the encoded value.
    pub fn write(&self) -> &WritePlan {
        &self.write
    }

    /// Returns whether either direction includes a result container.
    pub fn uses_result(&self) -> bool {
        self.read.uses_result() || self.write.uses_result()
    }

    /// Returns whether either direction includes the given builtin value.
    pub fn uses_builtin(&self, kind: BuiltinType) -> bool {
        self.read.uses_builtin(kind) || self.write.uses_builtin(kind)
    }

    /// Appends every family-tagged declaration referenced by either codec direction.
    pub fn append_referenced_declarations(&self, references: &mut BTreeSet<DeclarationId>) {
        self.read.append_referenced_declarations(references);
        self.write.append_referenced_declarations(references);
    }

    /// Returns whether either direction references a declaration.
    pub fn references_declaration(&self, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        self.append_referenced_declarations(&mut references);
        references.contains(&declaration)
    }

    /// Returns whether either direction includes a tagged interned string.
    pub fn uses_interned_string(&self) -> bool {
        self.read.uses_interned_string() || self.write.uses_interned_string()
    }
}

/// One node in a codec tree.
///
/// Names a value that requires encoding work to cross the boundary:
/// primitives, strings, byte buffers, optional and repeated values,
/// container shapes, and references to user-declared types. A node that
/// references a record knows whether the record crosses by direct memory
/// or by encoded payload, so a renderer that walks the tree cannot pick a
/// different boundary strategy for the nested value.
///
/// # Example
///
/// `Optional(Box<Sequence(Box<String>)>)` describes
/// `Option<Vec<String>>`. The same shape is what reading and writing
/// agree on; only the direction differs at render time.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CodecNode {
    /// Primitive scalar value.
    Primitive(Primitive),
    /// UTF-8 string value.
    String,
    #[doc(hidden)]
    Utf8String,
    /// Tagged interned-string value (static pool id or dynamic UTF-8 bytes).
    ///
    /// Wire format: `tag (u8)` where `0` = `u32` pool index, `1` = length-prefixed
    /// UTF-8 bytes. Hosts that do not explicitly advertise `InternedString`
    /// capability would silently misparse these bytes as a plain string.
    InternedString {
        /// Static values addressable by wire id before falling back to dynamic bytes.
        static_values: Vec<String>,
    },
    /// Byte buffer value.
    Bytes,
    /// Record carried by direct memory layout.
    DirectRecord(RecordId),
    /// Record carried by encoded fields.
    EncodedRecord(RecordId),
    /// Fieldless enum carried by an integer discriminant.
    CStyleEnum(EnumId),
    /// Payload-carrying enum carried by a tag and payload.
    DataEnum(EnumId),
    /// Class instance carried by a handle.
    ClassHandle(ClassId),
    /// Callback object carried by a handle.
    CallbackHandle(CallbackId),
    /// Custom type carried through its selected representation.
    Custom {
        /// Custom type declaration id.
        id: CustomTypeId,
        /// Codec used for the custom type's representation.
        representation: Box<CodecNode>,
    },
    /// Builtin value carried through its BoltFFI wire representation.
    Builtin(BuiltinType),
    /// Optional value with a presence marker followed by the inner value.
    Optional(Box<CodecNode>),
    /// Repeated values prefixed by an element count.
    Sequence {
        /// Expression that yields the number of elements.
        len: Op<ElementCount>,
        /// Codec used for each element.
        element: Box<CodecNode>,
    },
    /// Fixed-size ordered group of values.
    Tuple(Vec<CodecNode>),
    /// Fallible value with success and error payload codecs.
    Result {
        /// Codec used for the success payload.
        ok: Box<CodecNode>,
        /// Codec used for the error payload.
        err: Box<CodecNode>,
    },
    /// Key-value collection.
    Map {
        /// Source map constructor.
        kind: MapKind,
        /// Codec used for each key.
        key: Box<CodecNode>,
        /// Codec used for each value.
        value: Box<CodecNode>,
    },
}

impl CodecNode {
    #[doc(hidden)]
    #[allow(missing_docs)]
    pub fn owned_wire_encoding(&self) -> OwnedWireEncoding {
        match self {
            Self::String => OwnedWireEncoding::String,
            Self::Utf8String => OwnedWireEncoding::Utf8String,
            Self::Bytes => OwnedWireEncoding::Bytes,
            Self::Custom { representation, .. } => representation.owned_wire_encoding(),
            _ => OwnedWireEncoding::Generic,
        }
    }

    /// Appends every family-tagged declaration referenced by this codec tree.
    pub fn append_referenced_declarations(&self, references: &mut BTreeSet<DeclarationId>) {
        match self {
            Self::DirectRecord(id) | Self::EncodedRecord(id) => {
                references.insert(DeclarationId::Record(*id));
            }
            Self::CStyleEnum(id) | Self::DataEnum(id) => {
                references.insert(DeclarationId::Enum(*id));
            }
            Self::ClassHandle(id) => {
                references.insert(DeclarationId::Class(*id));
            }
            Self::CallbackHandle(id) => {
                references.insert(DeclarationId::Callback(*id));
            }
            Self::Custom { id, representation } => {
                references.insert(DeclarationId::CustomType(*id));
                representation.append_referenced_declarations(references);
            }
            Self::Optional(inner) | Self::Sequence { element: inner, .. } => {
                inner.append_referenced_declarations(references);
            }
            Self::Tuple(elements) => elements
                .iter()
                .for_each(|element| element.append_referenced_declarations(references)),
            Self::Result { ok, err } => {
                ok.append_referenced_declarations(references);
                err.append_referenced_declarations(references);
            }
            Self::Map { key, value, .. } => {
                key.append_referenced_declarations(references);
                value.append_referenced_declarations(references);
            }
            Self::Primitive(_)
            | Self::String
            | Self::Utf8String
            | Self::InternedString { .. }
            | Self::Bytes
            | Self::Builtin(_) => {}
        }
    }

    /// Returns whether this codec tree references a declaration.
    pub fn references_declaration(&self, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        self.append_referenced_declarations(&mut references);
        references.contains(&declaration)
    }

    /// Returns whether this codec tree contains an [`InternedString`](Self::InternedString) node.
    ///
    /// Used by capability gates to detect bindings that require the `InternedString`
    /// host capability.
    pub fn contains_interned_string(&self) -> bool {
        self.render_read_with(&mut InternedStringDetector)
    }

    /// Returns whether this codec tree contains a custom conversion node.
    pub fn contains_custom(&self) -> bool {
        match self {
            Self::Custom { .. } => true,
            Self::Optional(inner) | Self::Sequence { element: inner, .. } => {
                inner.contains_custom()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_custom),
            Self::Result { ok, err } => ok.contains_custom() || err.contains_custom(),
            Self::Map { key, value, .. } => key.contains_custom() || value.contains_custom(),
            _ => false,
        }
    }

    /// Returns whether this codec tree includes a result container.
    pub fn uses_result(&self) -> bool {
        match self {
            Self::Result { .. } => true,
            Self::Custom { representation, .. } => representation.uses_result(),
            Self::Optional(inner) | Self::Sequence { element: inner, .. } => inner.uses_result(),
            Self::Tuple(elements) => elements.iter().any(Self::uses_result),
            Self::Map { key, value, .. } => key.uses_result() || value.uses_result(),
            _ => false,
        }
    }

    /// Returns whether this codec tree includes the given builtin value.
    pub fn uses_builtin(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Builtin(builtin) => *builtin == kind,
            Self::Custom { representation, .. } => representation.uses_builtin(kind),
            Self::Optional(inner) | Self::Sequence { element: inner, .. } => {
                inner.uses_builtin(kind)
            }
            Self::Tuple(elements) => elements.iter().any(|element| element.uses_builtin(kind)),
            Self::Result { ok, err } => ok.uses_builtin(kind) || err.uses_builtin(kind),
            Self::Map { key, value, .. } => key.uses_builtin(kind) || value.uses_builtin(kind),
            _ => false,
        }
    }

    /// Renders this codec node through the shared read walker.
    pub fn render_read_with<R>(&self, renderer: &mut R) -> R::Expr
    where
        R: CodecRead,
    {
        CodecWalker::read(self, renderer)
    }
}

/// Target-language rendering for codec reads.
///
/// Implementors receive rendered child expressions for container nodes.
/// The shared walker owns the traversal order, so backend code cannot
/// accidentally walk `CodecNode` with a different shape than another target.
pub trait CodecRead {
    /// Target expression produced by the reader.
    type Expr;

    /// Reads a primitive scalar.
    fn primitive(&mut self, primitive: Primitive) -> Self::Expr;

    /// Reads a UTF-8 string.
    fn string(&mut self) -> Self::Expr;

    #[doc(hidden)]
    fn utf8_string(&mut self) -> Self::Expr {
        self.string()
    }

    /// Reads a tagged interned string (static pool id or dynamic UTF-8 bytes).
    fn interned_string(&mut self, static_values: &[String]) -> Self::Expr;

    /// Reads a byte buffer.
    fn bytes(&mut self) -> Self::Expr;

    /// Reads a directly-carried record.
    fn direct_record(&mut self, id: RecordId) -> Self::Expr;

    /// Reads an encoded record.
    fn encoded_record(&mut self, id: RecordId) -> Self::Expr;

    /// Reads a fieldless enum.
    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr;

    /// Reads a payload-carrying enum.
    fn data_enum(&mut self, id: EnumId) -> Self::Expr;

    /// Reads a class handle.
    fn class_handle(&mut self, id: ClassId) -> Self::Expr;

    /// Reads a callback handle.
    fn callback_handle(&mut self, id: CallbackId) -> Self::Expr;

    /// Reads a custom type representation.
    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr;

    /// Reads a builtin value.
    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr;

    /// Reads an optional value.
    fn optional(&mut self, inner: Self::Expr) -> Self::Expr;

    /// Reads a sequence.
    fn sequence(&mut self, len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr;

    /// Reads a tuple.
    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr;

    /// Reads a fallible value.
    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr;

    /// Reads a map.
    fn map(&mut self, kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr;
}

/// Classifies codec trees that require the `InternedString` host capability.
///
/// This intentionally implements every [`CodecRead`] method rather than
/// reopening [`CodecNode`]. New read leaves must therefore be explicitly
/// classified when the shared walker grows.
struct InternedStringDetector;

impl CodecRead for InternedStringDetector {
    type Expr = bool;

    fn primitive(&mut self, _: Primitive) -> Self::Expr {
        false
    }

    fn string(&mut self) -> Self::Expr {
        false
    }

    fn utf8_string(&mut self) -> Self::Expr {
        false
    }

    fn interned_string(&mut self, _: &[String]) -> Self::Expr {
        true
    }

    fn bytes(&mut self) -> Self::Expr {
        false
    }

    fn direct_record(&mut self, _: RecordId) -> Self::Expr {
        false
    }

    fn encoded_record(&mut self, _: RecordId) -> Self::Expr {
        false
    }

    fn c_style_enum(&mut self, _: EnumId) -> Self::Expr {
        false
    }

    fn data_enum(&mut self, _: EnumId) -> Self::Expr {
        false
    }

    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        false
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        false
    }

    fn custom(&mut self, _: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        representation
    }

    fn builtin(&mut self, _: BuiltinType) -> Self::Expr {
        false
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        inner
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        element
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        elements.into_iter().any(|element| element)
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        ok || err
    }

    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        key || value
    }
}

/// Target-language rendering for codec writes.
///
/// Container methods receive statements produced by the shared walker for
/// their children. The writer still receives the value reference for the
/// current node so target code can spell the source expression.
pub trait CodecWrite {
    /// Target statement produced by the writer.
    type Stmt;

    /// Writes a primitive scalar.
    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a UTF-8 string.
    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt>;

    #[doc(hidden)]
    fn utf8_string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        self.string(value)
    }

    /// Writes a tagged interned string (static pool id or dynamic UTF-8 bytes).
    fn interned_string(&mut self, static_values: &[String], value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a byte buffer.
    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a directly-carried record.
    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes an encoded record.
    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a fieldless enum.
    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a payload-carrying enum.
    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a class handle.
    fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a callback handle.
    fn callback_handle(&mut self, id: CallbackId, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a custom type representation.
    fn custom<F>(
        &mut self,
        id: CustomTypeId,
        value: &ValueRef,
        representation: F,
    ) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>;

    /// Writes a builtin value.
    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt>;

    /// Writes an optional value.
    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt>;

    /// Writes a sequence.
    fn sequence(
        &mut self,
        value: &ValueRef,
        len: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt>;

    /// Writes a tuple.
    fn tuple(&mut self, value: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt>;

    /// Writes a fallible value.
    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt>;

    /// Writes a map.
    fn map(
        &mut self,
        kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt>;
}

/// Target-language rendering for encoded byte counts.
///
/// Mirrors [`CodecWrite`] but produces the expression used to pre-size an
/// encoder before the value is written. Targets that allocate output buffers
/// up front use this walker to keep sizing and writing aligned with the same
/// codec tree.
pub trait CodecSize {
    /// Target expression produced by the sizer.
    type Expr;

    /// Returns the encoded size of a primitive scalar.
    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a UTF-8 string.
    fn string(&mut self, value: &ValueRef) -> Self::Expr;

    #[doc(hidden)]
    fn utf8_string(&mut self, value: &ValueRef) -> Self::Expr {
        self.string(value)
    }

    /// Returns the encoded size of a tagged interned string.
    fn interned_string(&mut self, static_values: &[String], value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a byte buffer.
    fn bytes(&mut self, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a directly-carried record.
    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of an encoded record.
    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a fieldless enum.
    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a payload-carrying enum.
    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a class handle.
    fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a callback handle.
    fn callback_handle(&mut self, id: CallbackId, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a custom type representation.
    fn custom<F>(&mut self, id: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr;

    /// Returns the encoded size of a builtin value.
    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr;

    /// Returns the encoded size of an optional value.
    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr;

    /// Returns the encoded size of a sequence.
    fn sequence(
        &mut self,
        value: &ValueRef,
        len: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr;

    /// Returns the encoded size of a tuple.
    fn tuple(&mut self, value: &ValueRef, elements: Vec<Self::Expr>) -> Self::Expr;

    /// Returns the encoded size of a fallible value.
    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr;

    /// Returns the encoded size of a map.
    fn map(
        &mut self,
        kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Self::Expr,
        value_binder: BinderId,
        map_value: Self::Expr,
    ) -> Self::Expr;
}

struct CodecWalker;

impl CodecWalker {
    fn read<R>(node: &CodecNode, renderer: &mut R) -> R::Expr
    where
        R: CodecRead,
    {
        match node {
            CodecNode::Primitive(primitive) => renderer.primitive(*primitive),
            CodecNode::String => renderer.string(),
            CodecNode::Utf8String => renderer.utf8_string(),
            CodecNode::InternedString { static_values } => renderer.interned_string(static_values),
            CodecNode::Bytes => renderer.bytes(),
            CodecNode::DirectRecord(id) => renderer.direct_record(*id),
            CodecNode::EncodedRecord(id) => renderer.encoded_record(*id),
            CodecNode::CStyleEnum(id) => renderer.c_style_enum(*id),
            CodecNode::DataEnum(id) => renderer.data_enum(*id),
            CodecNode::ClassHandle(id) => renderer.class_handle(*id),
            CodecNode::CallbackHandle(id) => renderer.callback_handle(*id),
            CodecNode::Custom { id, representation } => {
                let representation = Self::read(representation, renderer);
                renderer.custom(*id, representation)
            }
            CodecNode::Builtin(kind) => renderer.builtin(*kind),
            CodecNode::Optional(inner) => {
                let inner = Self::read(inner, renderer);
                renderer.optional(inner)
            }
            CodecNode::Sequence { len, element } => {
                let element = Self::read(element, renderer);
                renderer.sequence(len, element)
            }
            CodecNode::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| Self::read(element, renderer))
                    .collect();
                renderer.tuple(elements)
            }
            CodecNode::Result { ok, err } => {
                let ok = Self::read(ok, renderer);
                let err = Self::read(err, renderer);
                renderer.result(ok, err)
            }
            CodecNode::Map { kind, key, value } => {
                let key = Self::read(key, renderer);
                let value = Self::read(value, renderer);
                renderer.map(*kind, key, value)
            }
        }
    }

    fn write<W>(node: &CodecNode, value: &ValueRef, renderer: &mut W) -> Vec<W::Stmt>
    where
        W: CodecWrite,
    {
        let mut next_binder = 0;
        Self::write_node(node, value, renderer, &mut next_binder)
    }

    fn size<S>(node: &CodecNode, value: &ValueRef, renderer: &mut S) -> S::Expr
    where
        S: CodecSize,
    {
        let mut next_binder = 0;
        Self::size_node(node, value, renderer, &mut next_binder)
    }

    fn write_node<W>(
        node: &CodecNode,
        value: &ValueRef,
        renderer: &mut W,
        next_binder: &mut u32,
    ) -> Vec<W::Stmt>
    where
        W: CodecWrite,
    {
        match node {
            CodecNode::Primitive(primitive) => renderer.primitive(*primitive, value),
            CodecNode::String => renderer.string(value),
            CodecNode::Utf8String => renderer.utf8_string(value),
            CodecNode::InternedString { static_values } => {
                renderer.interned_string(static_values, value)
            }
            CodecNode::Bytes => renderer.bytes(value),
            CodecNode::DirectRecord(id) => renderer.direct_record(*id, value),
            CodecNode::EncodedRecord(id) => renderer.encoded_record(*id, value),
            CodecNode::CStyleEnum(id) => renderer.c_style_enum(*id, value),
            CodecNode::DataEnum(id) => renderer.data_enum(*id, value),
            CodecNode::ClassHandle(id) => renderer.class_handle(*id, value),
            CodecNode::CallbackHandle(id) => renderer.callback_handle(*id, value),
            CodecNode::Custom { id, representation } => {
                renderer.custom(*id, value, |renderer, value| {
                    Self::write_node(representation, value, renderer, next_binder)
                })
            }
            CodecNode::Builtin(kind) => renderer.builtin(*kind, value),
            CodecNode::Optional(inner) => {
                let binder = Self::next_binder(next_binder);
                let inner =
                    Self::write_node(inner, &ValueRef::binder(binder), renderer, next_binder);
                renderer.optional(value, binder, inner)
            }
            CodecNode::Sequence { len, element } => {
                let binder = Self::next_binder(next_binder);
                let element =
                    Self::write_node(element, &ValueRef::binder(binder), renderer, next_binder);
                renderer.sequence(value, len, binder, element)
            }
            CodecNode::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let field = FieldKey::position(index).expect("tuple index fits in u32");
                        Self::write_node(
                            element,
                            &value.clone().field(field),
                            renderer,
                            next_binder,
                        )
                    })
                    .collect();
                renderer.tuple(value, elements)
            }
            CodecNode::Result { ok, err } => {
                let binder = Self::next_binder(next_binder);
                let ok = Self::write_node(ok, &ValueRef::binder(binder), renderer, next_binder);
                let err = Self::write_node(err, &ValueRef::binder(binder), renderer, next_binder);
                renderer.result(value, binder, ok, err)
            }
            CodecNode::Map {
                kind,
                key,
                value: map_value,
            } => {
                let key_binder = Self::next_binder(next_binder);
                let value_binder = Self::next_binder(next_binder);
                let key =
                    Self::write_node(key, &ValueRef::binder(key_binder), renderer, next_binder);
                let map_value = Self::write_node(
                    map_value,
                    &ValueRef::binder(value_binder),
                    renderer,
                    next_binder,
                );
                renderer.map(*kind, value, key_binder, key, value_binder, map_value)
            }
        }
    }

    fn next_binder(next_binder: &mut u32) -> BinderId {
        let binder = BinderId::from_raw(*next_binder);
        *next_binder = next_binder
            .checked_add(1)
            .expect("codec binder id fits in u32");
        binder
    }

    fn size_node<S>(
        node: &CodecNode,
        value: &ValueRef,
        renderer: &mut S,
        next_binder: &mut u32,
    ) -> S::Expr
    where
        S: CodecSize,
    {
        match node {
            CodecNode::Primitive(primitive) => renderer.primitive(*primitive, value),
            CodecNode::String => renderer.string(value),
            CodecNode::Utf8String => renderer.utf8_string(value),
            CodecNode::InternedString { static_values } => {
                renderer.interned_string(static_values, value)
            }
            CodecNode::Bytes => renderer.bytes(value),
            CodecNode::DirectRecord(id) => renderer.direct_record(*id, value),
            CodecNode::EncodedRecord(id) => renderer.encoded_record(*id, value),
            CodecNode::CStyleEnum(id) => renderer.c_style_enum(*id, value),
            CodecNode::DataEnum(id) => renderer.data_enum(*id, value),
            CodecNode::ClassHandle(id) => renderer.class_handle(*id, value),
            CodecNode::CallbackHandle(id) => renderer.callback_handle(*id, value),
            CodecNode::Custom { id, representation } => {
                renderer.custom(*id, value, |renderer, value| {
                    Self::size_node(representation, value, renderer, next_binder)
                })
            }
            CodecNode::Builtin(kind) => renderer.builtin(*kind, value),
            CodecNode::Optional(inner) => {
                let binder = Self::next_binder(next_binder);
                let inner =
                    Self::size_node(inner, &ValueRef::binder(binder), renderer, next_binder);
                renderer.optional(value, binder, inner)
            }
            CodecNode::Sequence { len, element } => {
                let binder = Self::next_binder(next_binder);
                let element =
                    Self::size_node(element, &ValueRef::binder(binder), renderer, next_binder);
                renderer.sequence(value, len, binder, element)
            }
            CodecNode::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let field = FieldKey::position(index).expect("tuple index fits in u32");
                        Self::size_node(element, &value.clone().field(field), renderer, next_binder)
                    })
                    .collect();
                renderer.tuple(value, elements)
            }
            CodecNode::Result { ok, err } => {
                let binder = Self::next_binder(next_binder);
                let ok = Self::size_node(ok, &ValueRef::binder(binder), renderer, next_binder);
                let err = Self::size_node(err, &ValueRef::binder(binder), renderer, next_binder);
                renderer.result(value, binder, ok, err)
            }
            CodecNode::Map {
                kind,
                key,
                value: map_value,
            } => {
                let key_binder = Self::next_binder(next_binder);
                let value_binder = Self::next_binder(next_binder);
                let key =
                    Self::size_node(key, &ValueRef::binder(key_binder), renderer, next_binder);
                let map_value = Self::size_node(
                    map_value,
                    &ValueRef::binder(value_binder),
                    renderer,
                    next_binder,
                );
                renderer.map(*kind, value, key_binder, key, value_binder, map_value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use boltffi_ast::{BuiltinType, MapKind};

    use super::{CodecNode, CodecRead, CodecSize, CodecWrite, ReadPlan, WritePlan};
    use crate::{
        BinderId, CallbackId, ClassId, CustomTypeId, DeclarationId, ElementCount, EnumId, FieldKey,
        Op, Primitive, RecordId, ValueRef, ValueRoot,
    };

    struct TextRead;

    impl CodecRead for TextRead {
        type Expr = String;

        fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
            format!("primitive({primitive:?})")
        }

        fn string(&mut self) -> Self::Expr {
            "string".to_owned()
        }

        fn interned_string(&mut self, static_values: &[String]) -> Self::Expr {
            format!("interned_string([{}])", static_values.join(","))
        }

        fn bytes(&mut self) -> Self::Expr {
            "bytes".to_owned()
        }

        fn direct_record(&mut self, id: RecordId) -> Self::Expr {
            format!("direct_record({})", id.raw())
        }

        fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
            format!("encoded_record({})", id.raw())
        }

        fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
            format!("c_style_enum({})", id.raw())
        }

        fn data_enum(&mut self, id: EnumId) -> Self::Expr {
            format!("data_enum({})", id.raw())
        }

        fn class_handle(&mut self, id: ClassId) -> Self::Expr {
            format!("class_handle({})", id.raw())
        }

        fn callback_handle(&mut self, id: CallbackId) -> Self::Expr {
            format!("callback_handle({})", id.raw())
        }

        fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
            format!("custom({}, {representation})", id.raw())
        }

        fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
            format!("builtin({kind:?})")
        }

        fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
            format!("optional({inner})")
        }

        fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
            format!("sequence({element})")
        }

        fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
            format!("tuple({})", elements.join(","))
        }

        fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
            format!("result({ok},{err})")
        }

        fn map(&mut self, kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
            format!("map({kind:?},{key},{value})")
        }
    }

    struct TextWrite;

    struct TextSize;

    impl CodecWrite for TextWrite {
        type Stmt = String;

        fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("primitive({primitive:?}, {})", value_name(value))]
        }

        fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("string({})", value_name(value))]
        }

        fn interned_string(
            &mut self,
            static_values: &[String],
            value: &ValueRef,
        ) -> Vec<Self::Stmt> {
            vec![format!(
                "interned_string([{}], {})",
                static_values.join(","),
                value_name(value)
            )]
        }

        fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("bytes({})", value_name(value))]
        }

        fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!(
                "direct_record({}, {})",
                id.raw(),
                value_name(value)
            )]
        }

        fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!(
                "encoded_record({}, {})",
                id.raw(),
                value_name(value)
            )]
        }

        fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("c_style_enum({}, {})", id.raw(), value_name(value))]
        }

        fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("data_enum({}, {})", id.raw(), value_name(value))]
        }

        fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("class_handle({}, {})", id.raw(), value_name(value))]
        }

        fn callback_handle(&mut self, id: CallbackId, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!(
                "callback_handle({}, {})",
                id.raw(),
                value_name(value)
            )]
        }

        fn custom<F>(
            &mut self,
            id: CustomTypeId,
            value: &ValueRef,
            representation: F,
        ) -> Vec<Self::Stmt>
        where
            F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
        {
            let representation = representation(self, value);
            vec![format!(
                "custom({}, {}, [{}])",
                id.raw(),
                value_name(value),
                representation.join(";")
            )]
        }

        fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
            vec![format!("builtin({kind:?}, {})", value_name(value))]
        }

        fn optional(
            &mut self,
            value: &ValueRef,
            binder: BinderId,
            inner: Vec<Self::Stmt>,
        ) -> Vec<Self::Stmt> {
            vec![format!(
                "optional({}, b{}, [{}])",
                value_name(value),
                binder.raw(),
                inner.join(";")
            )]
        }

        fn sequence(
            &mut self,
            value: &ValueRef,
            _len: &Op<ElementCount>,
            binder: BinderId,
            element: Vec<Self::Stmt>,
        ) -> Vec<Self::Stmt> {
            vec![format!(
                "sequence({}, b{}, [{}])",
                value_name(value),
                binder.raw(),
                element.join(";")
            )]
        }

        fn tuple(&mut self, value: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
            let elements = elements
                .into_iter()
                .map(|element| element.join(";"))
                .collect::<Vec<_>>()
                .join("|");
            vec![format!("tuple({}, [{}])", value_name(value), elements)]
        }

        fn result(
            &mut self,
            value: &ValueRef,
            binder: BinderId,
            ok: Vec<Self::Stmt>,
            err: Vec<Self::Stmt>,
        ) -> Vec<Self::Stmt> {
            vec![format!(
                "result({}, b{}, [{}], [{}])",
                value_name(value),
                binder.raw(),
                ok.join(";"),
                err.join(";")
            )]
        }

        fn map(
            &mut self,
            kind: MapKind,
            value: &ValueRef,
            key_binder: BinderId,
            key: Vec<Self::Stmt>,
            value_binder: BinderId,
            map_value: Vec<Self::Stmt>,
        ) -> Vec<Self::Stmt> {
            vec![format!(
                "map({kind:?}, {}, k{}, [{}], v{}, [{}])",
                value_name(value),
                key_binder.raw(),
                key.join(";"),
                value_binder.raw(),
                map_value.join(";")
            )]
        }
    }

    impl CodecSize for TextSize {
        type Expr = String;

        fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Self::Expr {
            format!("size_primitive({primitive:?}, {})", value_name(value))
        }

        fn string(&mut self, value: &ValueRef) -> Self::Expr {
            format!("size_string({})", value_name(value))
        }

        fn interned_string(&mut self, static_values: &[String], value: &ValueRef) -> Self::Expr {
            format!(
                "size_interned_string([{}], {})",
                static_values.join(","),
                value_name(value)
            )
        }

        fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
            format!("size_bytes({})", value_name(value))
        }

        fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
            format!("size_direct_record({}, {})", id.raw(), value_name(value))
        }

        fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
            format!("size_encoded_record({}, {})", id.raw(), value_name(value))
        }

        fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
            format!("size_c_style_enum({}, {})", id.raw(), value_name(value))
        }

        fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
            format!("size_data_enum({}, {})", id.raw(), value_name(value))
        }

        fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Self::Expr {
            format!("size_class_handle({}, {})", id.raw(), value_name(value))
        }

        fn callback_handle(&mut self, id: CallbackId, value: &ValueRef) -> Self::Expr {
            format!("size_callback_handle({}, {})", id.raw(), value_name(value))
        }

        fn custom<F>(&mut self, id: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
        where
            F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
        {
            let representation = representation(self, value);
            format!(
                "size_custom({}, {}, {representation})",
                id.raw(),
                value_name(value)
            )
        }

        fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
            format!("size_builtin({kind:?}, {})", value_name(value))
        }

        fn optional(
            &mut self,
            value: &ValueRef,
            binder: BinderId,
            inner: Self::Expr,
        ) -> Self::Expr {
            format!(
                "size_optional({}, b{}, {inner})",
                value_name(value),
                binder.raw()
            )
        }

        fn sequence(
            &mut self,
            value: &ValueRef,
            _len: &Op<ElementCount>,
            binder: BinderId,
            element: Self::Expr,
        ) -> Self::Expr {
            format!(
                "size_sequence({}, b{}, {element})",
                value_name(value),
                binder.raw()
            )
        }

        fn tuple(&mut self, value: &ValueRef, elements: Vec<Self::Expr>) -> Self::Expr {
            format!(
                "size_tuple({}, [{}])",
                value_name(value),
                elements.join("|")
            )
        }

        fn result(
            &mut self,
            value: &ValueRef,
            binder: BinderId,
            ok: Self::Expr,
            err: Self::Expr,
        ) -> Self::Expr {
            format!(
                "size_result({}, b{}, {ok}, {err})",
                value_name(value),
                binder.raw()
            )
        }

        fn map(
            &mut self,
            kind: MapKind,
            value: &ValueRef,
            key_binder: BinderId,
            key: Self::Expr,
            value_binder: BinderId,
            map_value: Self::Expr,
        ) -> Self::Expr {
            format!(
                "size_map({kind:?}, {}, k{}, {key}, v{}, {map_value})",
                value_name(value),
                key_binder.raw(),
                value_binder.raw()
            )
        }
    }

    #[test]
    fn read_plan_renders_children_before_containers() {
        let plan = ReadPlan::new(CodecNode::Optional(Box::new(CodecNode::Sequence {
            len: Op::sequence_len(ValueRef::self_value()),
            element: Box::new(CodecNode::Primitive(Primitive::U32)),
        })));
        let mut renderer = TextRead;

        assert_eq!(
            plan.render_with(&mut renderer),
            "optional(sequence(primitive(U32)))"
        );
    }

    #[test]
    fn write_plan_allocates_unique_binders_for_nested_collections() {
        let plan = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Sequence {
                len: Op::sequence_len(ValueRef::self_value()),
                element: Box::new(CodecNode::Map {
                    kind: MapKind::Hash,
                    key: Box::new(CodecNode::String),
                    value: Box::new(CodecNode::Sequence {
                        len: Op::sequence_len(ValueRef::self_value()),
                        element: Box::new(CodecNode::Primitive(Primitive::U32)),
                    }),
                }),
            },
        );
        let mut renderer = TextWrite;

        assert_eq!(
            plan.render_with(&mut renderer),
            vec![
                "sequence(self, b0, [map(Hash, b0, k1, [string(b1)], v2, [sequence(b2, b3, [primitive(U32, b3)])])])"
                    .to_owned()
            ]
        );
    }

    #[test]
    fn write_plan_binds_optional_payload_before_rendering_inner_codec() {
        let plan = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Optional(Box::new(CodecNode::Primitive(Primitive::U32))),
        );
        let mut renderer = TextWrite;

        assert_eq!(
            plan.render_with(&mut renderer),
            vec!["optional(self, b0, [primitive(U32, b0)])".to_owned()]
        );
    }

    #[test]
    fn size_plan_allocates_unique_binders_for_nested_collections() {
        let plan = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Sequence {
                len: Op::sequence_len(ValueRef::self_value()),
                element: Box::new(CodecNode::Map {
                    kind: MapKind::Hash,
                    key: Box::new(CodecNode::String),
                    value: Box::new(CodecNode::Sequence {
                        len: Op::sequence_len(ValueRef::self_value()),
                        element: Box::new(CodecNode::Primitive(Primitive::U32)),
                    }),
                }),
            },
        );
        let mut renderer = TextSize;

        assert_eq!(
            plan.size_with(&mut renderer),
            "size_sequence(self, b0, size_map(Hash, b0, k1, size_string(b1), v2, size_sequence(b2, b3, size_primitive(U32, b3))))"
        );
    }

    #[test]
    fn write_plan_binds_result_payload_before_rendering_branch_codecs() {
        let plan = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Result {
                ok: Box::new(CodecNode::String),
                err: Box::new(CodecNode::Bytes),
            },
        );
        let mut renderer = TextWrite;

        assert_eq!(
            plan.render_with(&mut renderer),
            vec!["result(self, b0, [string(b0)], [bytes(b0)])".to_owned()]
        );
    }

    #[test]
    fn custom_codec_renders_representation_through_shared_walker() {
        let read = ReadPlan::new(CodecNode::Custom {
            id: CustomTypeId::from_raw(7),
            representation: Box::new(CodecNode::Sequence {
                len: Op::sequence_len(ValueRef::self_value()),
                element: Box::new(CodecNode::String),
            }),
        });
        let write = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Custom {
                id: CustomTypeId::from_raw(7),
                representation: Box::new(CodecNode::Sequence {
                    len: Op::sequence_len(ValueRef::self_value()),
                    element: Box::new(CodecNode::String),
                }),
            },
        );
        let mut reader = TextRead;
        let mut writer = TextWrite;

        assert_eq!(read.render_with(&mut reader), "custom(7, sequence(string))");
        assert_eq!(
            write.render_with(&mut writer),
            vec!["custom(7, self, [sequence(self, b0, [string(b0)])])".to_owned()]
        );
    }

    #[test]
    fn interned_string_codec_node_walks_with_distinct_method() {
        let static_values = vec!["Chrome".to_owned(), "Safari".to_owned()];
        let read = ReadPlan::new(CodecNode::InternedString {
            static_values: static_values.clone(),
        });
        let write = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::InternedString {
                static_values: static_values.clone(),
            },
        );
        let mut reader = TextRead;
        let mut writer = TextWrite;
        let mut sizer = TextSize;

        assert_eq!(
            read.render_with(&mut reader),
            "interned_string([Chrome,Safari])"
        );
        assert_eq!(
            write.render_with(&mut writer),
            vec!["interned_string([Chrome,Safari], self)".to_owned()]
        );
        assert_eq!(
            write.size_with(&mut sizer),
            "size_interned_string([Chrome,Safari], self)"
        );
    }

    #[test]
    fn interned_string_codec_node_nested_in_optional_reads_and_writes() {
        let static_values = vec!["Chrome".to_owned()];
        let read = ReadPlan::new(CodecNode::Optional(Box::new(CodecNode::InternedString {
            static_values: static_values.clone(),
        })));
        let write = WritePlan::new(
            ValueRef::self_value(),
            CodecNode::Optional(Box::new(CodecNode::InternedString {
                static_values: static_values.clone(),
            })),
        );
        let mut reader = TextRead;
        let mut writer = TextWrite;

        assert_eq!(
            read.render_with(&mut reader),
            "optional(interned_string([Chrome]))"
        );
        assert_eq!(
            write.render_with(&mut writer),
            vec!["optional(self, b0, [interned_string([Chrome], b0)])".to_owned()]
        );
    }

    #[test]
    fn codec_node_collects_nested_family_tagged_declaration_ids() {
        let node = CodecNode::Map {
            kind: MapKind::Hash,
            key: Box::new(CodecNode::Tuple(vec![
                CodecNode::DirectRecord(RecordId::from_raw(7)),
                CodecNode::ClassHandle(ClassId::from_raw(7)),
            ])),
            value: Box::new(CodecNode::Custom {
                id: CustomTypeId::from_raw(7),
                representation: Box::new(CodecNode::Optional(Box::new(CodecNode::CallbackHandle(
                    CallbackId::from_raw(7),
                )))),
            }),
        };
        let mut references = BTreeSet::new();

        node.append_referenced_declarations(&mut references);

        assert_eq!(
            references,
            BTreeSet::from([
                DeclarationId::Record(RecordId::from_raw(7)),
                DeclarationId::Class(ClassId::from_raw(7)),
                DeclarationId::Callback(CallbackId::from_raw(7)),
                DeclarationId::CustomType(CustomTypeId::from_raw(7)),
            ])
        );
    }

    #[test]
    fn codec_node_contains_interned_string_walks_every_container() {
        fn interned() -> CodecNode {
            CodecNode::InternedString {
                static_values: vec!["X".to_owned()],
            }
        }

        let sequence = |element| CodecNode::Sequence {
            len: Op::sequence_len(ValueRef::self_value()),
            element: Box::new(element),
        };

        assert!(
            CodecNode::Custom {
                id: CustomTypeId::from_raw(1),
                representation: Box::new(interned()),
            }
            .contains_interned_string()
        );
        assert!(CodecNode::Optional(Box::new(interned())).contains_interned_string());
        assert!(sequence(interned()).contains_interned_string());
        assert!(CodecNode::Tuple(vec![CodecNode::String, interned()]).contains_interned_string());
        assert!(
            CodecNode::Result {
                ok: Box::new(interned()),
                err: Box::new(CodecNode::String),
            }
            .contains_interned_string()
        );
        assert!(
            CodecNode::Result {
                ok: Box::new(CodecNode::String),
                err: Box::new(interned()),
            }
            .contains_interned_string()
        );
        assert!(
            CodecNode::Map {
                kind: MapKind::Hash,
                key: Box::new(interned()),
                value: Box::new(CodecNode::String),
            }
            .contains_interned_string()
        );
        assert!(
            CodecNode::Map {
                kind: MapKind::Hash,
                key: Box::new(CodecNode::String),
                value: Box::new(interned()),
            }
            .contains_interned_string()
        );

        let deeply_nested = CodecNode::Custom {
            id: CustomTypeId::from_raw(1),
            representation: Box::new(CodecNode::Optional(Box::new(sequence(CodecNode::Tuple(
                vec![CodecNode::Result {
                    ok: Box::new(CodecNode::String),
                    err: Box::new(CodecNode::Map {
                        kind: MapKind::Hash,
                        key: Box::new(CodecNode::Bytes),
                        value: Box::new(interned()),
                    }),
                }],
            ))))),
        };
        assert!(deeply_nested.contains_interned_string());
    }

    #[test]
    fn codec_node_contains_interned_string_rejects_explicit_non_interned_leaves() {
        let leaves = [
            CodecNode::Primitive(Primitive::U8),
            CodecNode::String,
            CodecNode::Utf8String,
            CodecNode::Bytes,
            CodecNode::DirectRecord(RecordId::from_raw(1)),
            CodecNode::EncodedRecord(RecordId::from_raw(1)),
            CodecNode::CStyleEnum(EnumId::from_raw(1)),
            CodecNode::DataEnum(EnumId::from_raw(1)),
            CodecNode::ClassHandle(ClassId::from_raw(1)),
            CodecNode::CallbackHandle(CallbackId::from_raw(1)),
            CodecNode::Builtin(BuiltinType::Duration),
        ];

        assert!(leaves.iter().all(|leaf| !leaf.contains_interned_string()));
        assert!(
            !CodecNode::Custom {
                id: CustomTypeId::from_raw(1),
                representation: Box::new(CodecNode::String),
            }
            .contains_interned_string()
        );
    }

    fn value_name(value: &ValueRef) -> String {
        let root = match value.root() {
            ValueRoot::SelfValue => "self".to_owned(),
            ValueRoot::Named(name) | ValueRoot::Local(name) => name.as_path_string(),
            ValueRoot::Binder(id) => format!("b{}", id.raw()),
        };
        value.path().iter().fold(root, |name, field| match field {
            FieldKey::Named(field) => format!("{name}.{}", field.as_path_string()),
            FieldKey::Position(index) => format!("{name}.{index}"),
        })
    }
}
