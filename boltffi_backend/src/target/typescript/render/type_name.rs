use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, EnumId, Primitive, RecordId, TypeRef,
    TypeRefRender, Wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{name_style::Name, primitive::Scalar, syntax::TypeName};

pub struct Type;

pub struct RenderedType {
    name: TypeName,
    scalar: Option<Primitive>,
}

impl Type {
    pub fn from_ref(ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<TypeName> {
        ty.render_with(&mut TypeRefRenderer { context })
            .map(RenderedType::into_name)
    }

    pub fn primitive(primitive: Primitive) -> Result<TypeName> {
        Scalar::new(primitive).map(Scalar::ty)
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}

impl RenderedType {
    fn new(name: TypeName) -> Self {
        Self { name, scalar: None }
    }

    fn scalar(primitive: Primitive, name: TypeName) -> Self {
        Self {
            name,
            scalar: Some(primitive),
        }
    }

    fn into_name(self) -> TypeName {
        self.name
    }
}

struct TypeRefRenderer<'context> {
    context: &'context RenderContext<'context, Wasm32>,
}

impl TypeRefRender for TypeRefRenderer<'_> {
    type Output = Result<RenderedType>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        Type::primitive(primitive).map(|name| RenderedType::scalar(primitive, name))
    }

    fn string(&mut self) -> Self::Output {
        Ok(RenderedType::new(TypeName::string()))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        // TypeScript does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString type reached TypeScript renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(RenderedType::new(TypeName::named("Uint8Array")))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        self.context
            .record(id)
            .map(|record| RenderedType::new(Name::new(record.name()).type_name()))
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "record without declaration",
            })
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        self.context
            .enumeration(id)
            .map(|enumeration| RenderedType::new(Name::new(enumeration.name()).type_name()))
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "enum without declaration",
            })
    }

    fn class(&mut self, id: ClassId) -> Self::Output {
        self.context
            .class(id)
            .map(|class| RenderedType::new(Name::new(class.name()).type_name()))
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "class without declaration",
            })
    }

    fn callback(&mut self, _id: CallbackId) -> Self::Output {
        Type::unsupported("callback type")
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        let declaration = self
            .context
            .custom_type(id)
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "custom type without declaration",
            })?;
        declaration
            .representation()
            .render_with(self)
            .map(|_| RenderedType::new(Name::new(declaration.name()).type_name()))
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(RenderedType::new(match kind {
            BuiltinType::Duration => TypeName::named("Duration"),
            BuiltinType::SystemTime => TypeName::named("Date"),
            BuiltinType::Uuid | BuiltinType::Url => TypeName::string(),
        }))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        inner.map(|inner| RenderedType::new(inner.name.nullable()))
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        let element = element?;
        let array = TypeName::generic("Array", [element.name]);
        match element.scalar {
            Some(primitive) => Ok(RenderedType::new(
                match Scalar::new(primitive)?.typed_array() {
                    Some(typed_array) => TypeName::union(array, typed_array),
                    None => array,
                },
            )),
            None => Ok(RenderedType::new(array)),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output {
        elements
            .into_iter()
            .map(|element| element.map(RenderedType::into_name))
            .collect::<Result<Vec<_>>>()
            .map(TypeName::tuple)
            .map(RenderedType::new)
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        let ok = ok?.into_name();
        let err = err?.into_name();
        Ok(RenderedType::new(TypeName::union(
            ok.clone(),
            TypeName::union(
                TypeName::generic("WireResult", [ok, err]),
                TypeName::named("Error"),
            ),
        )))
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        Ok(RenderedType::new(TypeName::generic(
            "Map",
            [key?.into_name(), value?.into_name()],
        )))
    }
}
