use std::{collections::BTreeSet, path::PathBuf};

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    Bindings, CanonicalName, ClassDecl, ClassId, CodecNode, ConstantDecl, CustomTypeDecl,
    CustomTypeId, DeclarationRef, EncodedRecordDecl, EnumDecl, EnumId, ErrorChannel,
    ExportedCallable, ExportedMethodDecl, FunctionDecl, InitializerDecl, Native, NativeSymbol,
    RecordDecl, RecordId, StreamDecl, TypeRef,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, FilePath, GeneratedFile, GeneratedOutput, Result},
    target::python::{
        codec::{CodecAdapters, EnumCodec, FixedStruct, ReadFunction, WriteFunction},
        cpython::{
            codec::{EncodedCodec, NativeCodec},
            render::function,
        },
        name_style::{Name, PackageModule},
        syntax::{Expression, Identifier, Literal},
    },
};

mod callable;
mod class;
mod constant;
mod enumeration;
mod name_scope;
mod record;
mod stream;
mod type_hint;

use self::{
    callable::{AssociatedCallable, FunctionStub},
    class::Class,
    constant::ConstantStub,
    enumeration::EnumClass,
    name_scope::NameScope,
    record::{RecordClass, RecordWire},
    stream::ClassStream,
};

pub use self::callable::NativeFutureMethods;

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.py", escape = "none")]
struct InitTemplate {
    module_name_literal: Literal,
    package_name_literal: Literal,
    package_version: Expression,
    windows_library: Literal,
    macos_library: Literal,
    unix_library: Literal,
    uses_sequence_annotations: bool,
    uses_callable_annotations: bool,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
    has_data_enums: bool,
    codec_decoders: Vec<ReadFunction>,
    codec_encoders: Vec<WriteFunction>,
    records: Vec<RecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    constants: Vec<ConstantStub>,
    functions: Vec<FunctionStub>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.pyi", escape = "none")]
struct StubTemplate {
    uses_sequence_annotations: bool,
    uses_callable_annotations: bool,
    has_data_enums: bool,
    records: Vec<RecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    constants: Vec<ConstantStub>,
    functions: Vec<FunctionStub>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/pyproject.toml", escape = "none")]
struct PyprojectTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/python/setup.py", escape = "none")]
struct SetupTemplate {
    module_name_literal: Literal,
    package_name_literal: Literal,
    package_version_literal: Literal,
    extension_name_literal: Literal,
    extension_source_literal: Literal,
}

pub struct Package<'bindings> {
    bindings: &'bindings Bindings<Native>,
    declarations: PackageDeclarations<'bindings>,
    bridge: &'bindings PythonCExtBridgeContract,
    module: PackageModule,
    distribution: String,
    version: Option<String>,
    library: String,
}

impl<'bindings> Package<'bindings> {
    pub fn new(
        bindings: &'bindings Bindings<Native>,
        bridge: &'bindings PythonCExtBridgeContract,
        module: PackageModule,
        distribution: String,
        version: Option<String>,
        library: String,
    ) -> Self {
        Self {
            bindings,
            declarations: PackageDeclarations::new(bindings),
            bridge,
            module,
            distribution,
            version,
            library,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let module = self.module_name();
        let package = self.distribution.clone();
        let version = self.version.clone();
        let error_types = ErrorTypes::from_declarations(&self.declarations);
        let records = self.records(&error_types)?;
        let enums = self.enums(&error_types)?;
        let classes = self.classes()?;
        let constants = self.constants()?;
        let functions = self.functions();
        let codec_adapters = CodecAdapters::from_bindings(self.bindings);
        let codec_decoders = codec_adapters
            .decoders()
            .iter()
            .map(|adapter| ReadFunction::from_adapter(adapter, &self))
            .collect::<Result<Vec<_>>>()?;
        let codec_encoders = codec_adapters
            .encoders()
            .iter()
            .map(|adapter| WriteFunction::from_adapter(adapter, &self))
            .collect::<Result<Vec<_>>>()?;
        let stubs = functions
            .iter()
            .map(|function| FunctionStub::from_declaration(function, &self))
            .collect::<Result<Vec<_>>>()?;
        self.validate_names(&records, &enums, &classes, &constants, &stubs)?;
        let uses_sequence_annotations = records.iter().any(RecordClass::uses_sequence_annotations)
            || enums.iter().any(EnumClass::uses_sequence_annotations)
            || classes.iter().any(Class::uses_sequence_annotations)
            || stubs.iter().any(FunctionStub::uses_sequence_annotations);
        let uses_callable_annotations = records.iter().any(RecordClass::uses_callable_annotations)
            || enums.iter().any(EnumClass::uses_callable_annotations)
            || classes.iter().any(Class::uses_callable_annotations)
            || stubs.iter().any(FunctionStub::uses_callable_annotations);
        let uses_wire_helpers = !records.is_empty()
            || enums.iter().any(EnumClass::has_wire)
            || enums.iter().any(EnumClass::uses_wire_helpers)
            || classes.iter().any(Class::uses_wire_helpers)
            || constants.iter().any(ConstantStub::uses_wire_helpers)
            || stubs.iter().any(FunctionStub::uses_wire_helpers)
            || !codec_adapters.is_empty();
        let uses_async_helpers = records.iter().any(RecordClass::uses_async_helpers)
            || enums.iter().any(EnumClass::uses_async_helpers)
            || classes.iter().any(Class::uses_async_helpers)
            || stubs.iter().any(FunctionStub::uses_async_helpers);
        Ok(GeneratedOutput::new(
            vec![
                self.file("pyproject.toml", Self::text(PyprojectTemplate.render()?))?,
                self.file(
                    "setup.py",
                    Self::text(
                        SetupTemplate {
                            module_name_literal: Self::literal(&module),
                            package_name_literal: Self::literal(&package),
                            package_version_literal: Self::literal(
                                version.as_deref().unwrap_or("0.0.0"),
                            ),
                            extension_name_literal: Self::literal(format!(
                                "{}.{}",
                                module,
                                self.bridge.module().as_str()
                            )),
                            extension_source_literal: Self::literal(
                                self.bridge.source_path().as_path().display().to_string(),
                            ),
                        }
                        .render()?,
                    ),
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.py"),
                    Self::text(
                        InitTemplate {
                            module_name_literal: Self::literal(&module),
                            package_name_literal: Self::literal(&package),
                            package_version: version
                                .as_deref()
                                .map(Self::literal)
                                .map(Expression::literal)
                                .unwrap_or_else(|| Expression::literal(Literal::none())),
                            windows_library: Self::literal(format!("{}.dll", self.library)),
                            macos_library: Self::literal(format!("lib{}.dylib", self.library)),
                            unix_library: Self::literal(format!("lib{}.so", self.library)),
                            uses_sequence_annotations,
                            uses_callable_annotations,
                            uses_wire_helpers,
                            uses_async_helpers,
                            has_data_enums: enums.iter().any(EnumClass::has_wire),
                            codec_decoders,
                            codec_encoders,
                            records: records.clone(),
                            enums: enums.clone(),
                            classes: classes.clone(),
                            constants: constants.clone(),
                            functions: stubs.clone(),
                        }
                        .render()?,
                    ),
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.pyi"),
                    Self::text(
                        StubTemplate {
                            uses_sequence_annotations,
                            uses_callable_annotations,
                            has_data_enums: enums.iter().any(EnumClass::has_wire),
                            records,
                            enums,
                            classes,
                            constants,
                            functions: stubs,
                        }
                        .render()?,
                    ),
                )?,
                self.file(PathBuf::from(&module).join("py.typed"), String::new())?,
            ],
            Vec::new(),
        ))
    }
}

#[derive(Default)]
struct PackageDeclarations<'bindings> {
    records: Vec<&'bindings RecordDecl<Native>>,
    enums: Vec<&'bindings EnumDecl<Native>>,
    classes: Vec<&'bindings ClassDecl<Native>>,
    constants: Vec<&'bindings ConstantDecl<Native>>,
    functions: Vec<&'bindings FunctionDecl<Native>>,
    streams: Vec<&'bindings StreamDecl<Native>>,
    customs: Vec<&'bindings CustomTypeDecl>,
}

#[derive(Default)]
struct ErrorTypes {
    records: BTreeSet<RecordId>,
    enums: BTreeSet<EnumId>,
}

impl ErrorTypes {
    fn from_declarations(declarations: &PackageDeclarations<'_>) -> Self {
        let mut types = Self::default();
        declarations
            .functions
            .iter()
            .for_each(|function| types.insert_callable(function.callable()));
        declarations
            .records
            .iter()
            .for_each(|record| types.insert_record(record));
        declarations
            .enums
            .iter()
            .for_each(|enumeration| types.insert_enum(enumeration));
        declarations
            .classes
            .iter()
            .for_each(|class| types.insert_class(class));
        types
    }

