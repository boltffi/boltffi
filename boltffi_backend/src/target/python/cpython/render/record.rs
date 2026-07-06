use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, DefaultValue, DirectFieldDecl, DirectRecordDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeSymbol, RecordDecl, RecordId,
};

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        python_cext::{ExtensionMethod, MethodFlags, MethodName, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::{
            codec::{self, EncodedCodec, EncodedCodecNode},
            render::{argument, direct_vector, function, primitive, result},
        },
        name_style::Name,
        syntax::Identifier as PythonIdentifier,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/record.c", escape = "none")]
struct DirectTemplate {
    class_name: PythonIdentifier,
    c_type: TypeFragment,
    type_object: Identifier,
    object_struct: Identifier,
    prefix: Identifier,
    type_setup: Identifier,
    parser: Identifier,
    boxer: Identifier,
    fields: Vec<Field>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/encoded_record.c", escape = "none")]
struct EncodedTemplate {
    class_name: PythonIdentifier,
    type_object: Identifier,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    wire_encoder: Identifier,
    owned_decoder: Identifier,
    codec: EncodedCodec,
}

pub struct Record {
    symbols: Symbols,
    shape: Shape,
    method: Option<ExtensionMethod>,
    callables: Vec<function::Function>,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge, context),
            RecordDecl::Encoded(record) => Self::from_encoded(record, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        let symbols = self.symbols;
        let source = match self.shape {
            Shape::Direct { fields, .. } => {
                let c_type = symbols.c_type()?.clone();
                let object_struct = symbols.object_struct()?;
                let prefix = symbols.prefix()?;
                let type_setup = symbols.type_setup()?;
                DirectTemplate {
                    class_name: symbols.class_name,
                    c_type,
                    type_object: symbols.type_object,
                    object_struct,
                    prefix,
                    type_setup,
                    parser: symbols.parser,
                    boxer: symbols.boxer,
                    fields,
                }
                .render()?
            }
            Shape::Encoded { codec, .. } => EncodedTemplate {
                class_name: symbols.class_name,
                type_object: symbols.type_object,
                register_method: symbols.register_method,
                register_wrapper: symbols.register_wrapper,
                wire_encoder: symbols.parser,
                owned_decoder: symbols.boxer,
                codec,
            }
            .render()?,
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
        self.method
            .iter()
            .chain(self.callables.iter().flat_map(function::Function::methods))
    }

    pub fn type_setup(&self) -> Result<Option<Identifier>> {
        match self.shape {
            Shape::Direct { .. } => self.symbols.type_setup().map(Some),
            Shape::Encoded { .. } => Ok(None),
        }
    }

    pub fn has_native_type(&self) -> bool {
        matches!(self.shape, Shape::Direct { .. })
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        let own = match &self.shape {
            Shape::Direct { primitives, .. } => primitives.clone(),
            Shape::Encoded { primitives, .. } => primitives.clone(),
        };
        own.into_iter()
            .chain(
                self.callables
                    .iter()
                    .flat_map(function::Function::primitives),
            )
            .collect()
    }

    pub fn cleanup(&self) -> c::Statement {
        c::Statement::new(format!("Py_CLEAR({})", self.symbols.type_object))
    }

    pub fn needs_owned_buffer(&self) -> bool {
        matches!(self.shape, Shape::Encoded { .. })
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

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record without C typedef",
                })?;
        if record.fields().len() != c_record.fields().len() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "record field mismatch",
            });
        }
        let symbols = Symbols::from_direct(record, c_record)?;
        let fields = record
            .fields()
            .iter()
            .zip(c_record.fields())
            .map(|(source, c_field)| Field::new(source, c_field))
            .collect::<Result<Vec<_>>>()?;
        let primitives = fields.iter().map(Field::primitive).collect();
        let callables = Self::direct_callables(record, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Direct { primitives, fields },
            method: None,
            callables,
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_encoded(record)?;
        let codec = EncodedCodec::from_record(record)?;
        let primitives = codec.primitives();
        let method = ExtensionMethod::new(
            MethodName::parse(symbols.register_method.as_str())?,
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::encoded_callables(record, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Encoded { codec, primitives },
            method: Some(method),
            callables,
        })
    }

    fn direct_callables(
        record: &DirectRecordDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Function>> {
        let initializers = record
            .initializers()
            .iter()
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = record.methods().iter().map(|method| {
            let receiver = method
                .callable()
                .receiver()
                .map(|receive| {
                    argument::Conversion::direct_record_receiver(
                        record.id(),
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

    fn encoded_callables(
        record: &EncodedRecordDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Function>> {
        let initializers = record
            .initializers()
            .iter()
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = record.methods().iter().map(|method| {
            let receiver = method
                .callable()
                .receiver()
                .map(|receive| {
                    argument::Conversion::encoded_record_receiver(
                        record.id(),
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
    type_object: Identifier,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    parser: Identifier,
    boxer: Identifier,
}

impl Symbols {
    pub fn from_record_id(
        record_id: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let record = context.record(record_id).ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "record id without declaration",
        })?;
        match record {
            RecordDecl::Direct(record) => {
                let c_record =
                    bridge
                        .source_direct_record(record_id)
                        .ok_or(Error::UnsupportedTarget {
                            target: "python",
                            shape: "direct record without C typedef",
                        })?;
                Self::from_direct(record, c_record)
            }
            RecordDecl::Encoded(record) => Self::from_encoded(record),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record declaration",
            }),
        }
    }

    pub fn c_type(&self) -> Result<&TypeFragment> {
        self.c_type.as_ref().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "encoded record has no C type",
        })
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn boxer(&self) -> &Identifier {
        &self.boxer
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

    pub fn from_direct(record: &DirectRecordDecl<Native>, c_record: &c::Record) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function_text()?)?.to_string();
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(record.name()).class())?,
            stem: stem.clone(),
            c_type: Some(TypeFragment::anonymous(&c::Type::named(c_record.name())?)?),
            type_object: Identifier::parse(format!("boltffi_python_{stem}_type"))?,
            register_method: PythonIdentifier::parse(format!("_register_{stem}"))?,
            register_wrapper: Identifier::parse(format!("boltffi_python_wrapper_register_{stem}"))?,
            parser: Identifier::parse(format!("boltffi_python_parse_{stem}"))?,
            boxer: Identifier::parse(format!("boltffi_python_box_{stem}"))?,
        })
    }

    pub fn object_struct(&self) -> Result<Identifier> {
        Identifier::parse(format!("boltffi_python_{}_object", self.stem))
    }

    pub fn prefix(&self) -> Result<Identifier> {
        Identifier::parse(format!("boltffi_python_{}", self.stem))
    }

    pub fn type_setup(&self) -> Result<Identifier> {
        Identifier::parse(format!("boltffi_python_setup_{}_type", self.stem))
    }

    pub fn from_encoded(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function_text()?)?.to_string();
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(record.name()).class())?,
            stem: stem.clone(),
            c_type: None,
            type_object: Identifier::parse(format!("boltffi_python_{stem}_type"))?,
            register_method: PythonIdentifier::parse(format!("_register_{stem}"))?,
            register_wrapper: Identifier::parse(format!("boltffi_python_wrapper_register_{stem}"))?,
            parser: Identifier::parse(format!("boltffi_python_wire_{stem}"))?,
            boxer: Identifier::parse(format!("boltffi_python_decode_owned_{stem}"))?,
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
    Direct {
        fields: Vec<Field>,
        primitives: Vec<primitive::Runtime>,
    },
    Encoded {
        codec: EncodedCodec,
        primitives: Vec<primitive::Runtime>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    python_name: PythonIdentifier,
    c_name: Identifier,
    value_name: Identifier,
    parser: Identifier,
    boxer: Identifier,
    default: Option<String>,
    primitive: primitive::Runtime,
}

impl Field {
    fn new(source: &DirectFieldDecl, c_field: &c::Field) -> Result<Self> {
        let primitive = primitive::Runtime::new(source.ty().primitive());
        let python_name = Self::python_name(source.key())?;
        Ok(Self {
            value_name: Identifier::escape(format!("{python_name}_value"))?,
            python_name,
            c_name: Identifier::parse(c_field.name())?,
            parser: primitive.parser()?,
            boxer: primitive.boxer()?,
            default: source
                .meta()
                .default()
                .map(Self::default_literal)
                .transpose()?,
            primitive,
        })
    }

    fn default_literal(value: &DefaultValue) -> Result<String> {
        match value {
            DefaultValue::Bool(value) => Ok(if *value { "true" } else { "false" }.to_owned()),
            DefaultValue::Integer(value) => Ok(value.get().to_string()),
            DefaultValue::Float(value) => {
                let float = value.to_f64();
                if float == f64::INFINITY {
                    Ok("INFINITY".to_owned())
                } else if float == f64::NEG_INFINITY {
                    Ok("-INFINITY".to_owned())
                } else if float.is_nan() {
                    Ok("NAN".to_owned())
                } else {
                    Ok(format!("{float:?}"))
                }
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "non-scalar direct record field default",
            }),
        }
    }

    fn primitive(&self) -> primitive::Runtime {
        self.primitive
    }

    fn python_name(key: &FieldKey) -> Result<PythonIdentifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => Name::position_field(*position),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record field key",
            }),
        }
    }
}
