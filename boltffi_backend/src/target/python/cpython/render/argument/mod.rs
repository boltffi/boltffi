use boltffi_binding::{
    CallbackId, DirectValueType, DirectVectorElementType, EnumId, HandlePresence, HandleTarget,
    IncomingParam, IntoRust, Native, ParamDecl, ParamPlanRender, Primitive, Receive, RecordId,
    TypeRef, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Identifier, Type, TypeFragment},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::{
        cpython::render::{
            callback, closure, direct, direct_vector, enumeration, primitive, record, result,
        },
        name_style::Name,
    },
};

mod buffered;

pub use self::buffered::MutationOutput;
use self::buffered::{BufferedArgument, RegisteredObject};

pub struct Conversion {
    index: usize,
    name: Identifier,
    kind: Kind,
    primitive: Option<primitive::Runtime>,
}

impl Conversion {
    pub fn from_parameter(
        owner: &str,
        index: usize,
        parameter: &ParamDecl<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function_text()?)?;
        match parameter.payload() {
            IncomingParam::Value(plan) => plan.render_with(&mut ParameterConversion {
                index,
                name,
                bridge,
                context,
            }),
            IncomingParam::Closure(closure) => {
                Self::from_closure(owner, index, name, closure, c_parameters, bridge, context)
            }
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn class_receiver(carrier: native::HandleCarrier) -> Result<Self> {
        Self::handle_with_name(0, Identifier::parse("receiver")?, carrier)
    }

    pub fn direct_record_receiver(
        record: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_record_id(record, bridge, context)?;
        Self::direct_with_name(
            0,
            Identifier::parse("receiver")?,
            direct.c_type().to_owned(),
            direct.parser().clone(),
        )
    }

    pub fn encoded_record_receiver(
        record: RecordId,
        receive: Receive,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = record::Symbols::from_record_id(record, bridge, context)?;
        Self::encoded_with_name(
            0,
            Identifier::parse("receiver")?,
            receive,
            BufferedArgument::RegisteredObject(RegisteredObject::new(
                symbols.parser().clone(),
                symbols.boxer().clone(),
            )),
        )
    }

    pub fn c_style_enum_receiver(
        enumeration: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_enum_id(enumeration, bridge, context)?;
        Self::direct_with_name(
            0,
            Identifier::parse("receiver")?,
            direct.c_type().to_owned(),
            direct.parser().clone(),
        )
    }

    pub fn data_enum_receiver(
        enumeration: EnumId,
        receive: Receive,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = enumeration::Symbols::from_enum_id(enumeration, bridge, context)?;
        Self::encoded_with_name(
            0,
            Identifier::parse("receiver")?,
            receive,
            BufferedArgument::RegisteredObject(RegisteredObject::new(
                symbols.parser().clone(),
                symbols.owned_decoder().clone(),
            )),
        )
    }

    pub fn call_args(&self) -> Result<Vec<c::Expression>> {
        match &self.kind {
            Kind::Direct(direct) => Ok(vec![direct.call_arg(self.name.clone())]),
            Kind::Buffered(buffered) => buffered.call_args(),
            Kind::Closure(closure) => closure
                .call_args()
                .into_iter()
                .map(c::Expression::identifier)
                .map(Ok)
                .collect(),
        }
    }

    pub fn c_arity(&self) -> usize {
        match &self.kind {
            Kind::Direct(_) => 1,
            Kind::Buffered(buffered) => buffered.c_arity(),
            Kind::Closure(_) => closure::Parameter::c_arity(),
        }
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn is_direct(&self) -> bool {
        matches!(self.kind, Kind::Direct(_))
    }

    pub fn is_encoded(&self) -> bool {
        matches!(self.kind, Kind::Buffered(_))
    }

    pub fn is_closure(&self) -> bool {
        matches!(self.kind, Kind::Closure(_))
    }

    pub fn is_string(&self) -> bool {
        false
    }

    pub fn is_bytes(&self) -> bool {
        false
    }

    pub fn is_raw_wire(&self) -> bool {
        matches!(&self.kind, Kind::Buffered(buffered) if buffered.is_raw_wire())
    }

    pub fn has_closure_string_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_string_argument())
    }

