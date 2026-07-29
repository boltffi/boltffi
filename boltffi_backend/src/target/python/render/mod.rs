use std::path::PathBuf;

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CanonicalName, ClassDecl, ClassId, CodecNode, ConstantDecl, ConstantOwner,
    CustomTypeDecl, CustomTypeId, DeclarationRef, EncodedRecordDecl, EnumDecl, EnumId,
    FunctionDecl, Native, RecordDecl, RecordId, StreamDecl, TypeRef,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{
        Error, FilePath, GeneratedFile, GeneratedOutput, RenderContext, RenderedDeclaration, Result,
    },
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
    associated_constants: Vec<ConstantStub>,
    functions: Vec<FunctionStub>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.pyi", escape = "none")]
struct StubTemplate {
    uses_sequence_annotations: bool,
    uses_callable_annotations: bool,
    has_associated_constants: bool,
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
    declarations: PackageDeclarations<'bindings>,
    codec_adapters: CodecAdapters<'bindings>,
    bridge: &'bindings PythonCExtBridgeContract,
    context: &'bindings RenderContext<'bindings, Native>,
    module: PackageModule,
    distribution: String,
    version: Option<String>,
    library: String,
}

impl<'bindings> Package<'bindings> {
    pub fn new(
        bridge: &'bindings PythonCExtBridgeContract,
        context: &'bindings RenderContext<'bindings, Native>,
        module: PackageModule,
        distribution: String,
        version: Option<String>,
        library: String,
        rendered: &[RenderedDeclaration<'bindings, Native>],
    ) -> Self {
        let declarations = PackageDeclarations::from_rendered(rendered);
        let declaration_refs = rendered
            .iter()
            .map(RenderedDeclaration::declaration)
            .collect::<Vec<_>>();
        Self {
            declarations,
            codec_adapters: CodecAdapters::from_declarations(context.bindings(), &declaration_refs),
            bridge,
            context,
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
        let records = self.records()?;
        let enums = self.enums()?;
        let classes = self.classes()?;
        let constants = self.constants()?;
        let associated_constants = records
            .iter()
            .flat_map(|record| &record.constants)
            .chain(enums.iter().flat_map(|enumeration| &enumeration.constants))
            .chain(classes.iter().flat_map(|class| &class.constants))
            .cloned()
            .collect::<Vec<_>>();
        let has_associated_constants = !associated_constants.is_empty();
        let functions = self.functions();
        let codec_decoders = self
            .codec_adapters
            .decoders()
            .iter()
            .map(|adapter| ReadFunction::from_adapter(adapter, &self))
            .collect::<Result<Vec<_>>>()?;
        let codec_encoders = self
            .codec_adapters
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
            || associated_constants
                .iter()
                .any(ConstantStub::uses_wire_helpers)
            || stubs.iter().any(FunctionStub::uses_wire_helpers)
            || !self.codec_adapters.is_empty();
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
                            associated_constants,
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
                            has_associated_constants,
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

impl<'bindings> PackageDeclarations<'bindings> {
    fn from_rendered(rendered: &[RenderedDeclaration<'bindings, Native>]) -> Self {
        rendered
            .iter()
            .map(RenderedDeclaration::declaration)
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
            .find(|record| record.id() == record_id)
            .map(|record| record.name())
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
            .find(|enumeration| enumeration.id() == enum_id)
            .map(|enumeration| enumeration.name())
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
            .find(|enumeration| enumeration.id() == enum_id)
            .map(|enumeration| match *enumeration {
                EnumDecl::CStyle(enumeration) => {
                    Ok(EnumCodec::CStyle(enumeration.repr().primitive()))
                }
                EnumDecl::Data(enumeration) => {
                    Identifier::parse(Name::new(enumeration.name()).class())
                        .map(|class_name| EnumCodec::Data { class_name })
                }
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown enum wire type",
                }),
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
                RecordDecl::Encoded(record) if record.id() == record_id => Some(record.as_ref()),
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
            .filter(|constant| constant.owner().is_none())
            .map(|constant| ConstantStub::from_declaration(constant, self))
            .collect()
    }

    fn constants_for_owner(&self, owner: ConstantOwner) -> Result<Vec<ConstantStub>> {
        self.declarations
            .constants
            .iter()
            .copied()
            .filter(|constant| constant.owner() == Some(owner))
            .map(|constant| ConstantStub::from_declaration(constant, self))
            .collect()
    }

    fn constants_for_c_style_enum(
        &self,
        enumeration: &CStyleEnumDecl<Native>,
    ) -> Result<Vec<ConstantStub>> {
        self.declarations
            .constants
            .iter()
            .copied()
            .filter(|constant| constant.owner() == Some(ConstantOwner::Enum(enumeration.id())))
            .map(|constant| {
                let constant_name = Name::new(constant.name()).constant()?;
                let colliding_variant = enumeration.variants().iter().try_fold(
                    None,
                    |colliding_variant, variant| -> Result<_> {
                        match colliding_variant {
                            Some(_) => Ok(colliding_variant),
                            None => {
                                let variant_name = Name::new(variant.name()).constant()?;
                                Ok((variant_name == constant_name).then_some(variant))
                            }
                        }
                    },
                )?;
                match colliding_variant {
                    Some(variant)
                        if matches!(
                            constant.owned_enum_variant_alias(),
                            Some((owner, aliased_variant))
                                if owner == enumeration.id()
                                    && aliased_variant == variant.name()
                        ) =>
                    {
                        Ok(None)
                    }
                    _ => ConstantStub::from_declaration(constant, self).map(Some),
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(|constants| constants.into_iter().flatten().collect())
    }

    fn records(&self) -> Result<Vec<RecordClass>> {
        self.declarations
            .records
            .iter()
            .copied()
            .map(|record| match record {
                RecordDecl::Direct(declared) => RecordClass::from_direct(declared, self),
                RecordDecl::Encoded(declared) => RecordClass::from_encoded(declared, self),
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record package",
                }),
            })
            .collect()
    }

    fn enums(&self) -> Result<Vec<EnumClass>> {
        self.declarations
            .enums
            .iter()
            .copied()
            .map(|enumeration| match enumeration {
                EnumDecl::CStyle(declared) => EnumClass::from_c_style(declared, self),
                EnumDecl::Data(declared) => EnumClass::from_data(declared, self),
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

    fn constant_owner_canonical_name(
        &self,
        owner: ConstantOwner,
    ) -> Result<&'bindings CanonicalName> {
        self.context
            .constant_owner_name(owner)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown associated constant owner",
            })
    }

    fn enum_variant_expression(
        &self,
        enum_name: &CanonicalName,
        variant_name: &CanonicalName,
    ) -> Result<Expression> {
        self.declarations
            .enums
            .iter()
            .find_map(|enumeration| match enumeration.name() == enum_name {
                true => match enumeration {
                    EnumDecl::CStyle(_) => Some(enumeration::VariantStyle::CStyle),
                    EnumDecl::Data(_) => Some(enumeration::VariantStyle::Data),
                    _ => None,
                },
                false => None,
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