    fn contains_record(&self, id: RecordId) -> bool {
        self.records.contains(&id)
    }

    fn contains_enum(&self, id: EnumId) -> bool {
        self.enums.contains(&id)
    }

    fn insert_callable(&mut self, callable: &ExportedCallable<Native>) {
        if let ErrorChannel::Encoded { ty, .. } = callable.error().channel() {
            self.insert_type(ty);
        }
    }

    fn insert_record(&mut self, record: &RecordDecl<Native>) {
        match record {
            RecordDecl::Direct(record) => {
                self.insert_members(record.initializers(), record.methods())
            }
            RecordDecl::Encoded(record) => {
                self.insert_members(record.initializers(), record.methods())
            }
            _ => {}
        }
    }

    fn insert_enum(&mut self, enumeration: &EnumDecl<Native>) {
        match enumeration {
            EnumDecl::CStyle(enumeration) => {
                self.insert_members(enumeration.initializers(), enumeration.methods())
            }
            EnumDecl::Data(enumeration) => {
                self.insert_members(enumeration.initializers(), enumeration.methods())
            }
            _ => {}
        }
    }

    fn insert_class(&mut self, class: &ClassDecl<Native>) {
        self.insert_members(class.initializers(), class.methods());
    }

    fn insert_members(
        &mut self,
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
    ) {
        initializers
            .iter()
            .for_each(|initializer| self.insert_callable(initializer.callable()));
        methods
            .iter()
            .for_each(|method| self.insert_callable(method.callable()));
    }

