use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, EnumId, Native, Primitive, RecordId, TypeRef,
    TypeRefRender,
};

use crate::core::{Error, RenderContext, Result};

use super::{
    name_style::{Name, Namespace},
    render::primitive_type,
    syntax::TypeFragment,
};

pub(super) fn type_ref(ty: &TypeRef, context: &RenderContext<Native>) -> Result<TypeFragment> {
    ty.render_with(&mut Renderer {
        context,
        namespace: None,
    })
}

pub(super) fn type_ref_qualified(
    ty: &TypeRef,
    namespace: &Namespace,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    ty.render_with(&mut Renderer {
        context,
        namespace: Some(namespace),
    })
}

pub(super) fn record(id: RecordId, context: &RenderContext<Native>) -> Result<TypeFragment> {
    context
        .record(id)
        .map(|record| Name::new(record.name()).pascal())
        .transpose()?
        .map(|name| TypeFragment::new(name.to_string()))
        .ok_or(Error::UnexpectedBindingShape {
            layer: "csharp type",
            shape: "missing record declaration",
        })
}

pub(super) fn enumeration(id: EnumId, context: &RenderContext<Native>) -> Result<TypeFragment> {
    context
        .enumeration(id)
        .map(|enumeration| Name::new(enumeration.name()).pascal())
        .transpose()?
        .map(|name| TypeFragment::new(name.to_string()))
        .ok_or(Error::UnexpectedBindingShape {
            layer: "csharp type",
            shape: "missing enum declaration",
        })
}

pub(super) fn class(id: ClassId, context: &RenderContext<Native>) -> Result<TypeFragment> {
    context
        .class(id)
        .map(|class| Name::new(class.name()).pascal())
        .transpose()?
        .map(|name| TypeFragment::new(name.to_string()))
        .ok_or(Error::UnexpectedBindingShape {
            layer: "csharp type",
            shape: "missing class declaration",
        })
}

pub(super) fn callback(id: CallbackId, context: &RenderContext<Native>) -> Result<TypeFragment> {
    context
        .callback(id)
        .map(|callback| Name::new(callback.name()).pascal())
        .transpose()?
        .map(|name| TypeFragment::new(name.to_string()))
        .ok_or(Error::UnexpectedBindingShape {
            layer: "csharp type",
            shape: "missing callback declaration",
        })
}

struct Renderer<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
    namespace: Option<&'context Namespace>,
}

impl TypeRefRender for Renderer<'_, '_> {
    type Output = Result<TypeFragment>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        Ok(primitive_type(primitive))
    }

    fn string(&mut self) -> Self::Output {
        Ok(TypeFragment::new("string"))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        unreachable!(
            "InternedString type ref reached C# renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(TypeFragment::new("byte[]"))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        qualify(record(id, self.context)?, self.namespace)
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        qualify(enumeration(id, self.context)?, self.namespace)
    }

    fn class(&mut self, id: ClassId) -> Self::Output {
        qualify(class(id, self.context)?, self.namespace)
    }

    fn callback(&mut self, id: CallbackId) -> Self::Output {
        qualify(callback(id, self.context)?, self.namespace)
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        match self.context.custom_type_mapping(id) {
            Some(mapping) => Ok(TypeFragment::new(mapping.target_type().as_str())),
            None => self
                .context
                .custom_type(id)
                .ok_or(Error::UnexpectedBindingShape {
                    layer: "csharp type",
                    shape: "missing custom type declaration",
                })?
                .representation()
                .render_with(self),
        }
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(TypeFragment::new(match kind {
            BuiltinType::Duration => "global::System.TimeSpan",
            BuiltinType::SystemTime => "global::System.DateTime",
            BuiltinType::Uuid => "global::System.Guid",
            BuiltinType::Url => "global::System.Uri",
        }))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        let inner = inner?;
        if inner.to_string().ends_with('?') {
            return super::unsupported("nested optional type");
        }
        Ok(TypeFragment::new(format!("{inner}?")))
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        Ok(TypeFragment::new(format!("{}[]", element?)))
    }

    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output {
        Ok(TypeFragment::new(format!(
            "({})",
            elements
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        Ok(TypeFragment::new(format!(
            "BoltFFIResult<{}, {}>",
            ok?, err?
        )))
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        Ok(TypeFragment::new(format!(
            "global::System.Collections.Generic.Dictionary<{}, {}>",
            key?, value?
        )))
    }
}

fn qualify(ty: TypeFragment, namespace: Option<&Namespace>) -> Result<TypeFragment> {
    Ok(namespace.map_or(ty.clone(), |namespace| {
        TypeFragment::new(format!("global::{namespace}.{ty}"))
    }))
}
