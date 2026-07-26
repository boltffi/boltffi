use std::{collections::BTreeSet, fmt};

use crate::{
    BinderId, BuiltinType, ByteSize, CallbackId, ClassId, ClosureReturn, CodecPlan, CodecRead,
    CodecWrite, ConstantDecl, ConstantValueDecl, CustomTypeDecl, CustomTypeId, DataVariantPayload,
    Decl, DeclarationId, DeclarationRef, DirectValueType, DirectVectorElementType, ElementCount,
    EnumDecl, EnumId, ErrorChannel, ExportedCallable, FieldKey, HandlePresence, HandleTarget,
    ImportedCallable, IncomingParam, InitializerDecl, IntoRust, IntrinsicOp, MapKind, Op, OpRender,
    OutOfRust, OutgoingParam, ParamPlan, ParamPlanRender, Primitive, ReadPlan, Receive, RecordDecl,
    RecordId, ReturnPlan, ReturnPlanRender, ReturnTypeRef, ReturnValueSlot, StreamDecl, StreamId,
    StreamItemPlanRender, Surface, TypeRef, TypeRefRender, ValueRef, WritePlan,
};

/// The declaration form required by an IR reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum DeclarationShape {
    /// Any record representation.
    Record,
    /// A direct record representation.
    DirectRecord,
    /// An encoded record representation.
    EncodedRecord,
    /// Any enum representation.
    Enum,
    /// A C-style enum representation.
    CStyleEnum,
    /// A data enum representation.
    DataEnum,
    /// A class declaration.
    Class,
    /// A callback declaration.
    Callback,
    /// A stream declaration.
    Stream,
    /// A custom type declaration.
    CustomType,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeclarationReference {
    id: DeclarationId,
    shape: DeclarationShape,
}

impl DeclarationReference {
    pub const fn id(self) -> DeclarationId {
        self.id
    }

    pub const fn shape(self) -> DeclarationShape {
        self.shape
    }

    pub fn accepts<S: Surface>(self, declaration: DeclarationRef<'_, S>) -> bool {
        if self.id != declaration.id() {
            return false;
        }
        matches!(
            (self.shape, declaration),
            (DeclarationShape::Record, DeclarationRef::Record(_))
                | (
                    DeclarationShape::DirectRecord,
                    DeclarationRef::Record(RecordDecl::Direct(_))
                )
                | (
                    DeclarationShape::EncodedRecord,
                    DeclarationRef::Record(RecordDecl::Encoded(_))
                )
                | (DeclarationShape::Enum, DeclarationRef::Enum(_))
                | (
                    DeclarationShape::CStyleEnum,
                    DeclarationRef::Enum(EnumDecl::CStyle(_))
                )
                | (
                    DeclarationShape::DataEnum,
                    DeclarationRef::Enum(EnumDecl::Data(_))
                )
                | (DeclarationShape::Class, DeclarationRef::Class(_))
                | (DeclarationShape::Callback, DeclarationRef::Callback(_))
                | (DeclarationShape::Stream, DeclarationRef::Stream(_))
                | (DeclarationShape::CustomType, DeclarationRef::CustomType(_))
        )
    }

    const fn record(id: RecordId) -> Self {
        Self::new(DeclarationId::Record(id), DeclarationShape::Record)
    }

    const fn direct_record(id: RecordId) -> Self {
        Self::new(DeclarationId::Record(id), DeclarationShape::DirectRecord)
    }

    const fn encoded_record(id: RecordId) -> Self {
        Self::new(DeclarationId::Record(id), DeclarationShape::EncodedRecord)
    }

    const fn enumeration(id: EnumId) -> Self {
        Self::new(DeclarationId::Enum(id), DeclarationShape::Enum)
    }

    const fn c_style_enum(id: EnumId) -> Self {
        Self::new(DeclarationId::Enum(id), DeclarationShape::CStyleEnum)
    }

    const fn data_enum(id: EnumId) -> Self {
        Self::new(DeclarationId::Enum(id), DeclarationShape::DataEnum)
    }

    const fn class(id: ClassId) -> Self {
        Self::new(DeclarationId::Class(id), DeclarationShape::Class)
    }

    const fn callback(id: CallbackId) -> Self {
        Self::new(DeclarationId::Callback(id), DeclarationShape::Callback)
    }

