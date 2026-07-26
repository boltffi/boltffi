use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, CanonicalName, DataEnumDecl, EnumDecl, EnumId,
    ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, Receive,
};

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        python_cext::{ExtensionMethod, MethodFlags, MethodName, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::{
            codec,
            render::{argument, direct_vector, function, primitive, result},
        },
        name_style::Name,
        syntax::Identifier as PythonIdentifier,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/enumeration.c", escape = "none")]
struct CStyleTemplate {
    class_name: PythonIdentifier,
    c_type: TypeFragment,
    registration: Identifier,
    members_by_wire_tag: Identifier,
    member_names: Identifier,
    member_native_values: Identifier,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    load_member: Identifier,
    parser: Identifier,
    wire_encoder: Identifier,
    boxer: Identifier,
    owned_decoder: Identifier,
    box_from_wire_tag: Identifier,
    native_to_wire_tag: Identifier,
    repr_parser: Identifier,
    repr_boxer: Identifier,
    repr_wire_size: usize,
    variants: Vec<Variant>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/data_enum.c", escape = "none")]
struct DataTemplate {
    class_name: PythonIdentifier,
    type_object: Identifier,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    wire_encoder: Identifier,
    owned_decoder: Identifier,
}

pub struct Enumeration {
    symbols: Symbols,
    shape: Shape,
    method: ExtensionMethod,
    callables: Vec<function::Function>,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, bridge, context),
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown enum",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        let symbols = self.symbols;
        let source = match self.shape {
            Shape::CStyle {
                variants,
                primitive,
            } => {
                let registration = symbols.registration()?;
                let members_by_wire_tag = symbols.members_by_wire_tag()?;
                let member_names = symbols.member_names()?;
                let member_native_values = symbols.member_native_values()?;
                let load_member = symbols.load_member()?;
                let box_from_wire_tag = symbols.box_from_wire_tag()?;
                let native_to_wire_tag = symbols.native_to_wire_tag()?;
                let c_type = symbols.c_type()?.clone();
                CStyleTemplate {
                    class_name: symbols.class_name,
                    c_type,
                    registration,
                    members_by_wire_tag,
                    member_names,
                    member_native_values,
                    register_method: symbols.register_method,
                    register_wrapper: symbols.register_wrapper,
                    load_member,
                    parser: symbols.parser,
                    wire_encoder: symbols.wire_encoder,
                    boxer: symbols.boxer,
                    owned_decoder: symbols.owned_decoder,
                    box_from_wire_tag,
                    native_to_wire_tag,
                    repr_parser: primitive.parser()?,
                    repr_boxer: primitive.boxer()?,
                    repr_wire_size: primitive.wire_size()?,
                    variants,
                }
                .render()?
            }
            Shape::Data => {
                let type_object = symbols.type_object()?;
                DataTemplate {
                    class_name: symbols.class_name,
                    type_object,
                    register_method: symbols.register_method,
                    register_wrapper: symbols.register_wrapper,
                    wire_encoder: symbols.parser,
                    owned_decoder: symbols.boxer,
                }
                .render()?
            }
        };
        let callables = self
            .callables
            .into_iter()
            .map(function::Function::render)
            .collect::<Result<Vec<_>>>()?;
        Ok(Emitted::primary(
            std::iter::once(source)
                .chain(
                    callables
                        .into_iter()
                        .map(|emitted| emitted.primary_chunk().as_str().to_owned()),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        std::iter::once(&self.method)
            .chain(self.callables.iter().flat_map(function::Function::methods))
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        match self.shape {
            Shape::CStyle { primitive, .. } => Some(primitive),
            Shape::Data => None,
        }
    }

    pub fn cleanup(&self) -> c::Statement {
        match &self.shape {
            Shape::CStyle { .. } => c::Statement::new(format!(
                "boltffi_python_clear_c_style_enum_registration(&{})",
                self.symbols
                    .registration
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            )),
            Shape::Data => c::Statement::new(format!(
                "Py_CLEAR({})",
                self.symbols
                    .type_object
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            )),
        }
    }

    pub fn needs_owned_buffer(&self) -> bool {
        matches!(self.shape, Shape::Data)
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::owned_buffers)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::direct_vector_elements)
    }

    pub fn native_sequences(&self) -> impl Iterator<Item = codec::NativeSequence> + '_ {
        self.callables
            .iter()
            .flat_map(function::Function::native_sequences)
    }

    pub fn has_string_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::has_raw_wire_argument)
    }

    pub fn uses_async_protocol(&self) -> bool {
        self.callables
            .iter()
            .any(function::Function::uses_async_protocol)
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if enumeration.variants().is_empty() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "empty c-style enum",
            });
        }
        let c_enum =
            bridge
                .source_c_style_enum(enumeration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "c-style enum without C typedef",
                })?;
        if enumeration.variants().len() != c_enum.variants().len() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "enum variant mismatch",
            });
        }
        let symbols = Symbols::from_c_style(enumeration, c_enum)?;
        let variants = enumeration
            .variants()
            .iter()
            .enumerate()
            .map(|(index, variant)| Variant::new(index, variant))
            .collect::<Result<Vec<_>>>()?;
        let method = ExtensionMethod::new(
            MethodName::parse(symbols.register_method.as_str())?,
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::c_style_callables(enumeration, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::CStyle {
                variants,
                primitive: primitive::Runtime::new(enumeration.repr().primitive()),
            },
            method,
            callables,
        })
    }

    fn from_data(
        enumeration: &DataEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_data(enumeration)?;
        let method = ExtensionMethod::new(
            MethodName::parse(symbols.register_method.as_str())?,
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::data_callables(enumeration, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Data,
            method,
            callables,
        })
    }

    fn c_style_callables(
        enumeration: &CStyleEnumDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Function>> {
        let initializers = enumeration
            .initializers()
            .iter()
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = enumeration
            .methods()
            .iter()
            .filter(|method| !matches!(method.callable().receiver(), Some(Receive::ByMutRef)))
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| {
                        argument::Conversion::c_style_enum_receiver(
                            enumeration.id(),
                            receive,
                            bridge,
                            context,
                        )
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn data_callables(
        enumeration: &DataEnumDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Function>> {
        let initializers = enumeration
            .initializers()
            .iter()
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = enumeration
            .methods()
            .iter()
            .filter(|method| !matches!(method.callable().receiver(), Some(Receive::ByMutRef)))
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| {
                        argument::Conversion::data_enum_receiver(
                            enumeration.id(),
                            receive,
                            bridge,
                            context,
                        )
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Function> {
        function::Function::from_export(
            symbols.initializer(initializer.name())?,
            initializer.symbol(),
            initializer.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    fn method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &Symbols,
        receiver: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Function> {
        function::Function::from_export(
            symbols.method(method.name())?,
            method.target(),
            method.callable(),
            receiver,
            bridge,
            context,
        )
    }
}

pub struct Symbols {
    class_name: PythonIdentifier,
    stem: String,
    c_type: Option<TypeFragment>,
    type_object: Option<Identifier>,
    registration: Option<Identifier>,
    members_by_wire_tag: Option<Identifier>,
    member_names: Option<Identifier>,
    member_native_values: Option<Identifier>,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    load_member: Option<Identifier>,
    parser: Identifier,
    wire_encoder: Identifier,
    boxer: Identifier,
    owned_decoder: Identifier,
    box_from_wire_tag: Option<Identifier>,
    native_to_wire_tag: Option<Identifier>,
}

impl Symbols {
    pub fn from_enum_id(
        enum_id: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let enumeration = context
            .enumeration(enum_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum id without declaration",
            })?;
        match enumeration {
            EnumDecl::CStyle(enumeration) => {
                let c_enum =
                    bridge
                        .source_c_style_enum(enum_id)
                        .ok_or(Error::UnsupportedTarget {
                            target: "python",
                            shape: "c-style enum without C typedef",
                        })?;
                Self::from_c_style(enumeration, c_enum)
            }
            EnumDecl::Data(enumeration) => Self::from_data(enumeration),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown enum declaration",
            }),
        }
    }

    pub fn c_type(&self) -> Result<&TypeFragment> {
        self.c_type.as_ref().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C type",
        })
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn boxer(&self) -> &Identifier {
        &self.boxer
    }

    pub fn owned_decoder(&self) -> &Identifier {
        &self.owned_decoder
    }

    pub fn stem(&self) -> &str {
        &self.stem
    }

    pub fn class_name(&self) -> &PythonIdentifier {
        &self.class_name
    }

    pub fn register_method(&self) -> &PythonIdentifier {
        &self.register_method
    }

    pub fn initializer(&self, name: &CanonicalName) -> Result<PythonIdentifier> {
        self.callable(name)
    }

    pub fn method(&self, name: &CanonicalName) -> Result<PythonIdentifier> {
        self.callable(name)
    }

    pub fn from_c_style(enumeration: &CStyleEnumDecl<Native>, c_enum: &c::Enum) -> Result<Self> {
        let stem = Identifier::escape(Name::new(enumeration.name()).function_text()?)?.to_string();
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(enumeration.name()).class())?,
            stem: stem.clone(),
            c_type: Some(TypeFragment::anonymous(&c::Type::named(c_enum.name())?)?),
            type_object: None,
            registration: Some(Identifier::parse(format!(
                "boltffi_python_{stem}_registration"
            ))?),
            members_by_wire_tag: Some(Identifier::parse(format!(
                "boltffi_python_{stem}_members_by_wire_tag"
            ))?),
            member_names: Some(Identifier::parse(format!(
                "boltffi_python_{stem}_member_names"
            ))?),
            member_native_values: Some(Identifier::parse(format!(
                "boltffi_python_{stem}_member_native_values"
            ))?),
            register_method: PythonIdentifier::parse(format!("_register_{stem}"))?,
            register_wrapper: Identifier::parse(format!("boltffi_python_wrapper_register_{stem}"))?,
            load_member: Some(Identifier::parse(format!(
                "boltffi_python_load_{stem}_member"
            ))?),
            parser: Identifier::parse(format!("boltffi_python_parse_{stem}"))?,
            wire_encoder: Identifier::parse(format!("boltffi_python_wire_{stem}"))?,
            boxer: Identifier::parse(format!("boltffi_python_box_{stem}"))?,
            owned_decoder: Identifier::parse(format!("boltffi_python_decode_owned_{stem}"))?,
            box_from_wire_tag: Some(Identifier::parse(format!(
                "boltffi_python_box_{stem}_from_wire_tag"
            ))?),
            native_to_wire_tag: Some(Identifier::parse(format!(
                "boltffi_python_{stem}_native_to_wire_tag"
            ))?),
        })
    }

    pub fn from_data(enumeration: &DataEnumDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(enumeration.name()).function_text()?)?.to_string();
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(enumeration.name()).class())?,
            stem: stem.clone(),
            c_type: None,
            type_object: Some(Identifier::parse(format!("boltffi_python_{stem}_type"))?),
            registration: None,
            members_by_wire_tag: None,
            member_names: None,
            member_native_values: None,
            register_method: PythonIdentifier::parse(format!("_register_{stem}"))?,
            register_wrapper: Identifier::parse(format!("boltffi_python_wrapper_register_{stem}"))?,
            load_member: None,
            parser: Identifier::parse(format!("boltffi_python_wire_{stem}"))?,
            wire_encoder: Identifier::parse(format!("boltffi_python_wire_{stem}"))?,
            boxer: Identifier::parse(format!("boltffi_python_decode_owned_{stem}"))?,
            owned_decoder: Identifier::parse(format!("boltffi_python_decode_owned_{stem}"))?,
            box_from_wire_tag: None,
            native_to_wire_tag: None,
        })
    }

    fn registration(&self) -> Result<Identifier> {
        self.registration.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style registration",
        })
    }

    fn members_by_wire_tag(&self) -> Result<Identifier> {
        self.members_by_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style member table",
            })
    }

    fn member_names(&self) -> Result<Identifier> {
        self.member_names.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style member names",
        })
    }

    fn member_native_values(&self) -> Result<Identifier> {
        self.member_native_values
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style native values",
            })
    }

    fn load_member(&self) -> Result<Identifier> {
        self.load_member.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style member loader",
        })
    }

    fn box_from_wire_tag(&self) -> Result<Identifier> {
        self.box_from_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style tag boxer",
            })
    }

    fn native_to_wire_tag(&self) -> Result<Identifier> {
        self.native_to_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style native tag mapper",
            })
    }

    fn type_object(&self) -> Result<Identifier> {
        self.type_object.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "c-style enum has no Python type object",
        })
    }

    fn callable(&self, name: &CanonicalName) -> Result<PythonIdentifier> {
        PythonIdentifier::parse(format!(
            "_boltffi_{}_{}",
            self.stem,
            Name::new(name).function()?
        ))
    }
}

