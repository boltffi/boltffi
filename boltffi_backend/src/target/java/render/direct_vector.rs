use boltffi_binding::{DirectVectorElementType, DirectVectorPrimitive, Native, RecordId};

use crate::{
    core::{RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        primitive::Primitive,
        render::Record,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeIdentifier, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectVector {
    ty: TypeName,
    codec: Codec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Codec {
    Primitive(Primitive),
    Record { ty: TypeName, size: u64 },
}

impl DirectVector {
    pub fn from_element(
        element: &DirectVectorElementType,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Self::from_primitive(*primitive),
            DirectVectorElementType::Record(record) => Self::from_record(*record, version, context),
            _ => Err(JavaHost::unsupported("unknown direct vector element")),
        }
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn parameter_jvm_type(&self) -> TypeName {
        match &self.codec {
            Codec::Primitive(primitive) => TypeName::array(TypeName::primitive(*primitive)),
            Codec::Record { .. } => Self::byte_array(),
        }
    }

    pub fn native_argument(&self, value: Expression) -> Expression {
        match &self.codec {
            Codec::Primitive(_) => value,
            Codec::Record { size, .. } => Self::codec_call(
                "writeRecords",
                [
                    value,
                    Expression::integer(*size),
                    Expression::lambda_statement(
                        [
                            Identifier::known("item"),
                            Identifier::known("buffer"),
                            Identifier::known("offset"),
                        ],
                        Statement::expression(
                            Expression::identifier(Identifier::known("item")).call(
                                Identifier::known("writeToDirectBuffer"),
                                [
                                    Expression::identifier(Identifier::known("buffer")),
                                    Expression::identifier(Identifier::known("offset")),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                    ),
                ],
            ),
        }
    }

    pub fn decoded_argument(&self, value: Expression) -> Expression {
        match &self.codec {
            Codec::Primitive(_) => value,
            Codec::Record { ty, size } => Self::codec_call(
                "readRecords",
                [
                    value,
                    Expression::integer(*size),
                    Expression::lambda(
                        [Identifier::known("buffer"), Identifier::known("offset")],
                        Expression::static_call(
                            ty.clone(),
                            Identifier::known("fromDirectBuffer"),
                            [
                                Expression::identifier(Identifier::known("buffer")),
                                Expression::identifier(Identifier::known("offset")),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    ),
                ],
            ),
        }
    }

    pub fn returned_expression(&self, value: Expression) -> Expression {
        match self.codec {
            Codec::Primitive(primitive) => {
                Self::codec_call(Self::primitive_reader(primitive), [value])
            }
            Codec::Record { .. } => self.decoded_argument(value),
        }
    }

    pub fn callback_return_expression(&self, value: Expression) -> Expression {
        match self.codec {
            Codec::Primitive(primitive) => {
                Self::codec_call(Self::primitive_writer(primitive), [value])
            }
            Codec::Record { .. } => self.native_argument(value),
        }
    }

    fn from_primitive(primitive: DirectVectorPrimitive) -> Result<Self> {
        let primitive = Primitive::try_from(primitive.primitive())?;
        Ok(Self {
            ty: TypeName::array(TypeName::primitive(primitive)),
            codec: Codec::Primitive(primitive),
        })
    }

    fn from_record(
        record: RecordId,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let size = Record::direct_size_for(record, context)?;
        let record = TypeName::named(Record::type_name_for(record, context, version)?);
        Ok(Self {
            ty: TypeName::parameterized(
                TypeName::qualified(
                    ["java", "util"]
                        .into_iter()
                        .map(Identifier::known)
                        .collect(),
                    TypeIdentifier::known("List", version),
                ),
                [record.clone()],
            ),
            codec: Codec::Record { ty: record, size },
        })
    }

    fn codec_call(
        method: &'static str,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Expression {
        Expression::static_call(
            TypeName::named(TypeIdentifier::known(
                "DirectVectorCodec",
                JavaVersion::JAVA_8,
            )),
            Identifier::known(method),
            arguments.into_iter().collect::<ArgumentList>(),
        )
    }

    fn primitive_reader(primitive: Primitive) -> &'static str {
        match primitive {
            Primitive::Boolean => "readBooleanArray",
            Primitive::Byte => "readByteArray",
            Primitive::Short => "readShortArray",
            Primitive::Int => "readIntArray",
            Primitive::Long => "readLongArray",
            Primitive::Float => "readFloatArray",
            Primitive::Double => "readDoubleArray",
        }
    }

    fn primitive_writer(primitive: Primitive) -> &'static str {
        match primitive {
            Primitive::Boolean => "writeBooleanArray",
            Primitive::Byte => "writeByteArray",
            Primitive::Short => "writeShortArray",
            Primitive::Int => "writeIntArray",
            Primitive::Long => "writeLongArray",
            Primitive::Float => "writeFloatArray",
            Primitive::Double => "writeDoubleArray",
        }
    }

    fn byte_array() -> TypeName {
        TypeName::array(TypeName::primitive(Primitive::Byte))
    }
}
