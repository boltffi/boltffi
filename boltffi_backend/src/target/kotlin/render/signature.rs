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