    pub fn has_closure_bytes_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_bytes_argument())
    }

    pub fn has_closure_raw_wire_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_raw_wire_argument())
    }

    pub fn wire_primitive(&self) -> Option<primitive::Runtime> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.primitive(),
            Kind::Closure(_) | Kind::Direct(_) => None,
        }
    }

    pub fn closure_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.primitives()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn closure_wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.wire_primitives()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.direct_vector_element(),
            Kind::Closure(_) | Kind::Direct(_) => None,
        }
    }

    pub fn closure_direct_vector_elements(
        &self,
    ) -> impl Iterator<Item = direct_vector::Element> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.direct_vector_elements()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn c_type(&self) -> &c::TypeFragment {
        match &self.kind {
            Kind::Direct(direct) => &direct.c_type,
            Kind::Buffered(_) | Kind::Closure(_) => {
                unreachable!("non-direct parameter has no C type")
            }
        }
    }

    pub fn parser(&self) -> &Identifier {
        match &self.kind {
            Kind::Direct(direct) => &direct.parser,
            Kind::Buffered(buffered) => &buffered.parser,
            Kind::Closure(closure) => closure.parser(),
        }
    }

    pub fn wire(&self) -> &Identifier {
        match &self.kind {
            Kind::Direct(_) => unreachable!("direct parameter has no wire object"),
            Kind::Buffered(buffered) => &buffered.wire,
            Kind::Closure(_) => unreachable!("closure parameter has no wire object"),
        }
    }

    pub fn pointer(&self) -> &Identifier {
        match &self.kind {
            Kind::Direct(_) => unreachable!("direct parameter has no wire pointer"),
            Kind::Buffered(buffered) => &buffered.pointer,
            Kind::Closure(_) => unreachable!("closure parameter has no wire pointer"),
        }
    }

    pub fn length(&self) -> &Identifier {
        match &self.kind {
            Kind::Direct(_) => unreachable!("direct parameter has no wire length"),
            Kind::Buffered(buffered) => &buffered.length,
            Kind::Closure(_) => unreachable!("closure parameter has no wire length"),
        }
    }

    pub fn has_mutation(&self) -> bool {
        matches!(&self.kind, Kind::Buffered(buffered) if buffered.mutation.is_some())
    }

    pub fn mutation_buffer(&self) -> &Identifier {
        match &self.kind {
            Kind::Buffered(buffered) => buffered
                .mutation
                .as_ref()
                .map(MutationOutput::buffer)
                .unwrap_or_else(|| unreachable!("buffered parameter has no mutation output")),
            Kind::Direct(_) | Kind::Closure(_) => {
                unreachable!("non-buffered parameter has no mutation output")
            }
        }
    }

    pub fn mutation(&self) -> Option<MutationOutput> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.mutation.clone(),
            Kind::Direct(_) | Kind::Closure(_) => None,
        }
    }

    pub fn mutation_owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.mutation().and_then(|mutation| mutation.owned_buffer())
    }

    pub fn closure_declaration(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_call_declaration(&self) -> &c::Statement {
        match &self.kind {
            Kind::Closure(closure) => closure.call_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure call declaration")
            }
        }
    }

    pub fn closure_call(&self) -> &Identifier {
        match &self.kind {
            Kind::Closure(closure) => closure.call(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure call")
            }
        }
    }

    pub fn closure_context_declaration(&self) -> &c::Statement {
        match &self.kind {
            Kind::Closure(closure) => closure.context_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure context declaration")
            }
        }
    }

    pub fn closure_context(&self) -> &Identifier {
        match &self.kind {
            Kind::Closure(closure) => closure.context(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure context")
            }
        }
    }

    pub fn closure_release_declaration(&self) -> &c::Statement {
        match &self.kind {
            Kind::Closure(closure) => closure.release_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure release declaration")
            }
        }
    }

    pub fn closure_release(&self) -> &Identifier {
        match &self.kind {
            Kind::Closure(closure) => closure.release(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure release")
            }
        }
    }

    pub fn closure_release_needed(&self) -> &Identifier {
        match &self.kind {
            Kind::Closure(closure) => closure.release_needed(),
            Kind::Direct(_) | Kind::Buffered(_) => {
                unreachable!("non-closure parameter has no closure release flag")
            }
        }
    }

    fn from_direct_slot_with_passing(
        index: usize,
        name: Identifier,
        direct: direct::NativeSlot,
        passing: DirectPassing,
    ) -> Result<Self> {
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: direct.c_type().clone(),
                parser: direct.parser().clone(),
                passing,
            }),
            primitive: direct.primitive(),
        })
    }

    fn from_handle(index: usize, name: Identifier, carrier: native::HandleCarrier) -> Result<Self> {
        Self::handle_with_name(index, name, carrier)
    }

    fn handle_with_name(
        index: usize,
        name: Identifier,
        carrier: native::HandleCarrier,
    ) -> Result<Self> {
        let carrier = primitive::Runtime::native_handle(carrier)?;
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: carrier.c_type()?,
                parser: carrier.parser()?,
                passing: DirectPassing::Value,
            }),
            primitive: Some(carrier),
        })
    }

    fn from_callback(
        index: usize,
        name: Identifier,
        callback: CallbackId,
        presence: HandlePresence,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = callback::Symbols::from_callback_id(callback, bridge, context)?;
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: TypeFragment::anonymous(&Type::CallbackHandle(callback))?,
                parser: symbols.parser(presence).clone(),
                passing: DirectPassing::Value,
            }),
            primitive: None,
        })
    }

    fn from_closure(
        owner: &str,
        index: usize,
        name: Identifier,
        closure: &boltffi_binding::ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let closure = closure::Parameter::new(
            owner,
            index,
            name.clone(),
            closure,
            c_parameters,
            bridge,
            context,
        )?;
        Ok(Self {
            index,
            name,
            kind: Kind::Closure(Box::new(closure)),
            primitive: None,
        })
    }

    fn encoded(
        index: usize,
        name: Identifier,
        receive: Receive,
        encoded: BufferedArgument,
    ) -> Result<Self> {
        Self::encoded_with_name(index, name, receive, encoded)
    }

    fn direct_with_name(
        index: usize,
        name: Identifier,
        c_type: c::TypeFragment,
        parser: Identifier,
    ) -> Result<Self> {
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type,
                parser,
                passing: DirectPassing::Value,
            }),
            primitive: None,
        })
    }

    fn encoded_with_name(
        index: usize,
        name: Identifier,
        receive: Receive,
        encoded: BufferedArgument,
    ) -> Result<Self> {
        let wire = Identifier::parse(format!("{name}_wire"))?;
        let pointer = Identifier::parse(format!("{name}_ptr"))?;
        let length = Identifier::parse(format!("{name}_len"))?;
        let mutation = match receive {
            Receive::ByMutRef => encoded.mutation_output(&name)?,
            Receive::ByValue | Receive::ByRef => None,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown encoded parameter receive mode",
                });
            }
        };
        let parser = encoded.parser()?;
        let primitive = encoded.primitive();
        Ok(Self {
            index,
            name,
            kind: Kind::Buffered(Box::new(BufferedParam {
                argument: encoded,
                parser,
                wire,
                pointer,
                length,
                mutation,
            })),
            primitive,
        })
    }
}