    fn insert_type(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Record(id) => {
                self.records.insert(*id);
            }
            TypeRef::Enum(id) => {
                self.enums.insert(*id);
            }
            _ => {}
        }
    }
}

impl<'bindings> PackageDeclarations<'bindings> {
    fn new(bindings: &'bindings Bindings<Native>) -> Self {
        bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .fold(Self::default(), Self::insert)
    }

    fn insert(mut self, declaration: DeclarationRef<'bindings, Native>) -> Self {
        match declaration {
            DeclarationRef::Record(record) => self.records.push(record),
            DeclarationRef::Enum(enumeration) => self.enums.push(enumeration),
            DeclarationRef::Class(class) => self.classes.push(class),
            DeclarationRef::Constant(constant) => self.constants.push(constant),
            DeclarationRef::Function(function) => self.functions.push(function),
            DeclarationRef::Stream(stream) => self.streams.push(stream),
            DeclarationRef::CustomType(custom_type) => self.customs.push(custom_type),
            DeclarationRef::Callback(_) => {}
        }
        self
    }
}

impl<'bindings> Package<'bindings> {
    pub fn record_name(&self, record_id: RecordId) -> Result<Identifier> {
        self.declarations
            .records
            .iter()
            .find_map(|record| match record {
                RecordDecl::Direct(record) if record.id() == record_id => Some(record.name()),
                RecordDecl::Encoded(record) if record.id() == record_id => Some(record.name()),
                RecordDecl::Direct(_) | RecordDecl::Encoded(_) => None,
                _ => None,
            })
            .map(|name| Identifier::parse(Name::new(name).class()))
            .transpose()?
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "record type hint without declaration",
            })
    }

    pub fn direct_record_struct(&self, record_id: RecordId) -> Result<FixedStruct> {
        self.declarations
            .records
            .iter()
            .find_map(|record| match record {
                RecordDecl::Direct(record) if record.id() == record_id => {
                    Some(self.record_name(record_id).and_then(|name| {
                        FixedStruct::from_layout(&name, record.fields(), record.layout())
                    }))
                }
                _ => None,
            })
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded record reached direct record spelling",
            })?
    }

    pub fn has_native_encoded_crossing(&self, node: &CodecNode) -> Result<bool> {
        NativeCodec::supports_node(node, |record_id| {
            self.encoded_record(record_id)
                .and_then(EncodedCodec::from_record)
                .map(|codec| codec.is_native())
        })
    }

    pub fn enum_name(&self, enum_id: EnumId) -> Result<Identifier> {
        self.declarations
            .enums
            .iter()
            .find_map(|enumeration| match enumeration {
                EnumDecl::CStyle(enumeration) if enumeration.id() == enum_id => {
                    Some(enumeration.name())
                }
                EnumDecl::Data(enumeration) if enumeration.id() == enum_id => {
                    Some(enumeration.name())
                }
                EnumDecl::CStyle(_) | EnumDecl::Data(_) => None,
                _ => None,
            })
            .map(|name| Identifier::parse(Name::new(name).class()))
            .transpose()?
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum type hint without declaration",
            })
    }

    pub fn enum_codec(&self, enum_id: EnumId) -> Result<EnumCodec> {
        self.declarations
            .enums
            .iter()
            .find_map(|enumeration| match enumeration {
                EnumDecl::CStyle(enumeration) if enumeration.id() == enum_id => {
                    Some(Ok(EnumCodec::CStyle(enumeration.repr().primitive())))
                }
                EnumDecl::Data(enumeration) if enumeration.id() == enum_id => Some(
                    Identifier::parse(Name::new(enumeration.name()).class())
                        .map(|class_name| EnumCodec::Data { class_name }),
                ),
                EnumDecl::CStyle(_) | EnumDecl::Data(_) => None,
                _ => None,
            })
            .transpose()?
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum wire type without declaration",
            })
    }

    pub fn class_name(&self, class_id: &ClassId) -> Result<Identifier> {
        self.declarations
            .classes
            .iter()
            .find(|class| class.id() == *class_id)
            .map(|class| Identifier::parse(Name::new(class.name()).class()))
            .transpose()?
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "class type hint without declaration",
            })
    }

    pub fn custom_representation(&self, custom_type: CustomTypeId) -> Result<&'bindings TypeRef> {
        self.custom_type(custom_type)
            .map(CustomTypeDecl::representation)
    }

    pub fn custom_type(&self, custom_type: CustomTypeId) -> Result<&'bindings CustomTypeDecl> {
        self.declarations
            .customs
            .iter()
            .copied()
            .find(|declaration| declaration.id() == custom_type)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "custom type without declaration",
            })
    }

    pub fn literal(value: impl AsRef<str>) -> Literal {
        Literal::string(value.as_ref())
    }

    fn encoded_record(&self, record_id: RecordId) -> Result<&'bindings EncodedRecordDecl<Native>> {
        self.declarations
            .records
            .iter()
            .find_map(|record| match record {
                RecordDecl::Encoded(record) if record.id() == record_id => Some(record),
                RecordDecl::Direct(_) | RecordDecl::Encoded(_) => None,
                _ => None,
            })
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "record type hint without declaration",
            })
    }
}