enum Shape {
    CStyle {
        variants: Vec<Variant>,
        primitive: primitive::Runtime,
    },
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonClass {
    class_name: PythonIdentifier,
    register_method: PythonIdentifier,
    variants: Vec<PythonVariant>,
}

impl PythonClass {
    pub fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let c_enum =
            bridge
                .source_c_style_enum(enumeration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "c-style enum package without C typedef",
                })?;
        let symbols = Symbols::from_c_style(enumeration, c_enum)?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            register_method: symbols.register_method().clone(),
            variants: enumeration
                .variants()
                .iter()
                .map(PythonVariant::from_variant)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn class_name(&self) -> &PythonIdentifier {
        &self.class_name
    }

    pub fn register_method(&self) -> &PythonIdentifier {
        &self.register_method
    }

    pub fn variants(&self) -> &[PythonVariant] {
        &self.variants
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonVariant {
    name: PythonIdentifier,
    value: i128,
}

impl PythonVariant {
    pub fn name(&self) -> &PythonIdentifier {
        &self.name
    }

    pub const fn value(&self) -> i128 {
        self.value
    }

    fn from_variant(variant: &CStyleVariantDecl) -> Result<Self> {
        Ok(Self {
            name: PythonIdentifier::parse(Name::new(variant.name()).enum_member())?,
            value: variant.discriminant().get(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Variant {
    member_name: PythonIdentifier,
    native_value: i128,
    wire_tag: usize,
    member_index: usize,
}

impl Variant {
    fn new(index: usize, variant: &CStyleVariantDecl) -> Result<Self> {
        Ok(Self {
            member_name: PythonIdentifier::parse(Name::new(variant.name()).enum_member())?,
            native_value: variant.discriminant().get(),
            wire_tag: index,
            member_index: index,
        })
    }
}
