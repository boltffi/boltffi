use std::collections::BTreeSet;

use super::{
    BinderId, BuiltinType, ByteSize, CallbackId, ClassId, ClosureReturn, CodecPlan, CodecRead,
    CodecWrite, CustomTypeId, DataVariantPayload, Decl, DeclarationRef, DirectValueType,
    DirectVectorElementType, ElementCount, EncodedFieldDecl, EnumDecl, EnumId, ErrorChannel,
    ExportedCallable, HandlePresence, HandleTarget, ImportedCallable, IncomingParam, IntoRust,
    MapKind, Op, OutOfRust, OutgoingParam, ParamPlanRender, Primitive, ReadPlan, Receive,
    RecordDecl, RecordId, ReturnPlanRender, ReturnValueSlot, StreamItemPlanRender, Surface,
    TypeRef, TypeRefRender, ValueRef, WritePlan,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ErrorPayloadReference {
    Record(RecordId),
    Enumeration(EnumId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ErrorPayloadTypes {
    records: BTreeSet<RecordId>,
    enumerations: BTreeSet<EnumId>,
    codec_records: BTreeSet<RecordId>,
}

impl ErrorPayloadTypes {
    pub(crate) fn from_decls<S: Surface>(decls: &[Decl<S>]) -> Self {
        decls
            .iter()
            .fold(Self::default(), |mut payloads, declaration| {
                payloads.insert_declaration_shape(DeclarationRef::from(declaration));
                declaration
                    .exported_callables()
                    .for_each(|callable| payloads.insert_exported_callable(callable));
                declaration
                    .imported_callables()
                    .for_each(|callable| payloads.insert_imported_callable(callable));
                payloads
            })
    }

    pub(crate) fn apply_decls<S: Surface>(&self, decls: &mut [Decl<S>]) {
        decls.iter_mut().for_each(|decl| match decl {
            Decl::Record(record) => {
                record.set_codec_payload(self.codec_records.contains(&record.id()));
                record.set_error_payload(self.records.contains(&record.id()));
            }
            Decl::Enum(enumeration) => {
                enumeration.set_error_payload(self.enumerations.contains(&enumeration.id()));
            }
            _ => {}
        });
    }

    fn insert_declaration_shape<S: Surface>(&mut self, declaration: DeclarationRef<'_, S>) {
        match declaration {
            DeclarationRef::Record(RecordDecl::Encoded(record)) => {
                self.insert_encoded_fields(record.fields());
                self.insert_codec_plan(record.codec());
            }
            DeclarationRef::Enum(EnumDecl::Data(enumeration)) => {
                enumeration
                    .variants()
                    .iter()
                    .for_each(|variant| match variant.payload() {
                        DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                            self.insert_encoded_fields(fields)
                        }
                        DataVariantPayload::Unit => {}
                    });
                self.insert_codec_plan(enumeration.codec());
            }
            DeclarationRef::Stream(stream) => {
                stream.item().render_with(self);
            }
            DeclarationRef::Record(RecordDecl::Direct(_))
            | DeclarationRef::Enum(EnumDecl::CStyle(_))
            | DeclarationRef::Function(_)
            | DeclarationRef::Class(_)
            | DeclarationRef::Callback(_)
            | DeclarationRef::Constant(_)
            | DeclarationRef::CustomType(_) => {}
        }
    }

    fn insert_encoded_fields(&mut self, fields: &[EncodedFieldDecl]) {
        fields.iter().for_each(|field| {
            self.insert_result_errors(field.ty());
            self.insert_codec_plan(field.codec());
        });
    }

    fn insert_exported_callable<S: Surface>(&mut self, callable: &ExportedCallable<S>) {
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                IncomingParam::Value(plan) => plan.render_with(self),
                IncomingParam::Closure(closure) => {
                    self.insert_imported_callable(closure.invoke());
                }
            });
        callable.returns().plan().render_with(self);
        if let ErrorChannel::Encoded { ty, codec, .. } = callable.error().channel() {
            self.insert_error_payload(ty);
            self.insert_read_plan(codec);
        }
    }

    fn insert_imported_callable<S: Surface>(&mut self, callable: &ImportedCallable<S>) {
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                OutgoingParam::Value(plan) => plan.render_with(self),
                OutgoingParam::Closure(closure) => {
                    self.insert_exported_callable(closure.invoke());
                }
            });
        callable.returns().plan().render_with(self);
        if let ErrorChannel::Encoded { ty, codec, .. } = callable.error().channel() {
            self.insert_error_payload(ty);
            self.insert_write_plan(codec);
        }
    }

    fn insert_error_payload(&mut self, ty: &TypeRef) {
        ty.render_with(self)
            .into_iter()
            .for_each(|reference| self.insert_payload(reference));
    }

    fn insert_result_errors(&mut self, ty: &TypeRef) {
        let _ = ty.render_with(self);
    }

    fn insert_payload(&mut self, reference: ErrorPayloadReference) {
        match reference {
            ErrorPayloadReference::Record(record) => {
                self.records.insert(record);
            }
            ErrorPayloadReference::Enumeration(enumeration) => {
                self.enumerations.insert(enumeration);
            }
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
}

impl TypeRefRender for ErrorPayloadTypes {
    type Output = Option<ErrorPayloadReference>;

    fn primitive(&mut self, _: Primitive) -> Self::Output {
        None
    }

    fn string(&mut self) -> Self::Output {
        None
    }

    fn interned_string(&mut self, _: &[String]) -> Self::Output {
        None
    }

    fn bytes(&mut self) -> Self::Output {
        None
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        Some(ErrorPayloadReference::Record(id))
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        Some(ErrorPayloadReference::Enumeration(id))
    }

    fn class(&mut self, _: ClassId) -> Self::Output {
        None
    }

    fn callback(&mut self, _: CallbackId) -> Self::Output {
        None
    }

    fn custom(&mut self, _: CustomTypeId) -> Self::Output {
        None
    }

    fn builtin(&mut self, _: BuiltinType) -> Self::Output {
        None
    }

    fn optional(&mut self, _: Self::Output) -> Self::Output {
        None
    }

    fn sequence(&mut self, _: Self::Output) -> Self::Output {
        None
    }

    fn tuple(&mut self, _: Vec<Self::Output>) -> Self::Output {
        None
    }

    fn result(&mut self, _: Self::Output, error: Self::Output) -> Self::Output {
        error
            .into_iter()
            .for_each(|reference| self.insert_payload(reference));
        None
    }

    fn map(&mut self, _: Self::Output, _: Self::Output) -> Self::Output {
        None
    }
}

impl CodecRead for ErrorPayloadTypes {
    type Expr = ();

    fn primitive(&mut self, _: Primitive) {}

    fn string(&mut self) {}

    fn interned_string(&mut self, _: &[String]) {}

    fn bytes(&mut self) {}

    fn direct_record(&mut self, id: RecordId) {
        self.codec_records.insert(id);
    }

    fn encoded_record(&mut self, _: RecordId) {}

    fn c_style_enum(&mut self, _: EnumId) {}

    fn data_enum(&mut self, _: EnumId) {}

    fn class_handle(&mut self, _: ClassId) {}

    fn callback_handle(&mut self, _: CallbackId) {}

    fn custom(&mut self, _: CustomTypeId, _: Self::Expr) {}

    fn builtin(&mut self, _: BuiltinType) {}

    fn optional(&mut self, _: Self::Expr) {}

    fn sequence(&mut self, _: &Op<ElementCount>, _: Self::Expr) {}

    fn tuple(&mut self, _: Vec<Self::Expr>) {}

    fn result(&mut self, _: Self::Expr, _: Self::Expr) {}

    fn map(&mut self, _: MapKind, _: Self::Expr, _: Self::Expr) {}
}

impl CodecWrite for ErrorPayloadTypes {
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
        self.codec_records.insert(id);
        Vec::new()
    }

    fn encoded_record(&mut self, _: RecordId, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn c_style_enum(&mut self, _: EnumId, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn data_enum(&mut self, _: EnumId, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn custom<F>(&mut self, _: CustomTypeId, value: &ValueRef, representation: F) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
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
        _: &Op<ElementCount>,
        _: BinderId,
        _: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
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

impl<'plan, S: Surface> ParamPlanRender<'plan, S, IntoRust> for ErrorPayloadTypes {
    type Output = ();

    fn direct(&mut self, _: &'plan DirectValueType, _: Receive) {}

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan WritePlan,
        _: S::BufferShape,
        _: Receive,
    ) {
        self.insert_result_errors(ty);
        self.insert_write_plan(codec);
    }

    fn handle(
        &mut self,
        _: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: Receive) {}
}

impl<'plan, S: Surface> ParamPlanRender<'plan, S, OutOfRust> for ErrorPayloadTypes {
    type Output = ();

    fn direct(&mut self, _: &'plan DirectValueType, _: ()) {}

    fn encoded(&mut self, ty: &'plan TypeRef, codec: &'plan ReadPlan, _: S::BufferShape, _: ()) {
        self.insert_result_errors(ty);
        self.insert_read_plan(codec);
    }

    fn handle(&mut self, _: &'plan HandleTarget, _: S::HandleCarrier, _: HandlePresence, _: ()) {}

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: ()) {}
}

impl<'plan, S: Surface> ReturnPlanRender<'plan, S, IntoRust> for ErrorPayloadTypes {
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan WritePlan,
        _: S::BufferShape,
    ) {
        self.insert_result_errors(ty);
        self.insert_write_plan(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) {}

    fn closure(&mut self, closure: &'plan ClosureReturn<S, IntoRust>) {
        self.insert_imported_callable(closure.invoke());
    }
}

impl<'plan, S: Surface> ReturnPlanRender<'plan, S, OutOfRust> for ErrorPayloadTypes {
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'plan DirectValueType) {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan ReadPlan,
        _: S::BufferShape,
    ) {
        self.insert_result_errors(ty);
        self.insert_read_plan(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: S::HandleCarrier,
        _: HandlePresence,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) {}

    fn closure(&mut self, closure: &'plan ClosureReturn<S, OutOfRust>) {
        self.insert_exported_callable(closure.invoke());
    }
}

impl<'plan, S: Surface> StreamItemPlanRender<'plan, S> for ErrorPayloadTypes {
    type Output = ();

    fn direct(&mut self, _: &'plan DirectValueType, _: ByteSize) {}

    fn encoded(&mut self, ty: &'plan TypeRef, read: &'plan ReadPlan, _: S::BufferShape) {
        self.insert_result_errors(ty);
        self.insert_read_plan(read);
    }
}