    const fn stream(id: StreamId) -> Self {
        Self::new(DeclarationId::Stream(id), DeclarationShape::Stream)
    }

    const fn custom_type(id: CustomTypeId) -> Self {
        Self::new(DeclarationId::CustomType(id), DeclarationShape::CustomType)
    }

    const fn new(id: DeclarationId, shape: DeclarationShape) -> Self {
        Self { id, shape }
    }
}

impl fmt::Display for DeclarationShape {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Record => "record",
            Self::DirectRecord => "direct record",
            Self::EncodedRecord => "encoded record",
            Self::Enum => "enum",
            Self::CStyleEnum => "C-style enum",
            Self::DataEnum => "data enum",
            Self::Class => "class",
            Self::Callback => "callback",
            Self::Stream => "stream",
            Self::CustomType => "custom type",
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeclarationReferences {
    references: BTreeSet<DeclarationReference>,
}

impl DeclarationReferences {
    pub fn from_decl<S: Surface>(declaration: &Decl<S>) -> Self {
        let mut references = Self::default();
        references.insert_declaration_shape(DeclarationRef::from(declaration));
        declaration
            .exported_callables()
            .for_each(|callable| references.insert_exported_callable(callable));
        declaration
            .imported_callables()
            .for_each(|callable| references.insert_imported_callable(callable));
        references
    }

    pub fn iter(&self) -> impl Iterator<Item = DeclarationReference> + '_ {
        self.references.iter().copied()
    }

    fn insert_declaration_shape<S: Surface>(&mut self, declaration: DeclarationRef<'_, S>) {
        match declaration {
            DeclarationRef::Record(record) => self.insert_record(record),
            DeclarationRef::Enum(enumeration) => self.insert_enum(enumeration),
            DeclarationRef::Class(class) => {
                self.insert_initializer_returns(class.initializers());
            }
            DeclarationRef::Stream(stream) => self.insert_stream(stream),
            DeclarationRef::Constant(constant) => self.insert_constant(constant),
            DeclarationRef::CustomType(custom_type) => self.insert_custom_type(custom_type),
            DeclarationRef::Function(_) | DeclarationRef::Callback(_) => {}
        }
    }

    fn insert_record<S: Surface>(&mut self, record: &RecordDecl<S>) {
        if let RecordDecl::Encoded(record) = record {
            record.fields().iter().for_each(|field| {
                self.insert_type(field.ty());
                self.insert_codec_plan(field.codec());
            });
            self.insert_codec_plan(record.codec());
        }
        self.insert_initializer_returns(record.initializers());
    }

