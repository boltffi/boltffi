use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CallbackId, ClassId, Decl, DeclarationRef, DirectValueType, EnumDecl, EnumId, Native,
    RecordDecl, RecordId, StreamId, native,
};

use crate::core::{Error, Result};

use super::{C_BRIDGE_CONTRACT, C_BRIDGE_LAYER, Identifier, Type, name};

#[derive(Clone, Debug, Default)]
pub struct Names {
    direct_records: BTreeMap<RecordId, Identifier>,
    enums: BTreeMap<EnumId, Identifier>,
    c_style_enum_reprs: BTreeMap<EnumId, Type>,
    classes: BTreeMap<ClassId, Identifier>,
    class_handles: BTreeMap<ClassId, native::HandleCarrier>,
    callbacks: BTreeMap<CallbackId, Identifier>,
    streams: BTreeMap<StreamId, Identifier>,
}

impl Names {
    pub fn new(bindings: &Bindings<Native>) -> Result<Self> {
        bindings
            .decls()
            .iter()
            .try_fold(Self::default(), |mut names, decl| {
                names.insert(decl)?;
                Ok(names)
            })
    }

    pub fn record(&self, id: RecordId) -> Result<Identifier> {
        self.direct_records
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing direct record type name",
            })
    }

    pub fn enumeration(&self, id: EnumId) -> Result<Identifier> {
        self.enums
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing enum type name",
            })
    }

    pub fn callback(&self, id: CallbackId) -> Result<Identifier> {
        self.callbacks
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing callback type name",
            })
    }

    pub fn class_handle(&self, id: ClassId) -> Result<native::HandleCarrier> {
        self.class_handles
            .get(&id)
            .copied()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing class handle carrier",
            })
    }

    pub fn direct_value(&self, ty: &DirectValueType) -> Result<Type> {
        match ty {
            DirectValueType::Primitive(primitive) => Type::primitive(*primitive),
            DirectValueType::Record(record) => self.record(*record).map(Type::DirectRecord),
            DirectValueType::Enum(enumeration) => Ok(Type::CStyleEnum {
                name: self.enumeration(*enumeration)?,
                repr: Box::new(self.c_style_enum_repr(*enumeration)?),
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "direct value type",
            }),
        }
    }

    fn insert(&mut self, decl: &Decl<Native>) -> Result<()> {
        match DeclarationRef::from(decl) {
            DeclarationRef::Record(RecordDecl::Direct(record)) => {
                let id = record.id();
                let name = Identifier::parse(name::Spelling::new(record.name()).typedef())?;
                self.direct_records.insert(id, name);
            }
            DeclarationRef::Record(RecordDecl::Encoded(_)) => {}
            DeclarationRef::Record(_) => {}
            DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => {
                let id = enumeration.id();
                let name = Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?;
                let repr = Type::primitive(enumeration.repr().primitive())?;
                self.enums.insert(id, name);
                self.c_style_enum_reprs.insert(id, repr);
            }
            DeclarationRef::Enum(EnumDecl::Data(enumeration)) => {
                let id = enumeration.id();
                let name = Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?;
                self.enums.insert(id, name);
            }
            DeclarationRef::Enum(enumeration) => {
                self.enums.insert(
                    enumeration.id(),
                    Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?,
                );
            }
            DeclarationRef::Class(class) => {
                self.classes.insert(
                    class.id(),
                    Identifier::parse(name::Spelling::new(class.name()).typedef())?,
                );
                self.class_handles.insert(class.id(), class.handle());
            }
            DeclarationRef::Callback(callback) => {
                self.callbacks.insert(
                    callback.id(),
                    Identifier::parse(name::Spelling::new(callback.name()).typedef())?,
                );
            }
            DeclarationRef::Stream(stream) => {
                self.streams.insert(
                    stream.id(),
                    Identifier::parse(name::Spelling::new(stream.name()).typedef())?,
                );
            }
            DeclarationRef::CustomType(_) => {}
            DeclarationRef::Function(_) | DeclarationRef::Constant(_) => {}
        }
        Ok(())
    }

    fn c_style_enum_repr(&self, id: EnumId) -> Result<Type> {
        self.c_style_enum_reprs
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing C-style enum representation",
            })
    }
}
