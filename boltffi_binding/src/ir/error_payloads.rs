use std::collections::BTreeSet;

use super::{
    CallbackDecl, CallbackProtocolIntrospect, ClassDecl, CodecNode, ConstantValueDecl, Decl,
    DeclarationRef, EnumDecl, EnumId, ErrorChannel, ExportedCallable, ExportedMethodDecl,
    FunctionDecl, ImportedCallable, IncomingParam, InitializerDecl, IntoRust, NativeSymbol,
    OutOfRust, OutgoingParam, ParamPlan, ReadPlan, RecordDecl, RecordId, ReturnPlan, StreamDecl,
    StreamItemPlan, Surface, TypeRef, WritePlan,
};

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
            .map(DeclarationRef::from)
            .fold(Self::default(), |mut payloads, declaration| {
                payloads.insert_declaration(declaration);
                payloads
            })
    }

    pub(crate) fn mark_decls<S: Surface>(&self, decls: &mut [Decl<S>]) {
        decls.iter_mut().for_each(|decl| match decl {
            Decl::Record(record) => {
                if self.codec_records.contains(&record.id()) {
                    record.mark_codec_payload();
                }
                if self.records.contains(&record.id()) {
                    record.mark_error_payload();
                }
            }
            Decl::Enum(enumeration) if self.enumerations.contains(&enumeration.id()) => {
                enumeration.mark_error_payload();
            }
            _ => {}
        });
    }

    fn insert_declaration<S: Surface>(&mut self, declaration: DeclarationRef<'_, S>) {
        match declaration {
            DeclarationRef::Function(function) => self.insert_function(function),
            DeclarationRef::Record(record) => self.insert_record(record),
            DeclarationRef::Enum(enumeration) => self.insert_enum(enumeration),
            DeclarationRef::Class(class) => self.insert_class(class),
            DeclarationRef::Constant(constant) => {
                if let ConstantValueDecl::Accessor { callable, .. } = constant.value() {
                    self.insert_exported_callable(callable);
                }
            }
            DeclarationRef::Callback(callback) => self.insert_callback(callback),
            DeclarationRef::Stream(stream) => self.insert_stream(stream),
            DeclarationRef::CustomType(_) => {}
        }
    }

    fn insert_function<S: Surface>(&mut self, function: &FunctionDecl<S>) {
        self.insert_exported_callable(function.callable());
    }

    fn insert_record<S: Surface>(&mut self, record: &RecordDecl<S>) {
        if let RecordDecl::Encoded(record) = record {
            self.insert_read_plan(record.read());
            self.insert_write_plan(record.write());
        }
        self.insert_associated(record.initializers(), record.methods());
    }

    fn insert_enum<S: Surface>(&mut self, enumeration: &EnumDecl<S>) {
        if let EnumDecl::Data(enumeration) = enumeration {
            self.insert_read_plan(enumeration.read());
            self.insert_write_plan(enumeration.write());
        }
        self.insert_associated(enumeration.initializers(), enumeration.methods());
    }

    fn insert_class<S: Surface>(&mut self, class: &ClassDecl<S>) {
        self.insert_associated(class.initializers(), class.methods());
    }

    fn insert_callback<S: Surface>(&mut self, callback: &CallbackDecl<S>) {
        callback
            .protocol()
            .method_callables()
            .for_each(|callable| self.insert_imported_callable(callable));
        if let Some(protocol) = callback.local_protocol() {
            protocol
                .methods()
                .iter()
                .for_each(|method| self.insert_exported_callable(method.callable()));
        }
    }

    fn insert_stream<S: Surface>(&mut self, stream: &StreamDecl<S>) {
        self.insert_stream_item(stream.item());
    }

    fn insert_stream_item<S: Surface>(&mut self, item: &StreamItemPlan<S>) {
        if let StreamItemPlan::Encoded { ty, read, .. } = item {
            self.insert_read_plan(read);
            self.insert_result_errors(ty);
        }
    }

    fn insert_associated<S: Surface>(
        &mut self,
        initializers: &[InitializerDecl<S>],
        methods: &[ExportedMethodDecl<S, NativeSymbol>],
    ) {
        initializers
            .iter()
            .for_each(|initializer| self.insert_exported_callable(initializer.callable()));
        methods
            .iter()
            .for_each(|method| self.insert_exported_callable(method.callable()));
    }

    fn insert_exported_callable<S: Surface>(&mut self, callable: &ExportedCallable<S>) {
        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
            self.insert_error_payload(ty);
        }
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                IncomingParam::Value(plan) => self.insert_incoming_param_plan(plan),
                IncomingParam::Closure(closure) => self.insert_imported_callable(closure.invoke()),
            });
        self.insert_exported_return_plan(callable.returns().plan());
    }

    fn insert_imported_callable<S: Surface>(&mut self, callable: &ImportedCallable<S>) {
        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
            self.insert_error_payload(ty);
        }
        callable
            .params()
            .iter()
            .for_each(|parameter| match parameter.payload() {
                OutgoingParam::Value(plan) => self.insert_outgoing_param_plan(plan),
                OutgoingParam::Closure(closure) => self.insert_exported_callable(closure.invoke()),
            });
        self.insert_imported_return_plan(callable.returns().plan());
    }

    fn insert_incoming_param_plan<S: Surface>(&mut self, plan: &ParamPlan<S, IntoRust>) {
        if let ParamPlan::Encoded { ty, codec, .. } = plan {
            self.insert_write_plan(codec);
            self.insert_result_errors(ty);
        }
    }

    fn insert_outgoing_param_plan<S: Surface>(&mut self, plan: &ParamPlan<S, OutOfRust>) {
        if let ParamPlan::Encoded { ty, codec, .. } = plan {
            self.insert_read_plan(codec);
            self.insert_result_errors(ty);
        }
    }

    fn insert_exported_return_plan<S: Surface>(&mut self, plan: &ReturnPlan<S, OutOfRust>) {
        match plan {
            ReturnPlan::EncodedViaReturnSlot { ty, codec, .. }
            | ReturnPlan::EncodedViaOutPointer { ty, codec, .. } => {
                self.insert_read_plan(codec);
                self.insert_result_errors(ty);
            }
            ReturnPlan::ClosureViaOutPointer(closure) => {
                self.insert_exported_callable(closure.invoke());
            }
            _ => {}
        }
    }

    fn insert_imported_return_plan<S: Surface>(&mut self, plan: &ReturnPlan<S, IntoRust>) {
        match plan {
            ReturnPlan::EncodedViaReturnSlot { ty, codec, .. }
            | ReturnPlan::EncodedViaOutPointer { ty, codec, .. } => {
                self.insert_write_plan(codec);
                self.insert_result_errors(ty);
            }
            ReturnPlan::ClosureViaOutPointer(closure) => {
                self.insert_imported_callable(closure.invoke());
            }
            _ => {}
        }
    }

    fn insert_error_payload(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Record(record) => {
                self.records.insert(*record);
            }
            TypeRef::Enum(enumeration) => {
                self.enumerations.insert(*enumeration);
            }
            _ => {}
        }
    }

    fn insert_read_plan(&mut self, plan: &ReadPlan) {
        self.insert_codec_payloads(plan.root());
    }

    fn insert_write_plan(&mut self, plan: &WritePlan) {
        self.insert_codec_payloads(plan.root());
    }

    fn insert_codec_payloads(&mut self, node: &CodecNode) {
        match node {
            CodecNode::DirectRecord(record) => {
                self.codec_records.insert(*record);
            }
            CodecNode::Custom { representation, .. } => {
                self.insert_codec_payloads(representation);
            }
            CodecNode::Optional(inner) => self.insert_codec_payloads(inner),
            CodecNode::Sequence { element, .. } => self.insert_codec_payloads(element),
            CodecNode::Tuple(elements) => elements.iter().for_each(|element| {
                self.insert_codec_payloads(element);
            }),
            CodecNode::Result { ok, err } => {
                self.insert_codec_payloads(ok);
                self.insert_codec_payloads(err);
            }
            CodecNode::Map { key, value, .. } => {
                self.insert_codec_payloads(key);
                self.insert_codec_payloads(value);
            }
            _ => {}
        }
    }

    fn insert_result_errors(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Optional(inner) | TypeRef::Sequence(inner) => self.insert_result_errors(inner),
            TypeRef::Tuple(elements) => elements.iter().for_each(|element| {
                self.insert_result_errors(element);
            }),
            TypeRef::Result { ok, err } => {
                self.insert_result_errors(ok);
                self.insert_error_payload(err);
            }
            TypeRef::Map { key, value, .. } => {
                self.insert_result_errors(key);
                self.insert_result_errors(value);
            }
            _ => {}
        }
    }
}
