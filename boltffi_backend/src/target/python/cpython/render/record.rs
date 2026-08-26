use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, DefaultValue, DirectFieldDecl, DirectRecordDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeSymbol, Primitive, RecordDecl,
    RecordId, TypeRef,
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
    module_name: String,
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

#[derive(AskamaTemplate)]
#[template(path = "target/python/native_opaque_record.c", escape = "none")]
struct NativeOpaqueTemplate {
    class_name: PythonIdentifier,
    type_object: Identifier,
    register_method: PythonIdentifier,
    register_wrapper: Identifier,
    boxer: Identifier,
    drop_wrapper: Identifier,
    drop_storage: Identifier,
    has_wrappers: Vec<NativeOpaqueHasView>,
    get_wrappers: Vec<NativeOpaqueGetView>,
    borrow_wrappers: Vec<NativeOpaqueBorrowWrapperView>,
}

/// C wrapper view for an optional-presence helper.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeOpaqueHasView {
    pub c_wrapper: Identifier,
    pub has_storage: Identifier,
}

/// C wrapper view for a primitive-get helper.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeOpaqueGetView {
    pub c_wrapper: Identifier,
    pub get_storage: Identifier,
    pub c_return_type: TypeFragment,
    pub boxer: Identifier,
}

/// C wrapper view for a String/Bytes borrow helper.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeOpaqueBorrowWrapperView {
    pub c_wrapper: Identifier,
    pub borrow_storage: Identifier,
}

