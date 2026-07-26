mod poll;
mod receiver;
mod signature;

pub use signature::Signature;

use boltffi_binding::{
    CStyleEnumDecl, ClassDecl, ConstantDecl, ConstantValueDecl, DataEnumDecl, DeclarationId,
    DeclarationRef, DirectRecordDecl, EncodedRecordDecl, EnumDecl, ExportedCallable,
    ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, RecordDecl, SymbolId,
};

use crate::core::{Error, Result};

use self::receiver::ReceiverAbi;
use super::{
    C_BRIDGE_LAYER, Identifier, Parameter, ParameterGroup, ParameterIndex, Type, names::Names,
};

/// Meaning of the C ABI return slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ReturnChannel {
    /// The return slot carries the callable success value.
    Value,
    /// The return slot carries an encoded error payload.
    EncodedError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Origin {
    Declaration {
        declaration: DeclarationId,
        symbol: SymbolId,
    },
    Auxiliary,
}

/// A C function declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Function {
    origin: Origin,
    name: Identifier,
    params: Vec<Parameter>,
    parameter_groups: Vec<ParameterGroup>,
    returns: Type,
    return_channel: ReturnChannel,
}

impl Function {
    /// Builds C ABI function declarations exposed by one lowered declaration.
    pub fn from_decl<'decl>(
        decl: DeclarationRef<'decl, Native>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        match decl {
            DeclarationRef::Function(function) => Self::from_callable(
                DeclarationId::Function(function.id()),
                function.symbol(),
                function.callable(),
                Vec::new(),
                names,
            ),
            DeclarationRef::Record(record) => Self::record_functions(record, names),
            DeclarationRef::Enum(enumeration) => Self::enum_functions(enumeration, names),
            DeclarationRef::Class(class) => Self::class_functions(class, names),
            DeclarationRef::Constant(constant) => Self::constant_functions(constant, names),
            DeclarationRef::Stream(_) => Ok(Vec::new()),
            DeclarationRef::Callback(_) | DeclarationRef::CustomType(_) => Ok(Vec::new()),
        }
    }

    /// Creates a C function declaration.
    pub fn new(name: impl Into<String>, params: Vec<Parameter>, returns: Type) -> Result<Self> {
        Self::with_return_channel(name, params, returns, ReturnChannel::Value)
    }

    /// Creates a C function associated with a source native symbol.
    pub fn exported(
        declaration: DeclarationId,
        symbol: &NativeSymbol,
        params: Vec<Parameter>,
        returns: Type,
    ) -> Result<Self> {
        Self::exported_with_channel(declaration, symbol, params, returns, ReturnChannel::Value)
    }

    /// Creates a source-backed C function with an explicit return slot channel.
    pub fn exported_with_channel(
        declaration: DeclarationId,
        symbol: &NativeSymbol,
        params: Vec<Parameter>,
        returns: Type,
        return_channel: ReturnChannel,
    ) -> Result<Self> {
        Self::build(
            symbol.name().as_str(),
            Origin::Declaration {
                declaration,
                symbol: symbol.id(),
            },
            params,
            returns,
            return_channel,
        )
    }

    /// Creates a C function declaration with an explicit return slot channel.
    pub fn with_return_channel(
        name: impl Into<String>,
        params: Vec<Parameter>,
        returns: Type,
        return_channel: ReturnChannel,
    ) -> Result<Self> {
        Self::build(name, Origin::Auxiliary, params, returns, return_channel)
    }

    fn record_functions(record: &RecordDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        match record {
            RecordDecl::Direct(record) => Self::direct_record_functions(record, names),
            RecordDecl::Encoded(record) => Self::encoded_record_functions(record, names),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown record declaration",
            }),
        }
    }

    fn enum_functions(enumeration: &EnumDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        match enumeration {
            EnumDecl::CStyle(enumeration) => Self::c_style_enum_functions(enumeration, names),
            EnumDecl::Data(enumeration) => Self::data_enum_functions(enumeration, names),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown enum declaration",
            }),
        }
    }

    fn direct_record_functions(
        record: &DirectRecordDecl<Native>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        Self::associated_functions(
            DeclarationId::Record(record.id()),
            record.initializers(),
            record.methods(),
            ReceiverAbi::direct("receiver", Type::DirectRecord(names.record(record.id())?))?,
            names,
        )
    }

    fn encoded_record_functions(
        record: &EncodedRecordDecl<Native>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        Self::associated_functions(
            DeclarationId::Record(record.id()),
            record.initializers(),
            record.methods(),
            ReceiverAbi::encoded("receiver")?,
            names,
        )
    }

    fn c_style_enum_functions(
        enumeration: &CStyleEnumDecl<Native>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        Self::associated_functions(
            DeclarationId::Enum(enumeration.id()),
            enumeration.initializers(),
            enumeration.methods(),
            ReceiverAbi::direct(
                "receiver",
                Type::CStyleEnum {
                    name: names.enumeration(enumeration.id())?,
                    repr: Box::new(Type::primitive(enumeration.repr().primitive())?),
                },
            )?,
            names,
        )
    }

    fn data_enum_functions(enumeration: &DataEnumDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        Self::associated_functions(
            DeclarationId::Enum(enumeration.id()),
            enumeration.initializers(),
            enumeration.methods(),
            ReceiverAbi::encoded("receiver")?,
            names,
        )
    }

    fn class_functions(class: &ClassDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let declaration = DeclarationId::Class(class.id());
        let receiver = ReceiverAbi::plain([Parameter::new(
            "receiver",
            Type::handle_carrier(class.handle())?,
        )?]);
        let release = Self::exported(
            declaration,
            class.release(),
            vec![Parameter::new(
                "handle",
                Type::handle_carrier(class.handle())?,
            )?],
            Type::Void,
        )?;
        let functions = Self::associated_functions(
            declaration,
            class.initializers(),
            class.methods(),
            receiver,
            names,
        )?;
        Ok(std::iter::once(release).chain(functions).collect())
    }

    fn constant_functions(constant: &ConstantDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        match constant.value() {
            ConstantValueDecl::Inline { .. } => Ok(Vec::new()),
            ConstantValueDecl::Accessor { symbol, callable } => Self::from_callable(
                DeclarationId::Constant(constant.id()),
                symbol,
                callable,
                Vec::new(),
                names,
            ),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown constant value declaration",
            }),
        }
    }

    fn associated_functions(
        declaration: DeclarationId,
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: ReceiverAbi,
        names: &Names,
    ) -> Result<Vec<Self>> {
        let initializers = initializers
            .iter()
            .map(|initializer| {
                Self::from_callable(
                    declaration,
                    initializer.symbol(),
                    initializer.callable(),
                    Vec::new(),
                    names,
                )
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();
        let methods = methods
            .iter()
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| receiver.parameters(receive))
                    .unwrap_or_default();
                Self::from_callable(
                    declaration,
                    method.target(),
                    method.callable(),
                    receiver,
                    names,
                )
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();
        Ok(initializers.chain(methods).collect())
    }

    fn from_callable(
        declaration: DeclarationId,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver: impl IntoIterator<Item = Parameter>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        Signature::new(names, receiver).exported(declaration, symbol, callable)
    }

    fn build(
        name: impl Into<String>,
        origin: Origin,
        params: Vec<Parameter>,
        returns: Type,
        return_channel: ReturnChannel,
    ) -> Result<Self> {
        let parameter_groups = ParameterGroup::from_params(&params)?;
        Ok(Self {
            origin,
            name: Identifier::parse(name)?,
            params,
            parameter_groups,
            returns,
            return_channel,
        })
    }
}

impl Function {
    /// Returns the source binding declaration represented by this C function.
    pub const fn source_declaration(&self) -> Option<DeclarationId> {
        match self.origin {
            Origin::Declaration { declaration, .. } => Some(declaration),
            Origin::Auxiliary => None,
        }
    }

    /// Returns the source native symbol represented by this C function.
    pub const fn source_symbol(&self) -> Option<SymbolId> {
        match self.origin {
            Origin::Declaration { symbol, .. } => Some(symbol),
            Origin::Auxiliary => None,
        }
    }

    /// Returns the C symbol name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the parameters in C ABI order.
    pub fn params(&self) -> &[Parameter] {
        &self.params
    }

    /// Returns source-level parameter groups in declaration order.
    pub fn parameter_groups(&self) -> &[ParameterGroup] {
        &self.parameter_groups
    }

    /// Returns the C ABI parameter at the given position.
    pub fn parameter(&self, index: ParameterIndex) -> &Parameter {
        &self.params[index.position()]
    }

    /// Returns the C return type.
    pub fn returns(&self) -> &Type {
        &self.returns
    }

    /// Returns the semantic channel carried by the C return slot.
    pub fn return_channel(&self) -> ReturnChannel {
        self.return_channel
    }
}
