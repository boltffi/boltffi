use boltffi_binding::{
    DirectValueType, DirectVectorElementType, HandlePresence, HandleTarget, Native, OutOfRust,
    OutgoingParam, ParamPlan,
};

use crate::{
    bridge::c::{CBridgeContract, ParameterGroup},
    core::{Error, RenderContext, Result},
};

use super::super::super::{
    codec::{Reader, Sizer, ValueScope, Writer, primitive_read_method, primitive_write_method},
    name_style::Name,
    native::NativeParameterSource,
    render::direct_vector::PrimitiveVector,
    syntax::{Identifier, Parameter, TypeFragment},
    type_name,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackParameter {
    signature: Parameter,
    entry_setup: Vec<String>,
    entry_argument: String,
    proxy_setup: Vec<String>,
    proxy_arguments: Vec<String>,
}

impl CallbackParameter {
    pub fn from_declaration(
        parameter: &boltffi_binding::ParamDecl<Native, OutOfRust>,
        group: &ParameterGroup,
        parameters: &impl NativeParameterSource,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let OutgoingParam::Value(plan) = parameter.payload() else {
            return super::unsupported("callback closure parameter");
        };
        let name = Name::new(parameter.name()).lower_camel()?;
        match plan {
            ParamPlan::Direct { ty, .. } => Self::direct(name, ty, group, parameters, context),
            ParamPlan::Encoded { ty, codec, .. } => {
                let ParameterGroup::ByteSlice(bytes) = group else {
                    return broken("encoded callback parameter group");
                };
                let pointer = super::super::super::native::parameter_name(
                    parameters.parameter(bytes.pointer()).name(),
                )?;
                let length = super::super::super::native::parameter_name(
                    parameters.parameter(bytes.length()).name(),
                )?;
                let reader = format!("_l${name}Reader");
                let storage = format!("_l${name}Storage");
                let writer = format!("_l${name}Writer");
                let public_type = type_name::type_ref(ty, context)?;
                let source = name.to_string();
                let size = codec
                    .write_self_value()
                    .size_with(&mut Sizer::new(
                        ValueScope::current(source.clone()),
                        context,
                    ))?
                    .into_source();
                let writes = codec
                    .write_self_value()
                    .render_with(&mut Writer::new(
                        &writer,
                        ValueScope::current(source),
                        context,
                    ))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(super::super::super::codec::WriteStatement::into_source)
                    .collect::<Vec<_>>();
                Ok(Self::new(
                    name,
                    public_type,
                    vec![format!(
                        "final {reader} = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({pointer}, {length}));"
                    )],
                    codec
                        .render_with(&mut Reader::new(&reader, context))?
                        .into_source(),
                    vec![
                        format!("final {storage} = _$$BoltCallocPtr<$$ffi.Uint8>.alloc({size});"),
                        format!(
                            "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
                        ),
                        writes.join("\n"),
                    ],
                    vec![format!("{storage}.ptr"), format!("{writer}.len")],
                ))
            }
            ParamPlan::ScalarOption { primitive } => {
                let ParameterGroup::ByteSlice(bytes) = group else {
                    return broken("scalar-option callback parameter group");
                };
                let pointer = super::super::super::native::parameter_name(
                    parameters.parameter(bytes.pointer()).name(),
                )?;
                let length = super::super::super::native::parameter_name(
                    parameters.parameter(bytes.length()).name(),
                )?;
                let reader = format!("_l${name}Reader");
                let storage = format!("_l${name}Storage");
                let writer = format!("_l${name}Writer");
                let public_type = type_name::primitive_type(*primitive)?.optional();
                let proxy_setup = vec![
                    format!(
                        "final {storage} = _$$BoltCallocPtr<$$ffi.Uint8>.alloc(1 + {});",
                        super::super::super::codec::primitive_size(*primitive)
                    ),
                    format!(
                        "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
                    ),
                    format!(
                        "if ({name} == null) {{\n  {writer}.writeU8(0);\n}} else {{\n  {writer}.writeU8(1);\n  {writer}.{}({name});\n}}",
                        primitive_write_method(*primitive)
                    ),
                ];
                Ok(Self::new(
                    name,
                    public_type,
                    vec![format!(
                        "final {reader} = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({pointer}, {length}));"
                    )],
                    format!(
                        "{reader}.readU8() == 0 ? null : {reader}.{}()",
                        primitive_read_method(*primitive)
                    ),
                    proxy_setup,
                    vec![format!("{storage}.ptr"), format!("{writer}.len")],
                ))
            }
            ParamPlan::DirectVec { element, .. } => {
                Self::direct_vector(name, element, group, parameters, bridge, context)
            }
            ParamPlan::Handle {
                target, presence, ..
            } => Self::handle(name, target, *presence, group, parameters, context),
            _ => super::unsupported("unknown callback parameter crossing"),
        }
    }

    fn direct(
        name: Identifier,
        ty: &DirectValueType,
        group: &ParameterGroup,
        parameters: &impl NativeParameterSource,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ParameterGroup::Value(index) = group else {
            return broken("direct callback parameter group");
        };
        let native_name =
            super::super::super::native::parameter_name(parameters.parameter(*index).name())?;
        let (entry_argument, proxy_argument) = match ty {
            DirectValueType::Primitive(_) => (native_name.to_owned(), name.to_string()),
            DirectValueType::Enum(_) => (
                format!(
                    "{}._m$fromDiscriminant({native_name})",
                    type_name::direct_value(ty, context)?
                ),
                format!("{name}.value"),
            ),
            DirectValueType::Record(_) => (
                format!(
                    "{}._m$fromStruct({native_name})",
                    type_name::direct_value(ty, context)?
                ),
                format!("{name}._m$toStruct()"),
            ),
            _ => return super::unsupported("unknown direct callback parameter"),
        };
        let public_type = type_name::direct_value(ty, context)?;
        Ok(Self::new(
            name,
            public_type,
            Vec::new(),
            entry_argument,
            Vec::new(),
            vec![proxy_argument],
        ))
    }

    fn direct_vector(
        name: Identifier,
        element: &DirectVectorElementType,
        group: &ParameterGroup,
        parameters: &impl NativeParameterSource,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ParameterGroup::DirectVector(vector) = group else {
            return broken("direct-vector callback parameter group");
        };
        let pointer = super::super::super::native::parameter_name(
            parameters.parameter(vector.pointer()).name(),
        )?;
        let length = super::super::super::native::parameter_name(
            parameters.parameter(vector.length()).name(),
        )?;
        let public_type = type_name::direct_vector(element, context)?;
        match element {
            DirectVectorElementType::Primitive(primitive) => {
                let primitive = primitive.primitive();
                let vector = PrimitiveVector::new(primitive)?;
                let native = vector.native();
                let storage = format!("_l${name}Storage");
                let entry_argument = vector.copied_from(&pointer, &length)?;
                let populate = vector.populate(&storage, name.as_str())?;
                let proxy_setup = vec![
                    format!(
                        "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>() * {name}.length);",
                        native.native(),
                        native.native()
                    ),
                    populate,
                ];
                let proxy_arguments = vec![format!("{storage}.ptr"), format!("{name}.length")];
                Ok(Self::new(
                    name,
                    public_type,
                    Vec::new(),
                    entry_argument,
                    proxy_setup,
                    proxy_arguments,
                ))
            }
            DirectVectorElementType::Record(record) => {
                let public_record =
                    type_name::direct_value(&DirectValueType::Record(*record), context)?;
                // The C bridge crosses a direct-record vector as packed bytes
                // (see `DirectVectorElementType::Record` in
                // `bridge/c/parameter/direct_vector.rs`), so the pointer
                // parameter's own type is `Uint8`, not the record struct --
                // the native struct name has to come from the record's own
                // registration on the bridge instead.
                let native = super::super::super::native::direct_record_struct(bridge, *record)?;
                let storage = format!("_l${name}Storage");
                let entry_setup = vec![format!(
                    "final _l${name}Count = {length} ~/ $$ffi.sizeOf<{}>();",
                    native
                )];
                let entry_argument = format!(
                    "List<{public_record}>.generate(_l${name}Count, (_l$index) => {public_record}._m$fromStruct({pointer}.cast<{}>().elementAt(_l$index).ref))",
                    native
                );
                let proxy_setup = vec![
                    format!(
                        "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>() * {name}.length);",
                        native, native
                    ),
                    format!(
                        "for (var _l$index = 0; _l$index < {name}.length; _l$index++) {{ {name}[_l$index]._m$writeStruct({storage}.ptr.elementAt(_l$index)); }}"
                    ),
                ];
                let proxy_arguments = vec![
                    format!("{storage}.ptr.cast<$$ffi.Uint8>()"),
                    format!("{name}.length * $$ffi.sizeOf<{native}>()"),
                ];
                Ok(Self::new(
                    name,
                    public_type,
                    entry_setup,
                    entry_argument,
                    proxy_setup,
                    proxy_arguments,
                ))
            }
            _ => super::unsupported("unknown direct-vector callback element"),
        }
    }

    fn handle(
        name: Identifier,
        target: &HandleTarget,
        presence: HandlePresence,
        group: &ParameterGroup,
        parameters: &impl NativeParameterSource,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ParameterGroup::Value(index) = group else {
            return broken("handle callback parameter group");
        };
        let native_name =
            super::super::super::native::parameter_name(parameters.parameter(*index).name())?;
        let required_type = type_name::handle(target, HandlePresence::Required, context)?;
        let (entry_argument, proxy_argument) = match target {
            HandleTarget::Class(_) => match presence {
                HandlePresence::Required => (
                    format!("{required_type}._({native_name})"),
                    format!("{name}._handle"),
                ),
                HandlePresence::Nullable => (
                    format!("{native_name} == 0 ? null : {required_type}._({native_name})"),
                    format!("{name}?._handle ?? 0"),
                ),
                _ => return super::unsupported("unknown callback class-handle presence"),
            },
            HandleTarget::Callback(_) => match presence {
                HandlePresence::Required => (
                    format!("{required_type}Bridge.wrap({native_name})"),
                    format!("{required_type}Bridge.create({name})"),
                ),
                HandlePresence::Nullable => (
                    format!(
                        "{native_name}.handle == 0 ? null : {required_type}Bridge.wrap({native_name})"
                    ),
                    format!("{required_type}Bridge.create({name})"),
                ),
                _ => return super::unsupported("unknown callback callback-handle presence"),
            },
            HandleTarget::Stream(_) => {
                return super::unsupported("callback stream handle parameter");
            }
            _ => return super::unsupported("unknown callback handle target"),
        };
        let public_type = type_name::handle(target, presence, context)?;
        Ok(Self::new(
            name,
            public_type,
            Vec::new(),
            entry_argument,
            Vec::new(),
            vec![proxy_argument],
        ))
    }

    fn new(
        name: Identifier,
        public_type: TypeFragment,
        entry_setup: Vec<String>,
        entry_argument: String,
        proxy_setup: Vec<String>,
        proxy_arguments: Vec<String>,
    ) -> Self {
        Self {
            signature: Parameter::new(name, public_type),
            entry_setup,
            entry_argument,
            proxy_setup,
            proxy_arguments,
        }
    }

    pub fn signature(&self) -> &Parameter {
        &self.signature
    }

    pub fn public_type(&self) -> &TypeFragment {
        self.signature.ty()
    }

    pub fn entry_setup(&self) -> &[String] {
        &self.entry_setup
    }

    pub fn entry_argument(&self) -> &str {
        &self.entry_argument
    }

    pub fn proxy_setup(&self) -> &[String] {
        &self.proxy_setup
    }

    pub fn proxy_arguments(&self) -> &[String] {
        &self.proxy_arguments
    }
}

pub fn group_indices(group: &ParameterGroup) -> Result<Vec<usize>> {
    match group {
        ParameterGroup::Value(index) => Ok(vec![index.position()]),
        ParameterGroup::ByteSlice(bytes) => {
            Ok(vec![bytes.pointer().position(), bytes.length().position()])
        }
        ParameterGroup::DirectVector(vector) => Ok(vec![
            vector.pointer().position(),
            vector.length().position(),
        ]),
        _ => broken("callback source parameter group"),
    }
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
