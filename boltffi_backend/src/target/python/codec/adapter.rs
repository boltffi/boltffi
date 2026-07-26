use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
};

use boltffi_binding::{
    Bindings, CallableDecl, ClosureReturn, Decl, DeclarationRef, DirectValueType,
    DirectVectorElementType, ErrorChannel, ErrorDecl, ExportedCallable, ForeignBody,
    HandlePresence, HandleTarget, ImportedCallable, IncomingParam, IntoRust, Native, OutOfRust,
    OutgoingParam, ParamPlanRender, Primitive, ReadPlan, Receive, ReturnPlan, ReturnPlanRender,
    ReturnValueSlot, RustBody, TypeRef, ValueRoot, WritePlan, native,
};

use crate::{
    bridge::c::{Identifier as CIdentifier, Literal as CLiteral},
    core::{Error, Result},
    target::python::{
        codec::Expression,
        name_style::Name,
        render::Package,
        syntax::{
            Expression as PythonExpression, Identifier as PythonIdentifier,
            Literal as PythonLiteral,
        },
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecAdapters<'binding> {
    decoders: Vec<ReadAdapter<'binding>>,
    encoders: Vec<WriteAdapter<'binding>>,
}

impl<'binding> CodecAdapters<'binding> {
    pub fn from_declarations(
        bindings: &'binding Bindings<Native>,
        declarations: &[DeclarationRef<'binding, Native>],
    ) -> Self {
        let collected = bindings
            .decls()
            .iter()
            .filter(|declaration| declarations.contains(&DeclarationRef::from(*declaration)))
            .fold(
                CollectedAdapters::default(),
                CollectedAdapters::collect_decl,
            );
        Self {
            decoders: collected.decoders,
            encoders: collected.encoders,
        }
    }

    pub fn decoders(&self) -> &[ReadAdapter<'binding>] {
        &self.decoders
    }

    pub fn encoders(&self) -> &[WriteAdapter<'binding>] {
        &self.encoders
    }

    pub fn is_empty(&self) -> bool {
        self.decoders.is_empty() && self.encoders.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadAdapter<'binding> {
    key: AdapterKey,
    plan: &'binding ReadPlan,
}

impl<'binding> ReadAdapter<'binding> {
    pub fn new(plan: &'binding ReadPlan) -> Self {
        Self {
            key: AdapterKey::read(plan),
            plan,
        }
    }

    pub fn key(&self) -> &AdapterKey {
        &self.key
    }

    pub fn plan(&self) -> &'binding ReadPlan {
        self.plan
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteAdapter<'binding> {
    key: AdapterKey,
    plan: &'binding WritePlan,
}

impl<'binding> WriteAdapter<'binding> {
    pub fn new(plan: &'binding WritePlan) -> Self {
        Self {
            key: AdapterKey::write(plan),
            plan,
        }
    }

    pub fn key(&self) -> &AdapterKey {
        &self.key
    }

    pub fn plan(&self) -> &'binding WritePlan {
        self.plan
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AdapterKey {
    stem: String,
}

impl AdapterKey {
    pub fn read(plan: &ReadPlan) -> Self {
        Self::from_hash("read", plan)
    }

    pub fn write(plan: &WritePlan) -> Self {
        Self::from_hash("write", plan)
    }

    pub fn python_literal(&self) -> PythonLiteral {
        PythonLiteral::string(&self.stem)
    }

    pub fn c_literal(&self) -> CLiteral {
        CLiteral::string(&self.stem)
    }

    pub fn python_function(&self) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!("_boltffi_{}", self.stem))
    }

    pub fn c_decoder(&self) -> Result<CIdentifier> {
        CIdentifier::parse(format!("boltffi_python_decode_{}", self.stem))
    }

    pub fn c_encoder(&self) -> Result<CIdentifier> {
        CIdentifier::parse(format!("boltffi_python_encode_{}", self.stem))
    }

    fn from_hash(prefix: &str, value: impl Hash) -> Self {
        let mut hasher = StableHasher::default();
        value.hash(&mut hasher);
        Self {
            stem: format!("{prefix}_{:016x}", hasher.finish()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadFunction {
    key: PythonLiteral,
    name: PythonIdentifier,
    expression: PythonExpression,
}

impl ReadFunction {
    pub fn from_adapter<'adapter, 'package>(
        adapter: &ReadAdapter<'adapter>,
        package: &Package<'package>,
    ) -> Result<Self> {
        Ok(Self {
            key: adapter.key().python_literal(),
            name: adapter.key().python_function()?,
            expression: Expression::read(adapter.plan(), package)?.into_expression(),
        })
    }

    pub fn key(&self) -> &PythonLiteral {
        &self.key
    }

    pub fn name(&self) -> &PythonIdentifier {
        &self.name
    }

    pub fn expression(&self) -> &PythonExpression {
        &self.expression
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteFunction {
    key: PythonLiteral,
    name: PythonIdentifier,
    argument: PythonIdentifier,
    expression: PythonExpression,
}

impl WriteFunction {
    pub fn from_adapter<'adapter, 'package>(
        adapter: &WriteAdapter<'adapter>,
        package: &Package<'package>,
    ) -> Result<Self> {
        Ok(Self {
            key: adapter.key().python_literal(),
            name: adapter.key().python_function()?,
            argument: Self::plan_argument(adapter.plan())?,
            expression: Expression::write(adapter.plan(), package)?.into_expression(),
        })
    }

    pub fn key(&self) -> &PythonLiteral {
        &self.key
    }

    pub fn name(&self) -> &PythonIdentifier {
        &self.name
    }

    pub fn argument(&self) -> &PythonIdentifier {
        &self.argument
    }

    pub fn expression(&self) -> &PythonExpression {
        &self.expression
    }

    fn plan_argument(plan: &WritePlan) -> Result<PythonIdentifier> {
        match plan.value().root() {
            ValueRoot::SelfValue => PythonIdentifier::parse("self"),
            ValueRoot::Named(name) | ValueRoot::Local(name) => Name::new(name).function(),
            ValueRoot::Binder(binder) => {
                PythonIdentifier::parse(format!("__boltffi_value_{}", binder.raw()))
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown codec adapter value root",
            }),
        }
    }
}

#[derive(Default)]
struct CollectedAdapters<'binding> {
    decoder_keys: BTreeSet<AdapterKey>,
    encoder_keys: BTreeSet<AdapterKey>,
    decoders: Vec<ReadAdapter<'binding>>,
    encoders: Vec<WriteAdapter<'binding>>,
}

impl<'binding> CollectedAdapters<'binding> {
    fn collect_decl(self, decl: &'binding Decl<Native>) -> Self {
        decl.exported_callables()
            .fold(self, Self::collect_exported_callable)
            .collect_imported_callables(decl.imported_callables())
    }

    fn collect_imported_callables(
        self,
        callables: impl IntoIterator<Item = &'binding ImportedCallable<Native>>,
    ) -> Self {
        callables
            .into_iter()
            .fold(self, Self::collect_imported_callable)
    }

    fn collect_exported_callable(mut self, callable: &'binding ExportedCallable<Native>) -> Self {
        self.collect_rust_callable(callable);
        self
    }

    fn collect_imported_callable(mut self, callable: &'binding ImportedCallable<Native>) -> Self {
        self.collect_foreign_callable(callable);
        self
    }

    fn collect_rust_callable(&mut self, callable: &'binding CallableDecl<Native, RustBody>) {
        callable
            .params()
            .iter()
            .for_each(|param| self.collect_incoming_param(param.payload()));
        self.collect_out_of_rust_return(callable.returns().plan());
        self.collect_out_of_rust_error(callable.error());
    }

    fn collect_foreign_callable(&mut self, callable: &'binding CallableDecl<Native, ForeignBody>) {
        callable
            .params()
            .iter()
            .for_each(|param| self.collect_outgoing_param(param.payload()));
        self.collect_into_rust_return(callable.returns().plan());
        self.collect_into_rust_error(callable.error());
    }

    fn collect_incoming_param(&mut self, param: &'binding IncomingParam<Native>) {
        match param {
            IncomingParam::Value(plan) => {
                plan.render_with(&mut IncomingParamAdapters { adapters: self })
            }
            IncomingParam::Closure(closure) => self.collect_foreign_callable(closure.invoke()),
        }
    }

    fn collect_outgoing_param(&mut self, param: &'binding OutgoingParam<Native>) {
        match param {
            OutgoingParam::Value(plan) => {
                plan.render_with(&mut OutgoingParamAdapters { adapters: self })
            }
            OutgoingParam::Closure(closure) => self.collect_rust_callable(closure.invoke()),
        }
    }

    fn collect_out_of_rust_return(&mut self, plan: &'binding ReturnPlan<Native, OutOfRust>) {
        plan.render_with(&mut OutOfRustReturnAdapters { adapters: self });
    }

    fn collect_into_rust_return(&mut self, plan: &'binding ReturnPlan<Native, IntoRust>) {
        plan.render_with(&mut IntoRustReturnAdapters { adapters: self });
    }

    fn collect_out_of_rust_error(&mut self, error: &'binding ErrorDecl<Native, OutOfRust>) {
        if let ErrorChannel::Encoded { codec, .. } = error.channel() {
            self.insert_decoder(codec);
        }
    }

    fn collect_into_rust_error(&mut self, error: &'binding ErrorDecl<Native, IntoRust>) {
        if let ErrorChannel::Encoded { codec, .. } = error.channel() {
            self.insert_encoder(codec);
        }
    }

    fn insert_decoder(&mut self, plan: &'binding ReadPlan) {
        let adapter = ReadAdapter::new(plan);
        if self.decoder_keys.insert(adapter.key().clone()) {
            self.decoders.push(adapter);
        }
    }

    fn insert_encoder(&mut self, plan: &'binding WritePlan) {
        let adapter = WriteAdapter::new(plan);
        if self.encoder_keys.insert(adapter.key().clone()) {
            self.encoders.push(adapter);
        }
    }
}

struct IncomingParamAdapters<'collector, 'binding> {
    adapters: &'collector mut CollectedAdapters<'binding>,
}

impl<'collector, 'binding> ParamPlanRender<'binding, Native, IntoRust>
    for IncomingParamAdapters<'collector, 'binding>
{
    type Output = ();

    fn direct(&mut self, _: &'binding DirectValueType, _: Receive) {}

    fn encoded(
        &mut self,
        _: &'binding TypeRef,
        codec: &'binding WritePlan,
        _: native::BufferShape,
        _: Receive,
    ) {
        self.adapters.insert_encoder(codec);
    }

    fn handle(
        &mut self,
        _: &'binding HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'binding DirectVectorElementType, _: Receive) {}
}

struct OutgoingParamAdapters<'collector, 'binding> {
    adapters: &'collector mut CollectedAdapters<'binding>,
}

impl<'collector, 'binding> ParamPlanRender<'binding, Native, OutOfRust>
    for OutgoingParamAdapters<'collector, 'binding>
{
    type Output = ();

    fn direct(&mut self, _: &'binding DirectValueType, _: ()) {}

    fn encoded(
        &mut self,
        _: &'binding TypeRef,
        codec: &'binding ReadPlan,
        _: native::BufferShape,
        _: (),
    ) {
        self.adapters.insert_decoder(codec);
    }

    fn handle(
        &mut self,
        _: &'binding HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'binding DirectVectorElementType, _: ()) {}
}

struct OutOfRustReturnAdapters<'collector, 'binding> {
    adapters: &'collector mut CollectedAdapters<'binding>,
}

impl<'collector, 'binding> ReturnPlanRender<'binding, Native, OutOfRust>
    for OutOfRustReturnAdapters<'collector, 'binding>
{
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'binding DirectValueType) {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        _: &'binding TypeRef,
        codec: &'binding ReadPlan,
        _: native::BufferShape,
    ) {
        self.adapters.insert_decoder(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'binding HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'binding DirectVectorElementType) {}

    fn closure(&mut self, closure: &'binding ClosureReturn<Native, OutOfRust>) {
        self.adapters.collect_rust_callable(closure.invoke());
    }
}

struct IntoRustReturnAdapters<'collector, 'binding> {
    adapters: &'collector mut CollectedAdapters<'binding>,
}

impl<'collector, 'binding> ReturnPlanRender<'binding, Native, IntoRust>
    for IntoRustReturnAdapters<'collector, 'binding>
{
    type Output = ();

    fn void(&mut self) {}

    fn direct(&mut self, _: ReturnValueSlot, _: &'binding DirectValueType) {}

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        _: &'binding TypeRef,
        codec: &'binding WritePlan,
        _: native::BufferShape,
    ) {
        self.adapters.insert_encoder(codec);
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'binding HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) {
    }

    fn scalar_option(&mut self, _: Primitive) {}

    fn direct_vector(&mut self, _: &'binding DirectVectorElementType) {}

    fn closure(&mut self, closure: &'binding ClosureReturn<Native, IntoRust>) {
        self.adapters.collect_foreign_callable(closure.invoke());
    }
}

struct StableHasher {
    hash: u64,
}

impl Default for StableHasher {
    fn default() -> Self {
        Self {
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|byte| {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
        });
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64);
    }
}
