use boltffi_binding::{DirectValueType, EnumDecl, EnumId, Native, Primitive};

use crate::{
    bridge::c::CBridgeContract,
    core::{Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        primitive::SwiftPrimitive,
        render::SwiftType,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectValue {
    api_type: TypeName,
    storage_type: TypeName,
    conversion: Conversion,
    payload: DirectPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Conversion {
    Identity,
    CValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirectPayload {
    Primitive(Primitive),
    Encodable,
    CStyleEnum(Primitive),
}

impl DirectValue {
    pub fn new(
        ty: &DirectValueType,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match ty {
            DirectValueType::Primitive(primitive) => {
                let ty = SwiftType::primitive(*primitive)?;
                Ok(Self {
                    api_type: ty.clone(),
                    storage_type: ty,
                    conversion: Conversion::Identity,
                    payload: DirectPayload::Primitive(*primitive),
                })
            }
            DirectValueType::Record(record) => Ok(Self {
                api_type: SwiftType::record(*record, context)?,
                storage_type: SwiftType::direct_record_storage(*record, bridge)?,
                conversion: Conversion::CValue,
                payload: DirectPayload::Encodable,
            }),
            DirectValueType::Enum(enumeration) => Ok(Self {
                api_type: SwiftType::enumeration(*enumeration, context)?,
                storage_type: SwiftType::c_style_enum_storage(*enumeration, bridge)?,
                conversion: Conversion::CValue,
                payload: DirectPayload::CStyleEnum(Self::enum_repr(*enumeration, context)?),
            }),
            _ => Err(SwiftHost::unsupported("unknown direct value")),
        }
    }

    pub fn api_type(&self) -> &TypeName {
        &self.api_type
    }

    pub fn storage_type(&self) -> &TypeName {
        &self.storage_type
    }

    pub fn swift_value(&self, value: Expression) -> Expression {
        match self.conversion {
            Conversion::Identity => value,
            Conversion::CValue => Expression::call(
                &self.api_type,
                [Expression::labeled("fromC", value)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        }
    }

    pub fn c_value(&self, value: Expression) -> Expression {
        match self.conversion {
            Conversion::Identity => value,
            Conversion::CValue => Expression::member(value, "cValue"),
        }
    }

    pub fn default_storage_value(&self) -> Expression {
        match self.conversion {
            Conversion::Identity => Expression::new("0"),
            Conversion::CValue => Expression::call(&self.storage_type, ArgumentList::default()),
        }
    }

    pub fn converts_from_c(&self) -> bool {
        matches!(self.conversion, Conversion::CValue)
    }

    pub fn write_statement(&self, writer: Identifier, value: Expression) -> Result<Statement> {
        match self.payload {
            DirectPayload::Primitive(primitive) => {
                SwiftPrimitive::new(primitive).write_statement(writer, value)
            }
            DirectPayload::Encodable => Ok(Statement::expression(Expression::call(
                Expression::member(value, "encode"),
                [Expression::labeled("to", Expression::address(writer))]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))),
            DirectPayload::CStyleEnum(primitive) => SwiftPrimitive::new(primitive)
                .write_statement(writer, Expression::member(value, "rawValue")),
        }
    }

    fn enum_repr(enumeration: EnumId, context: &RenderContext<Native>) -> Result<Primitive> {
        match context.enumeration(enumeration) {
            Some(EnumDecl::CStyle(enumeration)) => Ok(enumeration.repr().primitive()),
            Some(_) => Err(SwiftHost::unsupported(
                "non-C-style enum where direct enum was expected",
            )),
            None => Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing enum type in Swift direct value",
            }),
        }
    }
}