pub struct Record {
    symbols: Symbols,
    shape: Shape,
    method: Option<ExtensionMethod>,
    callables: Vec<function::Function>,
    /// Extra module methods generated for native opaque record field accessors.
    native_opaque_methods: Vec<ExtensionMethod>,
    /// Primitive runtimes needed by native opaque get-wrapper boxers.  These
    /// must be included in primitives() so the boxer definitions are emitted
    /// in native_module.c even when no other field uses them.
    native_opaque_get_primitives: Vec<primitive::Runtime>,
    /// All native-opaque helper view data for the C template.
    native_opaque_drop_wrapper: Option<Identifier>,
    native_opaque_drop_storage: Option<Identifier>,
    native_opaque_has_wrappers: Vec<NativeOpaqueHasView>,
    native_opaque_get_wrappers: Vec<NativeOpaqueGetView>,
    native_opaque_borrow_wrappers: Vec<NativeOpaqueBorrowWrapperView>,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge, context),
            RecordDecl::Encoded(record) if record.is_native_opaque() => {
                Self::from_native_opaque(record, bridge, context)
            }
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
            Shape::Direct {
                fields,
                module_name,
                ..
            } => {
                let c_type = symbols.c_type()?.clone();
                let object_struct = symbols.object_struct()?;
                let prefix = symbols.prefix()?;
                let type_setup = symbols.type_setup()?;
                DirectTemplate {
                    module_name,
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
            Shape::NativeOpaque => NativeOpaqueTemplate {
                class_name: symbols.class_name,
                type_object: symbols.type_object,
                register_method: symbols.register_method,
                register_wrapper: symbols.register_wrapper,
                boxer: symbols.boxer,
                drop_wrapper: self
                    .native_opaque_drop_wrapper
                    .clone()
                    .expect("native opaque drop wrapper present"),
                drop_storage: self
                    .native_opaque_drop_storage
                    .clone()
                    .expect("native opaque drop storage present"),
                has_wrappers: self.native_opaque_has_wrappers.clone(),
                get_wrappers: self.native_opaque_get_wrappers.clone(),
                borrow_wrappers: self.native_opaque_borrow_wrappers.clone(),
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
            .chain(self.native_opaque_methods.iter())
            .chain(self.callables.iter().flat_map(function::Function::methods))
    }

    pub fn type_setup(&self) -> Result<Option<Identifier>> {
        match self.shape {
            Shape::Direct { .. } => self.symbols.type_setup().map(Some),
            Shape::Encoded { .. } | Shape::NativeOpaque => Ok(None),
        }
    }

    pub fn has_native_type(&self) -> bool {
        matches!(self.shape, Shape::Direct { .. })
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        let own = match &self.shape {
            Shape::Direct { primitives, .. } => primitives.clone(),
            Shape::Encoded { primitives, .. } => primitives.clone(),
            // Include primitives from get wrappers so their boxer definitions
            // are emitted in native_module.c even when no other field uses them.
            Shape::NativeOpaque => self.native_opaque_get_primitives.clone(),
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
            shape: Shape::Direct {
                primitives,
                fields,
                module_name: bridge.module().as_str().to_owned(),
            },
            method: None,
            callables,
            native_opaque_methods: Vec::new(),
            native_opaque_get_primitives: Vec::new(),
            native_opaque_drop_wrapper: None,
            native_opaque_drop_storage: None,
            native_opaque_has_wrappers: Vec::new(),
            native_opaque_get_wrappers: Vec::new(),
            native_opaque_borrow_wrappers: Vec::new(),
        })
    }

    fn from_native_opaque(
        record: &EncodedRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        _context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_native_opaque(record)?;
        let method = ExtensionMethod::new(
            MethodName::parse(symbols.register_method.as_str())?,
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let stem = symbols.stem().to_owned();
        let exports = record
            .native_opaque_exports()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "native opaque record without helper exports",
            })?;

        // Drop wrapper — uses loaded storage pointer
        let drop_sym = exports.drop();
        let drop_loaded = bridge
            .loaded_function(drop_sym)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "native opaque drop helper not found in bridge",
            })?;
        let drop_wrapper_name = Identifier::parse(format!("boltffi_python_opaque_drop_{stem}"))?;
        let drop_storage = drop_loaded.storage_name().clone();
        let mut native_opaque_methods: Vec<ExtensionMethod> = vec![ExtensionMethod::new(
            MethodName::parse(drop_wrapper_name.as_str())?,
            drop_wrapper_name.clone(),
            MethodFlags::FastCall,
        )?];

        let mut has_wrappers: Vec<NativeOpaqueHasView> = Vec::new();
        let mut get_wrappers: Vec<NativeOpaqueGetView> = Vec::new();
        let mut borrow_wrappers: Vec<NativeOpaqueBorrowWrapperView> = Vec::new();
        let mut get_primitives: Vec<primitive::Runtime> = Vec::new();

        for (field, fexp) in record.fields().iter().zip(exports.fields()) {
            let inner_ty = match field.ty() {
                TypeRef::Optional(inner) => inner.as_ref(),
                ty => ty,
            };
            let is_optional = matches!(field.ty(), TypeRef::Optional(_));
            // Use sanitized function name for multi-word fields (Issues 5)
            let field_name = match field.key() {
                boltffi_binding::FieldKey::Named(n) => Name::new(n).function_text()?,
                _ => continue,
            };

            // Has wrapper (optional fields only)
            if is_optional {
                let has_sym = fexp.has().ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "optional native opaque field missing has helper",
                })?;
                let has_loaded =
                    bridge
                        .loaded_function(has_sym)
                        .ok_or(Error::UnsupportedTarget {
                            target: "python",
                            shape: "native opaque has helper not found in bridge",
                        })?;
                let has_wrapper_name =
                    Identifier::parse(format!("boltffi_python_opaque_has_{stem}_{field_name}"))?;
                has_wrappers.push(NativeOpaqueHasView {
                    c_wrapper: has_wrapper_name.clone(),
                    has_storage: has_loaded.storage_name().clone(),
                });
                native_opaque_methods.push(ExtensionMethod::new(
                    MethodName::parse(has_wrapper_name.as_str())?,
                    has_wrapper_name,
                    MethodFlags::FastCall,
                )?);
            }

            match inner_ty {
                TypeRef::Primitive(p) => {
                    let get_sym = fexp.get().ok_or(Error::UnsupportedTarget {
                        target: "python",
                        shape: "primitive native opaque field missing get helper",
                    })?;
                    let get_loaded =
                        bridge
                            .loaded_function(get_sym)
                            .ok_or(Error::UnsupportedTarget {
                                target: "python",
                                shape: "native opaque get helper not found in bridge",
                            })?;
                    let get_wrapper_name = Identifier::parse(format!(
                        "boltffi_python_opaque_get_{stem}_{field_name}"
                    ))?;
                    let prim = primitive::Runtime::new(*p);
                    get_wrappers.push(NativeOpaqueGetView {
                        c_wrapper: get_wrapper_name.clone(),
                        get_storage: get_loaded.storage_name().clone(),
                        c_return_type: prim.c_type()?,
                        boxer: prim.boxer()?,
                    });
                    // Collect the primitive so its boxer definition is emitted
                    // in native_module.c (via primitives()).
                    get_primitives.push(prim);
                    native_opaque_methods.push(ExtensionMethod::new(
                        MethodName::parse(get_wrapper_name.as_str())?,
                        get_wrapper_name,
                        MethodFlags::FastCall,
                    )?);
                }
                TypeRef::String | TypeRef::Bytes => {
                    let borrow_sym = fexp.borrow().ok_or(Error::UnsupportedTarget {
                        target: "python",
                        shape: "String/Bytes native opaque field missing borrow helper",
                    })?;
                    let borrow_loaded =
                        bridge
                            .loaded_function(borrow_sym)
                            .ok_or(Error::UnsupportedTarget {
                                target: "python",
                                shape: "native opaque borrow helper not found in bridge",
                            })?;
                    let borrow_wrapper_name = Identifier::parse(format!(
                        "boltffi_python_borrow_opaque_{stem}_{field_name}"
                    ))?;
                    borrow_wrappers.push(NativeOpaqueBorrowWrapperView {
                        c_wrapper: borrow_wrapper_name.clone(),
                        borrow_storage: borrow_loaded.storage_name().clone(),
                    });
                    native_opaque_methods.push(ExtensionMethod::new(
                        MethodName::parse(borrow_wrapper_name.as_str())?,
                        borrow_wrapper_name,
                        MethodFlags::FastCall,
                    )?);
                }
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "unsupported native opaque field type",
                    });
                }
            }
        }

        Ok(Self {
            symbols,
            shape: Shape::NativeOpaque,
            method: Some(method),
            callables: Vec::new(),
            native_opaque_methods,
            native_opaque_get_primitives: get_primitives,
            native_opaque_drop_wrapper: Some(drop_wrapper_name),
            native_opaque_drop_storage: Some(drop_storage),
            native_opaque_has_wrappers: has_wrappers,
            native_opaque_get_wrappers: get_wrappers,
            native_opaque_borrow_wrappers: borrow_wrappers,
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
            native_opaque_methods: Vec::new(),
            native_opaque_get_primitives: Vec::new(),
            native_opaque_drop_wrapper: None,
            native_opaque_drop_storage: None,
            native_opaque_has_wrappers: Vec::new(),
            native_opaque_get_wrappers: Vec::new(),
            native_opaque_borrow_wrappers: Vec::new(),
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

    pub fn from_native_opaque(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function_text()?)?.to_string();
        Ok(Self {
            class_name: PythonIdentifier::parse(Name::new(record.name()).class())?,
            stem: stem.clone(),
            c_type: None,
            type_object: Identifier::parse(format!("boltffi_python_{stem}_type"))?,
            register_method: PythonIdentifier::parse(format!("_register_{stem}"))?,
            register_wrapper: Identifier::parse(format!("boltffi_python_wrapper_register_{stem}"))?,
            // parser is unused for native opaque (no wire encoding); keep consistent naming
            parser: Identifier::parse(format!(
                "boltffi_python_native_opaque_{stem}_parser_unused"
            ))?,
            boxer: Identifier::parse(format!("boltffi_python_box_opaque_{stem}"))?,
        })
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
        module_name: String,
    },
    Encoded {
        codec: EncodedCodec,
        primitives: Vec<primitive::Runtime>,
    },
    NativeOpaque,
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
        let binding_primitive = source.ty().primitive();
        let primitive = primitive::Runtime::new(binding_primitive);
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
                .map(|value| Self::default_literal(value, binding_primitive))
                .transpose()?,
            primitive,
        })
    }

    fn default_literal(value: &DefaultValue, primitive: Primitive) -> Result<String> {
        match (primitive, value) {
            (Primitive::Bool, DefaultValue::Bool(value)) => {
                Ok(if *value { "true" } else { "false" }.to_owned())
            }
            (Primitive::I8, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<i8>(value.get())
            }
            (Primitive::U8, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<u8>(value.get())
            }
            (Primitive::I16, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<i16>(value.get())
            }
            (Primitive::U16, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<u16>(value.get())
            }
            (Primitive::I32, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<i32>(value.get())
            }
            (Primitive::U32, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<u32>(value.get())
            }
            (Primitive::I64 | Primitive::ISize, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<i64>(value.get())
            }
            (Primitive::U64 | Primitive::USize, DefaultValue::Integer(value)) => {
                Self::bounded_integer_literal::<u64>(value.get())
            }
            (Primitive::F32, DefaultValue::Float(value)) => {
                let value = value.to_f64();
                if value.is_finite() && value.abs() > f32::MAX.into() {
                    return Err(Self::invalid_default("direct record field default range"));
                }
                Ok(Self::float_literal(value))
            }
            (Primitive::F64, DefaultValue::Float(value)) => Ok(Self::float_literal(value.to_f64())),
            _ => Err(Self::invalid_default("direct record field default type")),
        }
    }

    fn bounded_integer_literal<Integer>(value: i128) -> Result<String>
    where
        Integer: TryFrom<i128>,
    {
        Integer::try_from(value)
            .map(|_| value.to_string())
            .map_err(|_| Self::invalid_default("direct record field default range"))
    }

    fn float_literal(value: f64) -> String {
        if value == f64::INFINITY {
            "INFINITY".to_owned()
        } else if value == f64::NEG_INFINITY {
            "-INFINITY".to_owned()
        } else if value.is_nan() {
            "NAN".to_owned()
        } else {
            format!("{value:?}")
        }
    }

    fn invalid_default(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "python",
            shape,
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