    fn insert_enum<S: Surface>(&mut self, enumeration: &EnumDecl<S>) {
        if let EnumDecl::Data(enumeration) = enumeration {
            enumeration
                .variants()
                .iter()
                .for_each(|variant| match variant.payload() {
                    DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                        fields.iter().for_each(|field| {
                            self.insert_type(field.ty());
                            self.insert_codec_plan(field.codec());
                        });
                    }
                    DataVariantPayload::Unit => {}
                });
            self.insert_codec_plan(enumeration.codec());
        }
        self.insert_initializer_returns(enumeration.initializers());
    }

    fn insert_stream<S: Surface>(&mut self, stream: &StreamDecl<S>) {
        stream
            .owner()
            .into_iter()
            .for_each(|owner| self.insert(DeclarationReference::class(owner)));
        stream.item().render_with(self);
    }

    fn insert_constant<S: Surface>(&mut self, constant: &ConstantDecl<S>) {
        if let ConstantValueDecl::Inline { ty, .. } = constant.value() {
            self.insert_type(ty);
        }
    }

    fn insert_custom_type(&mut self, custom_type: &CustomTypeDecl) {
        self.insert_type(custom_type.representation());
    }

    fn insert_initializer_returns<S: Surface>(&mut self, initializers: &[InitializerDecl<S>]) {
        initializers.iter().for_each(|initializer| {
            self.insert_return_type(initializer.returns());
        });
    }

    fn insert_exported_callable<S: Surface>(&mut self, callable: &ExportedCallable<S>) {
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                IncomingParam::Value(plan) => self.insert_incoming_param(plan),
                IncomingParam::Closure(closure) => self.insert_imported_callable(closure.invoke()),
            });
        self.insert_outgoing_return(callable.returns().plan());
        if let ErrorChannel::Encoded { ty, codec, .. } = callable.error().channel() {
            self.insert_type(ty);
            self.insert_read_plan(codec);
        }
    }

    fn insert_imported_callable<S: Surface>(&mut self, callable: &ImportedCallable<S>) {
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                OutgoingParam::Value(plan) => self.insert_outgoing_param(plan),
                OutgoingParam::Closure(closure) => self.insert_exported_callable(closure.invoke()),
            });
        self.insert_incoming_return(callable.returns().plan());
        if let ErrorChannel::Encoded { ty, codec, .. } = callable.error().channel() {
            self.insert_type(ty);
            self.insert_write_plan(codec);
        }
    }

    fn insert_incoming_param<S: Surface>(&mut self, plan: &ParamPlan<S, IntoRust>) {
        plan.render_with(self);
    }

    fn insert_outgoing_param<S: Surface>(&mut self, plan: &ParamPlan<S, OutOfRust>) {
        plan.render_with(self);
    }

    fn insert_outgoing_return<S: Surface>(&mut self, plan: &ReturnPlan<S, OutOfRust>) {
        plan.render_with(self);
    }

    fn insert_incoming_return<S: Surface>(&mut self, plan: &ReturnPlan<S, IntoRust>) {
        plan.render_with(self);
    }

    fn insert_return_type(&mut self, returns: &ReturnTypeRef) {
        if let ReturnTypeRef::Value(ty) = returns {
            self.insert_type(ty);
        }
    }

    fn insert_codec_plan(&mut self, plan: &CodecPlan) {
        self.insert_read_plan(plan.read());
        self.insert_write_plan(plan.write());
    }

    fn insert_read_plan(&mut self, plan: &ReadPlan) {
        plan.render_with(self);
    }

    fn insert_write_plan(&mut self, plan: &WritePlan) {
        plan.render_with(self);
    }

    fn insert_op<T>(&mut self, operation: &Op<T>) {
        operation.render_with(self);
    }

    fn insert_type(&mut self, ty: &TypeRef) {
        ty.render_with(self);
    }

    fn insert_direct_value(&mut self, ty: &DirectValueType) {
        match ty {
            DirectValueType::Record(id) => self.insert(DeclarationReference::direct_record(*id)),
            DirectValueType::Enum(id) => self.insert(DeclarationReference::c_style_enum(*id)),
            DirectValueType::Primitive(_) => {}
        }
    }

    fn insert_direct_vector(&mut self, element: &DirectVectorElementType) {
        if let DirectVectorElementType::Record(id) = element {
            self.insert(DeclarationReference::direct_record(*id));
        }
    }

    fn insert_handle(&mut self, target: &HandleTarget) {
        self.insert(match target {
            HandleTarget::Class(id) => DeclarationReference::class(*id),
            HandleTarget::Callback(id) => DeclarationReference::callback(*id),
            HandleTarget::Stream(id) => DeclarationReference::stream(*id),
        });
    }

    fn insert(&mut self, reference: DeclarationReference) {
        self.references.insert(reference);
    }
}

impl TypeRefRender for DeclarationReferences {
    type Output = ();

    fn primitive(&mut self, _: Primitive) {}

    fn string(&mut self) {}

    fn interned_string(&mut self, _: &[String]) {}

    fn bytes(&mut self) {}

    fn record(&mut self, id: RecordId) {
        self.insert(DeclarationReference::record(id));
    }

    fn enumeration(&mut self, id: EnumId) {
        self.insert(DeclarationReference::enumeration(id));
    }

    fn class(&mut self, id: ClassId) {
        self.insert(DeclarationReference::class(id));
    }

    fn callback(&mut self, id: CallbackId) {
        self.insert(DeclarationReference::callback(id));
    }

    fn custom(&mut self, id: CustomTypeId) {
        self.insert(DeclarationReference::custom_type(id));
    }

    fn builtin(&mut self, _: BuiltinType) {}

    fn optional(&mut self, _: Self::Output) {}

    fn sequence(&mut self, _: Self::Output) {}

    fn tuple(&mut self, _: Vec<Self::Output>) {}

    fn result(&mut self, _: Self::Output, _: Self::Output) {}

