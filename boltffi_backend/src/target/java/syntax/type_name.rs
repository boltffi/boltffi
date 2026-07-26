use std::fmt;

use crate::{
    core::syntax::sealed,
    target::java::{JavaVersion, primitive::Primitive},
};

use super::{Identifier, TypeIdentifier};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(TypeShape);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TypeShape {
    Named {
        package: Vec<Identifier>,
        name: TypeIdentifier,
    },
    Primitive(Primitive),
    Array(Box<TypeName>),
    Nested {
        owner: Box<TypeName>,
        name: TypeIdentifier,
    },
    Parameterized {
        raw: Box<TypeName>,
        arguments: Vec<TypeName>,
    },
}

impl TypeName {
    pub fn named(name: TypeIdentifier) -> Self {
        Self(TypeShape::Named {
            package: Vec::new(),
            name,
        })
    }

    pub fn qualified(package: Vec<Identifier>, name: TypeIdentifier) -> Self {
        Self(TypeShape::Named { package, name })
    }

    pub fn primitive(primitive: Primitive) -> Self {
        Self(TypeShape::Primitive(primitive))
    }

    pub fn array(element: Self) -> Self {
        Self(TypeShape::Array(Box::new(element)))
    }

    pub fn nested(owner: Self, name: TypeIdentifier) -> Self {
        Self(TypeShape::Nested {
            owner: Box::new(owner),
            name,
        })
    }

    pub fn parameterized(raw: Self, arguments: impl IntoIterator<Item = Self>) -> Self {
        Self(TypeShape::Parameterized {
            raw: Box::new(raw),
            arguments: arguments.into_iter().collect(),
        })
    }

    pub fn boxed_primitive(primitive: Primitive, version: JavaVersion) -> Self {
        Self::named(TypeIdentifier::known(
            match primitive {
                Primitive::Boolean => "Boolean",
                Primitive::Byte => "Byte",
                Primitive::Short => "Short",
                Primitive::Int => "Integer",
                Primitive::Long => "Long",
                Primitive::Float => "Float",
                Primitive::Double => "Double",
            },
            version,
        ))
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TypeShape::Named { package, name } => {
                package
                    .iter()
                    .try_for_each(|component| write!(formatter, "{component}."))?;
                name.fmt(formatter)
            }
            TypeShape::Primitive(primitive) => primitive.fmt(formatter),
            TypeShape::Array(element) => write!(formatter, "{element}[]"),
            TypeShape::Nested { owner, name } => write!(formatter, "{owner}.{name}"),
            TypeShape::Parameterized { raw, arguments } => write!(
                formatter,
                "{raw}<{}>",
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl sealed::SyntaxFragment for TypeName {}

#[cfg(test)]
mod tests {
    use crate::target::java::{
        JavaVersion,
        primitive::Primitive,
        syntax::{Identifier, TypeIdentifier},
    };

    use super::TypeName;

    #[test]
    fn derives_reference_spelling_from_the_identifier() {
        let reference = TypeName::named(TypeIdentifier::known("Object", JavaVersion::JAVA_8));

        assert_eq!(reference.to_string(), "Object");
    }

    #[test]
    fn renders_qualified_reference_spelling() {
        let reference = TypeName::qualified(
            ["java", "nio"].into_iter().map(Identifier::known).collect(),
            TypeIdentifier::known("ByteBuffer", JavaVersion::JAVA_8),
        );

        assert_eq!(reference.to_string(), "java.nio.ByteBuffer");
    }

    #[test]
    fn renders_parameterized_and_array_types() {
        let list = TypeName::qualified(
            ["java", "util"]
                .into_iter()
                .map(Identifier::known)
                .collect(),
            TypeIdentifier::known("List", JavaVersion::JAVA_8),
        );
        let integer = TypeName::boxed_primitive(Primitive::Int, JavaVersion::JAVA_8);

        assert_eq!(
            TypeName::parameterized(list, [integer]).to_string(),
            "java.util.List<Integer>"
        );
        assert_eq!(
            TypeName::array(TypeName::primitive(Primitive::Double)).to_string(),
            "double[]"
        );
    }
}