struct ParameterConversion<'render> {
    index: usize,
    name: Identifier,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'render> ParameterConversion<'render> {
    fn direct_type(&self, ty: &DirectValueType, receive: Receive) -> Result<Conversion> {
        match receive {
            Receive::ByValue | Receive::ByRef => {
                let passing = match (ty, receive) {
                    (DirectValueType::Record(_), Receive::ByRef) => DirectPassing::Address,
                    _ => DirectPassing::Value,
                };
                Conversion::from_direct_slot_with_passing(
                    self.index,
                    self.name.clone(),
                    direct::NativeSlot::from_direct_value(ty, self.bridge, self.context)?,
                    passing,
                )
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "borrowed direct parameter",
            }),
        }
    }

    fn handle_type(
        &self,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        receive: Receive,
    ) -> Result<Conversion> {
        match (target, carrier, receive) {
            (HandleTarget::Class(_), carrier, _) => {
                Conversion::from_handle(self.index, self.name.clone(), carrier)
            }
            (
                HandleTarget::Callback(callback),
                native::HandleCarrier::CallbackHandle,
                Receive::ByValue,
            ) => Conversion::from_callback(
                self.index,
                self.name.clone(),
                *callback,
                presence,
                self.bridge,
                self.context,
            ),
            (HandleTarget::Callback(_), _, _) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback handle parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown handle parameter",
            }),
        }
    }
}