    fn map(&mut self, _: Self::Output, _: Self::Output) {}
}

impl OpRender for DeclarationReferences {
    type Expr = ();

    fn value(&mut self, _: &ValueRef) {}

    fn byte_count(&mut self, _: u64) {}

    fn integer(&mut self, _: i128) {}

    fn add(&mut self, _: Self::Expr, _: Self::Expr) {}

    fn mul(&mut self, _: Self::Expr, _: Self::Expr) {}

    fn eq(&mut self, _: Self::Expr, _: Self::Expr) {}

    fn field(&mut self, _: Self::Expr, _: &FieldKey) {}

    fn intrinsic(&mut self, _: IntrinsicOp, _: Vec<Self::Expr>) {}

    fn size_of(&mut self, ty: &DirectValueType) {
        self.insert_direct_value(ty);
    }
}

impl CodecRead for DeclarationReferences {
    type Expr = ();

    fn primitive(&mut self, _: Primitive) {}

    fn string(&mut self) {}

    fn interned_string(&mut self, _: &[String]) {}

    fn bytes(&mut self) {}

    fn direct_record(&mut self, id: RecordId) {
        self.insert(DeclarationReference::direct_record(id));
    }

    fn encoded_record(&mut self, id: RecordId) {
        self.insert(DeclarationReference::encoded_record(id));
    }

    fn c_style_enum(&mut self, id: EnumId) {
        self.insert(DeclarationReference::c_style_enum(id));
    }

    fn data_enum(&mut self, id: EnumId) {
        self.insert(DeclarationReference::data_enum(id));
    }

    fn class_handle(&mut self, id: ClassId) {
        self.insert(DeclarationReference::class(id));
    }

    fn callback_handle(&mut self, id: CallbackId) {
        self.insert(DeclarationReference::callback(id));
    }

    fn custom(&mut self, id: CustomTypeId, _: Self::Expr) {
        self.insert(DeclarationReference::custom_type(id));
    }

    fn builtin(&mut self, _: BuiltinType) {}

    fn optional(&mut self, _: Self::Expr) {}

    fn sequence(&mut self, len: &Op<ElementCount>, _: Self::Expr) {
        self.insert_op(len);
    }

    fn tuple(&mut self, _: Vec<Self::Expr>) {}

    fn result(&mut self, _: Self::Expr, _: Self::Expr) {}

    fn map(&mut self, _: MapKind, _: Self::Expr, _: Self::Expr) {}
}

impl CodecWrite for DeclarationReferences {
    type Stmt = ();

    fn primitive(&mut self, _: Primitive, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn string(&mut self, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn interned_string(&mut self, _: &[String], _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn bytes(&mut self, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn direct_record(&mut self, id: RecordId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::direct_record(id));
        Vec::new()
    }

    fn encoded_record(&mut self, id: RecordId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::encoded_record(id));
        Vec::new()
    }

    fn c_style_enum(&mut self, id: EnumId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::c_style_enum(id));
        Vec::new()
    }

    fn data_enum(&mut self, id: EnumId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::data_enum(id));
        Vec::new()
    }

