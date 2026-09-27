use std::collections::BTreeMap;
use std::fmt;

use boltffi_binding::{CallbackId, EnumId, Native, NativeSymbol, RecordId};

use crate::{
    bridge::c::{self, CBridgeContract, Function, HeaderInclude, Identifier},
    core::{
        BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result,
        contract::sealed,
    },
};

/// Contract for the CPython C extension bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PythonCExtBridgeContract {
    capabilities: BridgeCapabilities,
    module: PythonExtensionName,
    source_path: FilePath,
    c_header: HeaderInclude,
    symbols: ModuleSymbols,
    source_direct_records: BTreeMap<RecordId, c::Record>,
    source_c_style_enums: BTreeMap<EnumId, c::Enum>,
    source_callbacks: BTreeMap<CallbackId, c::Callback>,
    functions: Vec<LoadedFunction>,
    loader: ExtensionMethod,
}

/// CPython extension module name used by `PyInit_<name>`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct PythonExtensionName {
    name: String,
}

/// C bridge function loaded into the CPython extension module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct LoadedFunction {
    function: Function,
    typedef_name: Identifier,
    storage_name: Identifier,
}

/// C identifiers reserved by the CPython extension module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ModuleSymbols {
    init_function: Identifier,
    method_table: Identifier,
    module_definition: Identifier,
    free_function: Identifier,
}

/// CPython method table entry contributed by the bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ExtensionMethod {
    python_name: MethodName,
    c_function: Identifier,
    flags: MethodFlags,
}

/// Python-visible name stored in a CPython method table.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct MethodName {
    name: String,
}

/// CPython call convention flags for one method table entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum MethodFlags {
    /// A method that receives one Python object argument.
    OneObject,
    /// A method that receives Python's fast-call argument array.
    FastCall,
    /// A method that receives no Python arguments.
    NoArgs,
}

impl PythonCExtBridgeContract {
    /// Builds the CPython bridge contract from the C bridge contract.
    pub fn from_c_bridge(
        module: PythonExtensionName,
        source_path: FilePath,
        c_bridge: &CBridgeContract,
    ) -> Result<Self> {
        let functions = c_bridge
            .support()
            .functions()
            .iter()
            .chain(
                c_bridge
                    .callbacks()
                    .iter()
                    .flat_map(|callback| [callback.register(), callback.create_handle()]),
            )
            .chain(c_bridge.functions())
            .map(LoadedFunction::new)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            capabilities: c_bridge
                .capabilities()
                .clone()
                .stable(BridgeCapability::PythonExtension),
            symbols: ModuleSymbols::new(&module)?,
            functions,
            loader: ExtensionMethod::loader()?,
            c_header: HeaderInclude::from_files(&source_path, c_bridge.header_path())?,
            source_direct_records: c_bridge.source_direct_records().clone(),
            source_c_style_enums: c_bridge.source_c_style_enums().clone(),
            source_callbacks: c_bridge
                .callbacks()
                .iter()
                .map(|callback| (callback.id(), callback.clone()))
                .collect(),
            module,
            source_path,
        })
    }

    /// Returns the CPython extension module name.
    pub fn module(&self) -> &PythonExtensionName {
        &self.module
    }

    /// Returns the generated C extension source path.
    pub fn source_path(&self) -> &FilePath {
        &self.source_path
    }

    /// Returns the C header include path used by the extension source.
    pub fn c_header(&self) -> &HeaderInclude {
        &self.c_header
    }

    /// Returns loaded C bridge functions.
    pub fn functions(&self) -> &[LoadedFunction] {
        &self.functions
    }

    /// Returns the loaded C bridge function for a native symbol.
    pub fn loaded_function(&self, symbol: &NativeSymbol) -> Option<&LoadedFunction> {
        self.functions
            .iter()
            .find(|function| function.function().name() == symbol.name().as_str())
    }

    /// Returns the loaded support function that releases a BoltFFI buffer.
    pub fn buffer_free(&self) -> Result<&LoadedFunction> {
        self.support_function(
            "boltffi_free_buf",
            "missing CPython free buffer support symbol",
        )
    }

    /// Returns the loaded support function that copies bytes into a BoltFFI buffer.
    pub fn buffer_from_bytes(&self) -> Result<&LoadedFunction> {
        self.support_function(
            "boltffi_buf_from_bytes",
            "missing CPython buffer copy support symbol",
        )
    }

    pub(crate) fn callback_error(&self) -> Result<&LoadedFunction> {
        self.support_function(
            "boltffi_callback_error",
            "missing CPython callback error support symbol",
        )
    }

    /// Returns the C typedef selected for a direct source record.
    pub fn source_direct_record(&self, record: RecordId) -> Option<&c::Record> {
        self.source_direct_records.get(&record)
    }

    /// Returns the C typedef selected for a source C-style enum.
    pub fn source_c_style_enum(&self, enumeration: EnumId) -> Option<&c::Enum> {
        self.source_c_style_enums.get(&enumeration)
    }

    /// Returns the C vtable selected for a source callback trait.
    pub fn source_callback(&self, callback: CallbackId) -> Option<&c::Callback> {
        self.source_callbacks.get(&callback)
    }

    /// Returns C identifiers reserved by the extension module.
    pub fn symbols(&self) -> &ModuleSymbols {
        &self.symbols
    }

    /// Returns bridge-owned CPython methods.
    pub fn methods(&self) -> &[ExtensionMethod] {
        std::slice::from_ref(&self.loader)
    }

    /// Returns the bridge-owned loader method.
    pub fn loader_method(&self) -> &ExtensionMethod {
        &self.loader
    }

    fn support_function(&self, name: &'static str, shape: &'static str) -> Result<&LoadedFunction> {
        self.functions
            .iter()
            .find(|function| function.function().name() == name)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape,
            })
    }
}