impl<'plan, 'render> ParamPlanRender<'plan, Native, IntoRust> for ParameterConversion<'render> {
    type Output = Result<Conversion>;

    fn direct(&mut self, ty: &DirectValueType, receive: Receive) -> Self::Output {
        self.direct_type(ty, receive)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &WritePlan,
        shape: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => Conversion::encoded(
                self.index,
                self.name.clone(),
                receive,
                BufferedArgument::RawWire,
            ),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported encoded parameter",
            }),
        }
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        receive: Receive,
    ) -> Self::Output {
        self.handle_type(target, carrier, presence, receive)
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Conversion::encoded(
            self.index,
            self.name.clone(),
            Receive::ByValue,
            BufferedArgument::OptionalPrimitive(primitive::Runtime::new(primitive)),
        )
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType) -> Self::Output {
        Conversion::encoded(
            self.index,
            self.name.clone(),
            Receive::ByValue,
            BufferedArgument::DirectVector(direct_vector::Element::from_element(
                element,
                self.bridge,
                self.context,
            )?),
        )
    }
}

enum Kind {
    Direct(Direct),
    Buffered(Box<BufferedParam>),
    Closure(Box<closure::Parameter>),
}

struct Direct {
    c_type: c::TypeFragment,
    parser: Identifier,
    passing: DirectPassing,
}

#[derive(Clone, Copy)]
enum DirectPassing {
    Value,
    Address,
}

impl Direct {
    fn call_arg(&self, name: Identifier) -> c::Expression {
        let value = c::Expression::identifier(name);
        match self.passing {
            DirectPassing::Value => value,
            DirectPassing::Address => c::Expression::address_of(value),
        }
    }
}

enum EitherIter<Left, Right> {
    Left(Left),
    Right(Right),
}

impl<Left, Right> EitherIter<Left, Right> {
    fn left(left: Left) -> Self {
        Self::Left(left)
    }

    fn right(right: Right) -> Self {
        Self::Right(right)
    }
}

impl<Item, Left, Right> Iterator for EitherIter<Left, Right>
where
    Left: Iterator<Item = Item>,
    Right: Iterator<Item = Item>,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Left(left) => left.next(),
            Self::Right(right) => right.next(),
        }
    }
}

struct BufferedParam {
    argument: BufferedArgument,
    parser: Identifier,
    wire: Identifier,
    pointer: Identifier,
    length: Identifier,
    mutation: Option<MutationOutput>,
}

impl BufferedParam {
    fn call_args(&self) -> Result<Vec<c::Expression>> {
        self.argument
            .call_args(&self.pointer, &self.length, self.mutation.as_ref())
    }

    fn c_arity(&self) -> usize {
        2 + usize::from(self.mutation.is_some())
    }

    fn is_raw_wire(&self) -> bool {
        self.argument.is_raw_wire()
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.argument.primitive()
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.argument.direct_vector_element()
    }
}
