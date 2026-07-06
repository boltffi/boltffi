use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CallbackId, DeclarationRef, EnumDecl, EnumId, Native, RecordDecl, RecordId, StreamId,
};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result, contract::sealed,
};

use super::names::Names;
use super::{
    C_BRIDGE_CONTRACT, C_BRIDGE_LAYER, Callback, Enum, Function, Record, Stream, SupportFunctions,
};

/// C ABI contract produced for native bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CBridgeContract {
    capabilities: BridgeCapabilities,
    header_path: FilePath,
    support: SupportFunctions,
    direct_records: Vec<Record>,
    source_direct_records: BTreeMap<RecordId, Record>,
    source_c_style_enums: BTreeMap<EnumId, Enum>,
    enums: Vec<Enum>,
    callbacks: Vec<Callback>,
    source_callbacks: BTreeMap<CallbackId, Callback>,
    streams: Vec<Stream>,
    source_streams: BTreeMap<StreamId, Stream>,
    functions: Vec<Function>,
}

impl CBridgeContract {
    /// Builds the C ABI contract for native bindings.
    pub fn from_bindings(bindings: &Bindings<Native>, header_path: FilePath) -> Result<Self> {
        let names = Names::new(bindings)?;
        let source_direct_records =
            bindings
                .decls()
                .iter()
                .try_fold(BTreeMap::new(), |mut records, decl| {
                    match DeclarationRef::from(decl) {
                        DeclarationRef::Record(RecordDecl::Direct(record)) => {
                            records.insert(record.id(), Record::direct(record, &names)?);
                        }
                        DeclarationRef::Record(RecordDecl::Encoded(_)) => {}
                        DeclarationRef::Record(_) => {
                            return Err(Error::UnexpectedBindingShape {
                                layer: C_BRIDGE_LAYER,
                                shape: "unknown record declaration",
                            });
                        }
                        DeclarationRef::Enum(_)
                        | DeclarationRef::Function(_)
                        | DeclarationRef::Class(_)
                        | DeclarationRef::Callback(_)
                        | DeclarationRef::Stream(_)
                        | DeclarationRef::Constant(_)
                        | DeclarationRef::CustomType(_) => {}
                    }
                    Ok(records)
                })?;
        let direct_records = source_direct_records.values().cloned().collect();
        let enums = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(enumeration) => Some(enumeration),
                DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| Enum::from_decl(enumeration, &names))
            .collect::<Result<Vec<_>>>()?;
        let source_c_style_enums = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => Some(enumeration),
                DeclarationRef::Enum(EnumDecl::Data(_))
                | DeclarationRef::Enum(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| Ok((enumeration.id(), Enum::c_style(enumeration, &names)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let source_callbacks = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Callback(callback) => Some(callback),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|callback| {
                Callback::from_decl(callback, &names).map(|protocol| (callback.id(), protocol))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let callbacks = source_callbacks.values().cloned().collect::<Vec<_>>();
        let source_streams = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Stream(stream) => Some(stream),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|stream| Stream::from_decl(stream, &names).map(|protocol| (stream.id(), protocol)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let streams = source_streams.values().cloned().collect::<Vec<_>>();
        let functions = bindings
            .decls()
            .iter()
            .map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Stream(stream) => source_streams
                    .get(&stream.id())
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: C_BRIDGE_CONTRACT,
                        invariant: "missing typed stream protocol",
                    })
                    .map(|stream| stream.functions().into_iter().cloned().collect::<Vec<_>>()),
                decl => Function::from_decl(decl, &names),
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();

        Ok(Self {
            capabilities: BridgeCapabilities::new().stable(BridgeCapability::CAbi),
            header_path,
            support: SupportFunctions::new()?,
            direct_records,
            source_direct_records,
            source_c_style_enums,
            enums,
            callbacks,
            source_callbacks,
            streams,
            source_streams,
            functions,
        })
    }

    /// Returns the generated C header path.
    pub fn header_path(&self) -> &FilePath {
        &self.header_path
    }

    /// Returns C typedefs for direct source records.
    pub fn direct_records(&self) -> &[Record] {
        &self.direct_records
    }

    /// Returns the C typedef selected for a direct source record.
    pub fn source_direct_record(&self, record: RecordId) -> Option<&Record> {
        self.source_direct_records.get(&record)
    }

    /// Returns C typedefs keyed by direct source record id.
    pub fn source_direct_records(&self) -> &BTreeMap<RecordId, Record> {
        &self.source_direct_records
    }

    /// Returns the C typedef selected for a source C-style enum.
    pub fn source_c_style_enum(&self, enumeration: EnumId) -> Option<&Enum> {
        self.source_c_style_enums.get(&enumeration)
    }

    /// Returns C typedefs keyed by source C-style enum id.
    pub fn source_c_style_enums(&self) -> &BTreeMap<EnumId, Enum> {
        &self.source_c_style_enums
    }

    /// Returns C ABI support functions.
    pub fn support(&self) -> &SupportFunctions {
        &self.support
    }

    /// Returns C enum declarations.
    pub fn enums(&self) -> &[Enum] {
        &self.enums
    }

    /// Returns C callback vtable declarations.
    pub fn callbacks(&self) -> &[Callback] {
        &self.callbacks
    }

    /// Returns the C callback protocol selected for a source callback.
    pub fn source_callback(&self, callback: CallbackId) -> Option<&Callback> {
        self.source_callbacks.get(&callback)
    }

    /// Returns C stream protocol declarations.
    pub fn streams(&self) -> &[Stream] {
        &self.streams
    }

    /// Returns the C stream protocol selected for a source stream.
    pub fn source_stream(&self, stream: StreamId) -> Option<&Stream> {
        self.source_streams.get(&stream)
    }

    /// Returns C function declarations.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
}

impl BridgeContract for CBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for CBridgeContract {}