impl BridgeContract for PythonCExtBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for PythonCExtBridgeContract {}

impl PythonExtensionName {
    /// Creates a CPython extension module name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        Identifier::parse(name.clone())?;
        Ok(Self { name })
    }

    /// Returns the module name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl LoadedFunction {
    /// Creates a loaded function from a C bridge function.
    pub fn new(function: &Function) -> Result<Self> {
        let symbol = Identifier::parse(function.name())?;
        Ok(Self {
            function: function.clone(),
            typedef_name: Identifier::parse(format!("boltffi_python_{}_fn", symbol.as_str()))?,
            storage_name: Identifier::parse(format!("boltffi_python_{}", symbol.as_str()))?,
        })
    }

    /// Returns the C bridge function.
    pub fn function(&self) -> &Function {
        &self.function
    }

    /// Returns the C function-pointer typedef name.
    pub fn typedef_name(&self) -> &Identifier {
        &self.typedef_name
    }

    /// Returns the static function-pointer storage name.
    pub fn storage_name(&self) -> &Identifier {
        &self.storage_name
    }
}

impl ModuleSymbols {
    /// Creates CPython extension module symbols.
    pub fn new(module: &PythonExtensionName) -> Result<Self> {
        Ok(Self {
            init_function: Identifier::parse(format!("PyInit_{}", module.as_str()))?,
            method_table: Identifier::parse("boltffi_python_methods")?,
            module_definition: Identifier::parse("boltffi_python_module")?,
            free_function: Identifier::parse("boltffi_python_free_module")?,
        })
    }

    /// Returns the `PyInit_*` function name.
    pub fn init_function(&self) -> &Identifier {
        &self.init_function
    }

    /// Returns the CPython method table identifier.
    pub fn method_table(&self) -> &Identifier {
        &self.method_table
    }

    /// Returns the CPython module definition identifier.
    pub fn module_definition(&self) -> &Identifier {
        &self.module_definition
    }

    /// Returns the module cleanup function identifier.
    pub fn free_function(&self) -> &Identifier {
        &self.free_function
    }
}

impl ExtensionMethod {
    /// Creates a CPython method table entry.
    pub fn new(
        python_name: MethodName,
        c_function: Identifier,
        flags: MethodFlags,
    ) -> Result<Self> {
        Ok(Self {
            python_name,
            c_function,
            flags,
        })
    }

    /// Returns the Python-visible method name.
    pub fn python_name(&self) -> &MethodName {
        &self.python_name
    }

    /// Returns the C function used by the method table entry.
    pub fn c_function(&self) -> &Identifier {
        &self.c_function
    }

    /// Returns the CPython call convention flags.
    pub const fn flags(&self) -> MethodFlags {
        self.flags
    }

    fn loader() -> Result<Self> {
        Self::new(
            MethodName::parse("_initialize_loader")?,
            Identifier::parse("boltffi_python_initialize_loader")?,
            MethodFlags::OneObject,
        )
    }
}

impl MethodName {
    /// Creates a CPython method table name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.bytes().any(|byte| byte == 0) {
            return Err(Error::InvalidPythonMethodName { name });
        }
        Ok(Self { name })
    }

    /// Returns the method name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for MethodName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl MethodFlags {
    /// Returns the CPython C macro for these flags.
    pub const fn as_c_macro(self) -> &'static str {
        match self {
            Self::OneObject => "METH_O",
            Self::FastCall => "METH_FASTCALL",
            Self::NoArgs => "METH_NOARGS",
        }
    }
}