impl<'bindings> Package<'bindings> {
    fn module_name(&self) -> String {
        self.module.as_str().to_owned()
    }

    fn functions(&self) -> Vec<&'bindings FunctionDecl<Native>> {
        self.declarations
            .functions
            .iter()
            .copied()
            .filter(|function| function::Function::can_render(function.callable()))
            .collect()
    }

    fn constants(&self) -> Result<Vec<ConstantStub>> {
        self.declarations
            .constants
            .iter()
            .copied()
            .map(|constant| ConstantStub::from_declaration(constant, self))
            .collect()
    }

    fn records(&self, error_types: &ErrorTypes) -> Result<Vec<RecordClass>> {
        self.declarations
            .records
            .iter()
            .copied()
            .map(|record| match record {
                RecordDecl::Direct(record) => {
                    RecordClass::from_direct(record, self, error_types.contains_record(record.id()))
                }
                RecordDecl::Encoded(record) => RecordClass::from_encoded(
                    record,
                    self,
                    error_types.contains_record(record.id()),
                ),
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record package",
                }),
            })
            .collect()
    }

    fn enums(&self, error_types: &ErrorTypes) -> Result<Vec<EnumClass>> {
        self.declarations
            .enums
            .iter()
            .copied()
            .map(|enumeration| match enumeration {
                EnumDecl::CStyle(enumeration) => EnumClass::from_c_style(
                    enumeration,
                    self,
                    error_types.contains_enum(enumeration.id()),
                ),
                EnumDecl::Data(enumeration) => EnumClass::from_data(
                    enumeration,
                    self,
                    error_types.contains_enum(enumeration.id()),
                ),
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown enum package",
                }),
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<Class>> {
        self.declarations
            .classes
            .iter()
            .copied()
            .map(|class| Class::from_declaration(class, self))
            .collect()
    }

    fn streams_for_class(&self, class: ClassId) -> Vec<&'bindings StreamDecl<Native>> {
        self.declarations
            .streams
            .iter()
            .copied()
            .filter(|stream| stream.owner() == Some(class))
            .collect()
    }

    fn enum_variant_expression(
        &self,
        enum_name: &CanonicalName,
        variant_name: &CanonicalName,
    ) -> Result<Expression> {
        self.declarations
            .enums
            .iter()
            .find_map(|enumeration| match enumeration {
                EnumDecl::CStyle(enumeration) if enumeration.name() == enum_name => {
                    Some(enumeration::VariantStyle::CStyle)
                }
                EnumDecl::Data(enumeration) if enumeration.name() == enum_name => {
                    Some(enumeration::VariantStyle::Data)
                }
                EnumDecl::CStyle(_) | EnumDecl::Data(_) => None,
                _ => None,
            })
            .map(|style| style.expression(enum_name, variant_name))
            .transpose()?
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum constant without declaration",
            })
    }

    fn exception_name(&self, class_name: &Identifier) -> Result<Identifier> {
        Identifier::parse(format!("{class_name}Exception"))
    }

    fn file(&self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<GeneratedFile> {
        Ok(GeneratedFile::new(FilePath::new(path.into())?, contents))
    }

    fn text(contents: String) -> String {
        match contents.ends_with('\n') {
            true => contents,
            false => format!("{contents}\n"),
        }
    }

    fn validate_names(
        &self,
        records: &[RecordClass],
        enums: &[EnumClass],
        classes: &[Class],
        constants: &[ConstantStub],
        functions: &[FunctionStub],
    ) -> Result<()> {
        let scope = match enums.iter().any(EnumClass::is_int_enum) {
            true => {
                NameScope::new("python module").insert("IntEnum", "imported enum base `IntEnum`")?
            }
            false => NameScope::new("python module"),
        };
        scope
            .insert_all(records.iter().map(|record| record.top_level_name()))
            .and_then(|scope| {
                scope.insert_all(
                    records
                        .iter()
                        .filter_map(RecordClass::exception_top_level_name),
                )
            })
            .and_then(|scope| scope.insert_all(enums.iter().map(EnumClass::top_level_name)))
            .and_then(|scope| {
                scope.insert_all(enums.iter().filter_map(EnumClass::exception_top_level_name))
            })
            .and_then(|scope| {
                scope.insert_all(enums.iter().flat_map(EnumClass::data_variant_names))
            })
            .and_then(|scope| scope.insert_all(classes.iter().map(Class::top_level_name)))
            .and_then(|scope| scope.insert_all(classes.iter().flat_map(Class::subscription_names)))
            .and_then(|scope| scope.insert_all(constants.iter().map(ConstantStub::top_level_name)))
            .and_then(|scope| scope.insert_all(functions.iter().map(FunctionStub::top_level_name)))
            .map(|_| ())?;

        records.iter().try_for_each(RecordClass::validate_names)?;
        enums.iter().try_for_each(EnumClass::validate_names)?;
        classes.iter().try_for_each(Class::validate_names)?;
        functions.iter().try_for_each(FunctionStub::validate_names)
    }
}
