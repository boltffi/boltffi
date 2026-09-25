use crate::{
    core::{Error, Result},
    target::kotlin::syntax::{Identifier, TypeName},
};

/// Rejects members that would duplicate a generated declaration such as
/// `close()`. Only zero-parameter members collide; overloads are legal Kotlin.
pub fn validate_reserved_members<'a>(
    scope: &TypeName,
    reserved: &[&str],
    zero_parameter_members: impl IntoIterator<Item = &'a Identifier>,
) -> Result<()> {
    zero_parameter_members
        .into_iter()
        .find(|name| reserved.contains(&name.as_str()))
        .map_or(Ok(()), |name| {
            Err(Error::KotlinNameCollision {
                scope: scope.to_string(),
                name: format!("{name}()"),
            })
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
}

impl Parameter {
    pub fn new(name: Identifier, ty: TypeName) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }
}

pub fn validate_exception_fields<'a>(
    scope: &TypeName,
    fields: impl IntoIterator<Item = (&'a Identifier, &'a TypeName)>,
) -> Result<Option<Identifier>> {
    fields.into_iter().try_fold(None, |message, (name, ty)| {
        match (name.as_str(), ty.is_string()) {
            ("message", true) => Ok(Some(name.clone())),
            ("message" | "cause", _) | ("localizedMessage", true) => {
                Err(Error::KotlinNameCollision {
                    scope: scope.to_string(),
                    name: name.to_string(),
                })
            }
            _ => Ok(message),
        }
    })
}