    fn class_handle(&mut self, id: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::class(id));
        Vec::new()
    }

    fn callback_handle(&mut self, id: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        self.insert(DeclarationReference::callback(id));
        Vec::new()
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
        self.insert(DeclarationReference::custom_type(id));
        representation(self, value)
    }

    fn builtin(&mut self, _: BuiltinType, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn optional(&mut self, _: &ValueRef, _: BinderId, _: Vec<Self::Stmt>) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn sequence(
        &mut self,
        _: &ValueRef,
        len: &Op<ElementCount>,
        _: BinderId,
        _: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        self.insert_op(len);
        Vec::new()
    }

    fn tuple(&mut self, _: &ValueRef, _: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn result(
        &mut self,
        _: &ValueRef,
        _: BinderId,
        _: Vec<Self::Stmt>,
        _: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn map(
        &mut self,
        _: MapKind,
        _: &ValueRef,
        _: BinderId,
        _: Vec<Self::Stmt>,
        _: BinderId,
        _: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        Vec::new()
    }
}

impl<'plan, S: Surface> ParamPlanRender<'plan, S, IntoRust> for DeclarationReferences {
    type Output = ();

    fn direct(&mut self, ty: &'plan DirectValueType, _: Receive) {
        self.insert_direct_value(ty);
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan WritePlan,
        _: S::BufferShape,
        _: Receive,
    ) {
        self.insert_type(ty);
        self.insert_write_plan(codec);
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) {
        self.insert_handle(target);
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType, _: Receive) {
        self.insert_direct_vector(element);
    }
}

impl<'plan, S: Surface> ParamPlanRender<'plan, S, OutOfRust> for DeclarationReferences {
    type Output = ();

    fn direct(&mut self, ty: &'plan DirectValueType, _: ()) {
        self.insert_direct_value(ty);
    }

    fn encoded(&mut self, ty: &'plan TypeRef, codec: &'plan ReadPlan, _: S::BufferShape, _: ()) {
        self.insert_type(ty);
        self.insert_read_plan(codec);
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) {
        self.insert_handle(target);
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType, _: ()) {
        self.insert_direct_vector(element);
    }
}

impl<'plan, S: Surface> ReturnPlanRender<'plan, S, IntoRust> for DeclarationReferences {
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, ty: &'plan DirectValueType) {
        self.insert_direct_value(ty);
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan WritePlan,
        _: S::BufferShape,
    ) {
        self.insert_type(ty);
        self.insert_write_plan(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
    ) {
        self.insert_handle(target);
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) {
        self.insert_direct_vector(element);
    }

    fn closure(&mut self, closure: &'plan ClosureReturn<S, IntoRust>) {
        self.insert_imported_callable(closure.invoke());
    }
}

impl<'plan, S: Surface> ReturnPlanRender<'plan, S, OutOfRust> for DeclarationReferences {
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, ty: &'plan DirectValueType) {
        self.insert_direct_value(ty);
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan ReadPlan,
        _: S::BufferShape,
    ) {
        self.insert_type(ty);
        self.insert_read_plan(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
    ) {
        self.insert_handle(target);
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) {
        self.insert_direct_vector(element);
    }

    fn closure(&mut self, closure: &'plan ClosureReturn<S, OutOfRust>) {
        self.insert_exported_callable(closure.invoke());
    }
}

impl<'plan, S: Surface> StreamItemPlanRender<'plan, S> for DeclarationReferences {
    type Output = ();

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) {
        self.insert_direct_value(ty);
    }

    fn encoded(&mut self, ty: &'plan TypeRef, read: &'plan ReadPlan, _: S::BufferShape) {
        self.insert_type(ty);
        self.insert_read_plan(read);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use boltffi_ast::PackageInfo;
    use serde_json::json;

    use crate::{
        Bindings, CallbackId, ClassId, CodecNode, CustomTypeId, DeclarationId, DeclarationRef,
        ElementCount, EnumId, Native, Op, ReadPlan, RecordId, TypeRef, lower,
    };

    use super::{DeclarationReference, DeclarationReferences, DeclarationShape};

    #[test]
    fn collects_nested_type_and_codec_references() {
        let record = RecordId::from_raw(1);
        let sized_record = RecordId::from_raw(2);
        let enumeration = EnumId::from_raw(3);
        let class = ClassId::from_raw(4);
        let callback = CallbackId::from_raw(5);
        let custom_type = CustomTypeId::from_raw(6);
        let mut references = DeclarationReferences::default();
        references.insert_type(&TypeRef::Map {
            key: Box::new(TypeRef::Tuple(vec![
                TypeRef::Record(record),
                TypeRef::Enum(enumeration),
            ])),
            value: Box::new(TypeRef::Result {
                ok: Box::new(TypeRef::Class(class)),
                err: Box::new(TypeRef::Callback(callback)),
            }),
        });
        references.insert_type(&TypeRef::Custom(custom_type));
        let size: Op<ElementCount> = serde_json::from_value(json!({
            "node": {
                "SizeOf": {
                    "Record": sized_record.raw()
                }
            }
        }))
        .expect("size operation");
        references.insert_read_plan(&ReadPlan::new(CodecNode::Sequence {
            len: size,
            element: Box::new(CodecNode::Custom {
                id: custom_type,
                representation: Box::new(CodecNode::DirectRecord(record)),
            }),
        }));

        assert_eq!(
            references.iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                DeclarationReference::record(record),
                DeclarationReference::direct_record(record),
                DeclarationReference::direct_record(sized_record),
                DeclarationReference::enumeration(enumeration),
                DeclarationReference::class(class),
                DeclarationReference::callback(callback),
                DeclarationReference::custom_type(custom_type),
            ])
        );
    }

    #[test]
    fn collects_class_callback_closure_stream_custom_constant_and_initializer_edges() {
        let bindings = bindings(
            r#"
            use std::sync::Arc;
            use boltffi::EventSubscription;

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[repr(i32)]
            #[data]
            pub enum Mode {
                Fast = 1,
            }

            custom_type!(
                pub Wrapped,
                remote = WrappedRust,
                repr = Point,
                into_ffi = wrapped_into_ffi,
                try_from_ffi = wrapped_from_ffi
            );

            #[export]
            pub trait Listener {
                fn receive(&self, point: Point);
            }

            pub struct Engine;

            #[export]
            impl Engine {
                pub fn new(point: Point) -> Self { todo!() }

                #[ffi_stream(item = Point, mode = "batch")]
                pub fn points(&self) -> Arc<EventSubscription<Point>> { todo!() }
            }

            #[export]
            pub fn run(callback: impl Fn(Point) -> Point) {}

            #[export]
            pub const DEFAULT_MODE: Mode = Mode::Fast;
            "#,
        );
        let point = declaration_id(&bindings, "Point");
        let mode = declaration_id(&bindings, "Mode");
        let wrapped = declaration_id(&bindings, "Wrapped");
        let listener = declaration_id(&bindings, "Listener");
        let engine = declaration_id(&bindings, "Engine");
        let points = declaration_id(&bindings, "points");
        let run = declaration_id(&bindings, "run");
        let default_mode = declaration_id(&bindings, "DEFAULT_MODE");

        assert!(references(&bindings, wrapped).contains(&point));
        assert!(references(&bindings, listener).contains(&point));
        assert!(references(&bindings, engine).contains(&point));
        assert!(references(&bindings, engine).contains(&engine));
        assert_eq!(
            references(&bindings, points),
            BTreeSet::from([engine, point])
        );
        assert!(references(&bindings, run).contains(&point));
        assert_eq!(references(&bindings, default_mode), BTreeSet::from([mode]));
    }

    #[test]
    fn rejects_reference_shape_mismatches() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct Message {
                pub text: String,
            }
            "#,
        );
        let declaration = bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .find_map(DeclarationRef::record)
            .expect("message record");
        let reference = DeclarationReference::direct_record(declaration.id());

        assert!(!reference.accepts(DeclarationRef::Record(declaration)));
        assert_eq!(reference.shape(), DeclarationShape::DirectRecord);
        assert_eq!(reference.shape().to_string(), "direct record");
    }

    fn bindings(source: &str) -> Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("source fixture scans");
        lower::<Native>(&source).expect("source fixture lowers")
    }

    fn declaration_id(bindings: &Bindings<Native>, name: &str) -> DeclarationId {
        bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .find(|declaration| declaration_name(*declaration) == name)
            .map(DeclarationRef::id)
            .unwrap_or_else(|| panic!("missing declaration {name}"))
    }

    fn declaration_name(declaration: DeclarationRef<'_, Native>) -> &str {
        match declaration {
            DeclarationRef::Record(record) => record.name(),
            DeclarationRef::Enum(enumeration) => enumeration.name(),
            DeclarationRef::Function(function) => function.name(),
            DeclarationRef::Class(class) => class.name(),
            DeclarationRef::Callback(callback) => callback.name(),
            DeclarationRef::Stream(stream) => stream.name(),
            DeclarationRef::Constant(constant) => constant.name(),
            DeclarationRef::CustomType(custom_type) => custom_type.name(),
        }
        .source_spelling()
        .expect("source spelling")
    }

    fn references(
        bindings: &Bindings<Native>,
        declaration: DeclarationId,
    ) -> BTreeSet<DeclarationId> {
        bindings
            .decls()
            .iter()
            .find(|candidate| candidate.id() == declaration)
            .map(DeclarationReferences::from_decl)
            .expect("declaration references")
            .iter()
            .map(DeclarationReference::id)
            .collect()
    }
}
