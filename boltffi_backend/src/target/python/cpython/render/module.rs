use std::collections::BTreeSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{DeclarationRef, Native};

use crate::{
    bridge::{
        c::{Identifier, Literal, Statement},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Emitted, FileLayout, GeneratedOutput, RenderContext, RenderedDeclaration, Result},
    target::python::{
        codec::{CodecAdapters, ReadAdapter, WriteAdapter},
        cpython::{
            codec,
            render::{
                callback, class, constant, direct_vector, enumeration, function, method, primitive,
                record, result, stream,
            },
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/native_module.c", escape = "none")]
struct NativeModuleTemplate {
    module_name: String,
    method_table: Identifier,
    module_definition: Identifier,
    free_function: Identifier,
    init_function: Identifier,
    support: ModuleSupport,
    records: Vec<String>,
    enums: Vec<String>,
    classes: Vec<String>,
    callback_declarations: Vec<Statement>,
    callbacks: Vec<String>,
    streams: Vec<String>,
    constants: Vec<String>,
    codec_decoders: Vec<CodecDecoder>,
    codec_encoders: Vec<CodecEncoder>,
    host_bindings: Vec<String>,
    functions: Vec<String>,
    methods: Vec<method::Entry>,
    cleanup: Vec<Statement>,
    type_setups: Vec<Identifier>,
}

pub struct NativeModule<'render, 'bindings> {
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'bindings, Native>,
    declarations: Vec<RenderedDeclaration<'bindings, Native>>,
}

impl<'render, 'bindings> NativeModule<'render, 'bindings> {
    pub fn new(
        bridge: &'render PythonCExtBridgeContract,
        context: &'render RenderContext<'bindings, Native>,
        declarations: Vec<RenderedDeclaration<'bindings, Native>>,
    ) -> Self {
        Self {
            bridge,
            context,
            declarations,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let bridge = self.bridge;
        let declaration_refs = self
            .declarations
            .iter()
            .map(RenderedDeclaration::declaration)
            .collect::<Vec<_>>();
        let declarations =
            ModuleDeclarations::collect(self.bridge, self.context, &self.declarations)?;
        let codec_adapters =
            CodecAdapters::from_declarations(self.context.bindings(), &declaration_refs);
        let codec_decoders = codec_adapters
            .decoders()
            .iter()
            .map(CodecDecoder::from_adapter)
            .collect::<Result<Vec<_>>>()?;
        let codec_encoders = codec_adapters
            .encoders()
            .iter()
            .map(CodecEncoder::from_adapter)
            .collect::<Result<Vec<_>>>()?;
        let methods = declarations.methods(bridge);
        let support = ModuleSupport::new(bridge, declarations.support())?;
        let source = NativeModuleTemplate {
            module_name: bridge.module().as_str().to_owned(),
            method_table: bridge.symbols().method_table().clone(),
            module_definition: bridge.symbols().module_definition().clone(),
            free_function: bridge.symbols().free_function().clone(),
            init_function: bridge.symbols().init_function().clone(),
            support,
            records: declarations.record_sources(),
            enums: declarations.enum_sources(),
            classes: declarations.class_sources(),
            callback_declarations: declarations.callback_declarations(),
            callbacks: declarations.callback_sources(),
            streams: declarations.stream_sources(),
            constants: declarations.constant_sources(),
            codec_decoders,
            codec_encoders,
            host_bindings: declarations.host_bindings(),
            functions: declarations.function_sources(),
            methods,
            cleanup: declarations.cleanup(),
            type_setups: declarations.type_setups()?,
        }
        .render()?;
        FileLayout::single(bridge.source_path().clone()).assemble([Emitted::primary(source)])
    }
}

#[derive(Default)]
struct ModuleDeclarations {
    records: Vec<Rendered<record::Record>>,
    enums: Vec<Rendered<enumeration::Enumeration>>,
    classes: Vec<Rendered<class::Class>>,
    callbacks: Vec<Rendered<callback::Callback>>,
    streams: Vec<Rendered<stream::Stream>>,
    constants: Vec<Rendered<constant::Constant>>,
    functions: Vec<Rendered<function::Function>>,
}

impl ModuleDeclarations {
    fn collect<'decl, 'render>(
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<'render, Native>,
        declarations: &[RenderedDeclaration<'decl, Native>],
    ) -> Result<Self> {
        declarations
            .iter()
            .try_fold(Self::default(), |module, declaration| {
                module.collect_declaration(bridge, context, declaration)
            })
    }

    fn record_sources(&self) -> Vec<String> {
        self.records.iter().map(Rendered::source).collect()
    }

    fn type_setups(&self) -> Result<Vec<Identifier>> {
        self.records
            .iter()
            .filter_map(|record| record.declaration.type_setup().transpose())
            .collect()
    }

    fn enum_sources(&self) -> Vec<String> {
        self.enums.iter().map(Rendered::source).collect()
    }

    fn class_sources(&self) -> Vec<String> {
        self.classes.iter().map(Rendered::source).collect()
    }

    fn callback_declarations(&self) -> Vec<Statement> {
        self.callbacks
            .iter()
            .flat_map(|callback| callback.declaration.parser_declarations())
            .collect()
    }

    fn callback_sources(&self) -> Vec<String> {
        self.callbacks.iter().map(Rendered::source).collect()
    }

    fn stream_sources(&self) -> Vec<String> {
        self.streams.iter().map(Rendered::source).collect()
    }

    fn constant_sources(&self) -> Vec<String> {
        self.constants.iter().map(Rendered::source).collect()
    }

    fn function_sources(&self) -> Vec<String> {
        self.functions.iter().map(Rendered::source).collect()
    }

    fn host_bindings(&self) -> Vec<String> {
        self.callbacks
            .iter()
            .map(|callback| callback.declaration.binding().to_owned())
            .collect()
    }

    fn methods(&self, bridge: &PythonCExtBridgeContract) -> Vec<method::Entry> {
        bridge
            .methods()
            .iter()
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.declaration.methods()),
            )
            .chain(
                self.enums
                    .iter()
                    .flat_map(|enumeration| enumeration.declaration.methods()),
            )
            .chain(
                self.classes
                    .iter()
                    .flat_map(|class| class.declaration.methods()),
            )
            .chain(
                self.streams
                    .iter()
                    .flat_map(|stream| stream.declaration.methods()),
            )
            .chain(
                self.constants
                    .iter()
                    .flat_map(|constant| constant.declaration.methods()),
            )
            .chain(
                self.functions
                    .iter()
                    .flat_map(|function| function.declaration.methods()),
            )
            .map(method::Entry::from_method)
            .collect()
    }

    fn cleanup(&self) -> Vec<Statement> {
        self.records
            .iter()
            .map(|record| record.declaration.cleanup())
            .chain(
                self.enums
                    .iter()
                    .map(|enumeration| enumeration.declaration.cleanup()),
            )
            .collect()
    }

    fn support<'module>(&'module self) -> SupportArtifacts<'module> {
        SupportArtifacts {
            records: self.records.iter().map(Rendered::declaration).collect(),
            enums: self.enums.iter().map(Rendered::declaration).collect(),
            classes: self.classes.iter().map(Rendered::declaration).collect(),
            callbacks: self.callbacks.iter().map(Rendered::declaration).collect(),
            streams: self.streams.iter().map(Rendered::declaration).collect(),
            constants: self.constants.iter().map(Rendered::declaration).collect(),
            functions: self.functions.iter().map(Rendered::declaration).collect(),
        }
    }

    fn collect_declaration<'decl, 'render>(
        mut self,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<'render, Native>,
        declaration: &RenderedDeclaration<'decl, Native>,
    ) -> Result<Self> {
        match declaration.declaration() {
            DeclarationRef::Record(record) => self.records.push(Rendered::new(
                record::Record::from_declaration(record, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Enum(enumeration) => self.enums.push(Rendered::new(
                enumeration::Enumeration::from_declaration(enumeration, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Function(function) => self.functions.push(Rendered::new(
                function::Function::from_declaration(function, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Class(class) => self.classes.push(Rendered::new(
                class::Class::from_declaration(class, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Callback(callback) => self.callbacks.push(Rendered::new(
                callback::Callback::from_declaration(callback, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Stream(stream) => self.streams.push(Rendered::new(
                stream::Stream::from_declaration(stream, bridge, context)?,
                declaration,
            )),
            DeclarationRef::Constant(constant) => self.constants.push(Rendered::new(
                constant::Constant::from_declaration(constant, bridge, context)?,
                declaration,
            )),
            DeclarationRef::CustomType(_) => {}
        }
        Ok(self)
    }
}

struct Rendered<T> {
    declaration: T,
    source: String,
}

impl<T> Rendered<T> {
    fn new<'decl>(declaration: T, rendered: &RenderedDeclaration<'decl, Native>) -> Self {
        Self {
            declaration,
            source: rendered.emitted().primary_chunk().as_str().to_owned(),
        }
    }

    fn declaration(&self) -> &T {
        &self.declaration
    }

    fn source(&self) -> String {
        self.source.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CodecDecoder {
    key: Literal,
    function: Identifier,
}

impl CodecDecoder {
    fn from_adapter<'adapter>(adapter: &ReadAdapter<'adapter>) -> Result<Self> {
        Ok(Self {
            key: adapter.key().c_literal(),
            function: adapter.key().c_decoder()?,
        })
    }

    fn key(&self) -> &Literal {
        &self.key
    }

    fn function(&self) -> &Identifier {
        &self.function
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CodecEncoder {
    key: Literal,
    function: Identifier,
}

impl CodecEncoder {
    fn from_adapter<'adapter>(adapter: &WriteAdapter<'adapter>) -> Result<Self> {
        Ok(Self {
            key: adapter.key().c_literal(),
            function: adapter.key().c_encoder()?,
        })
    }

    fn key(&self) -> &Literal {
        &self.key
    }

    fn function(&self) -> &Identifier {
        &self.function
    }
}

struct SupportArtifacts<'module> {
    records: Vec<&'module record::Record>,
    enums: Vec<&'module enumeration::Enumeration>,
    classes: Vec<&'module class::Class>,
    callbacks: Vec<&'module callback::Callback>,
    streams: Vec<&'module stream::Stream>,
    constants: Vec<&'module constant::Constant>,
    functions: Vec<&'module function::Function>,
}

impl<'module> SupportArtifacts<'module> {
    fn has_encoded_records(&self) -> bool {
        self.records
            .iter()
            .any(|record| record.needs_owned_buffer())
    }

    fn has_data_enums(&self) -> bool {
        self.enums
            .iter()
            .any(|enumeration| enumeration.needs_owned_buffer())
    }

    fn uses_async_protocol(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.uses_async_protocol())
            || self
                .records
                .iter()
                .any(|record| record.uses_async_protocol())
            || self
                .enums
                .iter()
                .any(|enumeration| enumeration.uses_async_protocol())
            || self.classes.iter().any(|class| class.uses_async_protocol())
    }

    fn owned_buffers(&self) -> BTreeSet<result::OwnedBuffer> {
        self.functions
            .iter()
            .flat_map(|function| function.owned_buffers())
            .chain(
                self.constants
                    .iter()
                    .filter_map(|constant| constant.owned_buffer()),
            )
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.owned_buffers()),
            )
            .chain(
                self.enums
                    .iter()
                    .flat_map(|enumeration| enumeration.owned_buffers()),
            )
            .chain(self.classes.iter().flat_map(|class| class.owned_buffers()))
            .chain(
                self.streams
                    .iter()
                    .filter_map(|stream| stream.owned_buffer()),
            )
            .collect()
    }

    fn direct_vector_elements(
        &self,
        owned_buffers: &BTreeSet<result::OwnedBuffer>,
    ) -> BTreeSet<direct_vector::Element> {
        owned_buffers
            .iter()
            .filter_map(|buffer| match buffer {
                result::OwnedBuffer::DirectVector(element) => Some((**element).clone()),
                result::OwnedBuffer::RawWire
                | result::OwnedBuffer::Utf8Text
                | result::OwnedBuffer::OptionalPrimitive(_)
                | result::OwnedBuffer::Native(_) => None,
            })
            .chain(
                self.functions
                    .iter()
                    .flat_map(|function| function.direct_vector_elements()),
            )
            .chain(
                self.constants
                    .iter()
                    .flat_map(|constant| constant.direct_vector_elements()),
            )
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.direct_vector_elements()),
            )
            .chain(
                self.enums
                    .iter()
                    .flat_map(|enumeration| enumeration.direct_vector_elements()),
            )
            .chain(
                self.classes
                    .iter()
                    .flat_map(|class| class.direct_vector_elements()),
            )
            .chain(
                self.callbacks
                    .iter()
                    .flat_map(|callback| callback.direct_vector_elements()),
            )
            .collect()
    }

    fn primitives(
        &self,
        direct_vector_elements: &BTreeSet<direct_vector::Element>,
    ) -> Result<Vec<primitive::Support>> {
        self.functions
            .iter()
            .flat_map(|function| function.primitives())
            .chain(
                self.constants
                    .iter()
                    .flat_map(|constant| constant.primitives()),
            )
            .chain(self.classes.iter().flat_map(|class| class.primitives()))
            .chain(
                self.callbacks
                    .iter()
                    .flat_map(|callback| callback.primitives()),
            )
            .chain(self.streams.iter().flat_map(|stream| stream.primitives()))
            .chain(self.records.iter().flat_map(|record| record.primitives()))
            .chain(
                self.enums
                    .iter()
                    .filter_map(|enumeration| enumeration.primitive()),
            )
            .chain(
                direct_vector_elements
                    .iter()
                    .filter_map(direct_vector::Element::runtime_primitive),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect()
    }

    fn wire_primitives(&self) -> Result<Vec<primitive::Support>> {
        self.functions
            .iter()
            .flat_map(|function| function.wire_primitives())
            .chain(
                self.constants
                    .iter()
                    .flat_map(|constant| constant.wire_primitives()),
            )
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.wire_primitives()),
            )
            .chain(
                self.enums
                    .iter()
                    .flat_map(|enumeration| enumeration.wire_primitives()),
            )
            .chain(
                self.classes
                    .iter()
                    .flat_map(|class| class.wire_primitives()),
            )
            .chain(
                self.callbacks
                    .iter()
                    .flat_map(|callback| callback.wire_primitives()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect()
    }

    fn native_sequences(
        &self,
        owned_buffers: &BTreeSet<result::OwnedBuffer>,
    ) -> Vec<codec::NativeSequence> {
        owned_buffers
            .iter()
            .filter_map(result::OwnedBuffer::native_sequence)
            .chain(
                self.functions
                    .iter()
                    .flat_map(|function| function.native_sequences()),
            )
            .chain(
                self.constants
                    .iter()
                    .flat_map(|constant| constant.native_sequences()),
            )
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.native_sequences()),
            )
            .chain(
                self.enums
                    .iter()
                    .flat_map(|enumeration| enumeration.native_sequences()),
            )
            .chain(
                self.classes
                    .iter()
                    .flat_map(|class| class.native_sequences()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn owned_primitives(
        &self,
        owned_buffers: &BTreeSet<result::OwnedBuffer>,
    ) -> Result<Vec<primitive::Support>> {
        owned_buffers
            .iter()
            .filter_map(|buffer| match buffer {
                result::OwnedBuffer::OptionalPrimitive(primitive) => Some(*primitive),
                result::OwnedBuffer::RawWire
                | result::OwnedBuffer::Utf8Text
                | result::OwnedBuffer::DirectVector(_)
                | result::OwnedBuffer::Native(_) => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect()
    }

    fn has_string_arguments(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.has_string_argument())
            || self
                .constants
                .iter()
                .any(|constant| constant.has_string_argument())
            || self
                .records
                .iter()
                .any(|record| record.has_string_argument())
            || self
                .enums
                .iter()
                .any(|enumeration| enumeration.has_string_argument())
            || self.classes.iter().any(|class| class.has_string_argument())
            || self
                .callbacks
                .iter()
                .any(|callback| callback.has_string_argument())
    }

    fn has_bytes_arguments(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.has_bytes_argument())
            || self
                .constants
                .iter()
                .any(|constant| constant.has_bytes_argument())
            || self
                .records
                .iter()
                .any(|record| record.has_bytes_argument())
            || self
                .enums
                .iter()
                .any(|enumeration| enumeration.has_bytes_argument())
            || self.classes.iter().any(|class| class.has_bytes_argument())
            || self
                .callbacks
                .iter()
                .any(|callback| callback.has_bytes_argument())
    }

    fn has_raw_wire_arguments(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.has_raw_wire_argument())
            || self
                .constants
                .iter()
                .any(|constant| constant.has_raw_wire_argument())
            || self
                .records
                .iter()
                .any(|record| record.has_raw_wire_argument())
            || self
                .enums
                .iter()
                .any(|enumeration| enumeration.has_raw_wire_argument())
            || self
                .classes
                .iter()
                .any(|class| class.has_raw_wire_argument())
            || self
                .callbacks
                .iter()
                .any(|callback| callback.has_raw_wire_argument())
    }
}

struct ModuleSupport {
    primitives: Vec<primitive::Support>,
    wire_primitives: Vec<primitive::Support>,
    owned_primitives: Vec<primitive::Support>,
    direct_vector_elements: Vec<direct_vector::Element>,
    native_sequences: Vec<codec::NativeSequence>,
    free_buffer: Identifier,
    string_arguments: bool,
    bytes_arguments: bool,
    raw_wire_arguments: bool,
    raw_wire_returns: bool,
    utf8_returns: bool,
    native_record_types: bool,
    encoded_records: bool,
    data_enums: bool,
    record_types: bool,
    c_style_enums: bool,
    callback_handles: bool,
    async_functions: bool,
}

impl ModuleSupport {
    fn new<'module>(
        bridge: &PythonCExtBridgeContract,
        artifacts: SupportArtifacts<'module>,
    ) -> Result<Self> {
        let encoded_records = artifacts.has_encoded_records();
        let data_enums = artifacts.has_data_enums();
        let async_functions = artifacts.uses_async_protocol();
        let owned_buffers = artifacts.owned_buffers();
        let direct_vector_elements = artifacts.direct_vector_elements(&owned_buffers);
        let primitives = artifacts.primitives(&direct_vector_elements)?;
        let wire_primitives = artifacts.wire_primitives()?;
        let native_sequences = artifacts.native_sequences(&owned_buffers);
        let owned_primitives = artifacts.owned_primitives(&owned_buffers)?;
        let string_arguments = artifacts.has_string_arguments();
        let bytes_arguments = artifacts.has_bytes_arguments();
        let raw_wire_arguments = artifacts.has_raw_wire_arguments();
        Ok(Self {
            primitives,
            wire_primitives,
            owned_primitives,
            direct_vector_elements: direct_vector_elements.into_iter().collect(),
            native_sequences,
            free_buffer: Self::free_buffer_storage(bridge)?,
            string_arguments,
            bytes_arguments,
            raw_wire_arguments,
            raw_wire_returns: owned_buffers.contains(&result::OwnedBuffer::RawWire),
            utf8_returns: owned_buffers.contains(&result::OwnedBuffer::Utf8Text),
            native_record_types: artifacts
                .records
                .iter()
                .any(|record| record.has_native_type()),
            encoded_records,
            data_enums,
            record_types: !artifacts.records.is_empty(),
            c_style_enums: !artifacts.enums.is_empty(),
            callback_handles: !artifacts.callbacks.is_empty(),
            async_functions,
        })
    }

    fn primitives(&self) -> &[primitive::Support] {
        &self.primitives
    }

    fn wire_primitives(&self) -> &[primitive::Support] {
        &self.wire_primitives
    }

    fn owned_primitives(&self) -> &[primitive::Support] {
        &self.owned_primitives
    }

    fn direct_vector_elements(&self) -> &[direct_vector::Element] {
        &self.direct_vector_elements
    }

    fn native_sequences(&self) -> &[codec::NativeSequence] {
        &self.native_sequences
    }

    fn free_buffer(&self) -> &Identifier {
        &self.free_buffer
    }

    fn uses_wire_arguments(&self) -> bool {
        self.string_arguments
            || self.bytes_arguments
            || self.raw_wire_arguments
            || !self.wire_primitives.is_empty()
    }

    fn uses_owned_buffers(&self) -> bool {
        self.raw_wire_returns
            || self.utf8_returns
            || !self.owned_primitives.is_empty()
            || !self.direct_vector_elements.is_empty()
            || !self.native_sequences.is_empty()
            || self.encoded_records
            || self.data_enums
            || self.c_style_enums
            || self.async_functions
    }

    fn uses_wire_strings(&self) -> bool {
        self.string_arguments
    }

    fn uses_wire_bytes(&self) -> bool {
        self.bytes_arguments
    }

    fn uses_raw_wire_arguments(&self) -> bool {
        self.raw_wire_arguments
    }

    fn uses_owned_utf8(&self) -> bool {
        self.async_functions || self.utf8_returns
    }

    fn uses_owned_bytes(&self) -> bool {
        false
    }

    fn uses_owned_raw_wire(&self) -> bool {
        self.raw_wire_returns
    }

    fn uses_c_style_enums(&self) -> bool {
        self.c_style_enums
    }

    fn uses_callback_handles(&self) -> bool {
        self.callback_handles
    }

    fn uses_registered_types(&self) -> bool {
        self.record_types || self.c_style_enums
    }

    fn uses_native_record_types(&self) -> bool {
        self.native_record_types
    }

    fn uses_async_functions(&self) -> bool {
        self.async_functions
    }

    fn free_buffer_storage(bridge: &PythonCExtBridgeContract) -> Result<Identifier> {
        Ok(bridge.buffer_free()?.storage_name().clone())
    }
}
