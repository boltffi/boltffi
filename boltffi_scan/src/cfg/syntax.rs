use syn::Attribute;
use syn::punctuated::Punctuated;

use super::ActiveCfg;
use super::attributes::{AttributeConfiguration, CfgLiveness};
use crate::ScanError;

#[derive(Clone)]
pub struct ConfiguredFile {
    syntax: syn::File,
}

impl ConfiguredFile {
    pub fn syntax(&self) -> &syn::File {
        &self.syntax
    }

    pub fn into_syntax(self) -> syn::File {
        self.syntax
    }

    fn new(mut syntax: syn::File, cfg: &ActiveCfg) -> Result<Self, ScanError> {
        match AttributeConfiguration::resolve(syntax.attrs, cfg)? {
            AttributeConfiguration::Active(attrs) => {
                syntax.attrs = attrs;
                syntax.items = syntax.items.configure_all(cfg)?;
            }
            AttributeConfiguration::Inactive => {
                syntax.attrs = Vec::new();
                syntax.items = Vec::new();
            }
        }
        Ok(Self { syntax })
    }
}

impl ActiveCfg {
    pub fn configure(&self, syntax: syn::File) -> Result<ConfiguredFile, ScanError> {
        ConfiguredFile::new(syntax, self)
    }
}

trait ConfigureNode: Sized {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>>;

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError>;

    fn configure(mut self, cfg: &ActiveCfg) -> Result<Option<Self>, ScanError> {
        let liveness = self
            .attrs()
            .map(|attrs| AttributeConfiguration::apply(attrs, cfg))
            .transpose()?
            .unwrap_or(CfgLiveness::Active);
        if liveness == CfgLiveness::Inactive {
            return Ok(None);
        }
        self.configure_contents(cfg)?;
        Ok(Some(self))
    }
}

trait ConfigureAll: Sized {
    fn configure_all(self, cfg: &ActiveCfg) -> Result<Self, ScanError>;
}

impl<T> ConfigureAll for Vec<T>
where
    T: ConfigureNode,
{
    fn configure_all(self, cfg: &ActiveCfg) -> Result<Self, ScanError> {
        self.into_iter()
            .map(|node| node.configure(cfg))
            .filter_map(|result| result.transpose())
            .collect()
    }
}

impl<T, P> ConfigureAll for Punctuated<T, P>
where
    T: ConfigureNode,
    P: Default,
{
    fn configure_all(self, cfg: &ActiveCfg) -> Result<Self, ScanError> {
        self.into_iter()
            .map(|node| node.configure(cfg))
            .filter_map(|result| result.transpose())
            .collect()
    }
}

impl ConfigureNode for syn::Item {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Const(item) => Some(&mut item.attrs),
            Self::Enum(item) => Some(&mut item.attrs),
            Self::ExternCrate(item) => Some(&mut item.attrs),
            Self::Fn(item) => Some(&mut item.attrs),
            Self::ForeignMod(item) => Some(&mut item.attrs),
            Self::Impl(item) => Some(&mut item.attrs),
            Self::Macro(item) => Some(&mut item.attrs),
            Self::Mod(item) => Some(&mut item.attrs),
            Self::Static(item) => Some(&mut item.attrs),
            Self::Struct(item) => Some(&mut item.attrs),
            Self::Trait(item) => Some(&mut item.attrs),
            Self::TraitAlias(item) => Some(&mut item.attrs),
            Self::Type(item) => Some(&mut item.attrs),
            Self::Union(item) => Some(&mut item.attrs),
            Self::Use(item) => Some(&mut item.attrs),
            Self::Verbatim(_) => None,
            _ => None,
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Const(item) => {
                item.generics.configure_contents(cfg)?;
                item.ty.configure_contents(cfg)
            }
            Self::Enum(item) => {
                item.generics.configure_contents(cfg)?;
                item.variants = std::mem::take(&mut item.variants).configure_all(cfg)?;
                Ok(())
            }
            Self::Fn(item) => item.sig.configure_contents(cfg),
            Self::ForeignMod(item) => {
                item.items = std::mem::take(&mut item.items).configure_all(cfg)?;
                Ok(())
            }
            Self::Impl(item) => {
                item.generics.configure_contents(cfg)?;
                item.self_ty.configure_contents(cfg)?;
                if let Some((_, path, _)) = &mut item.trait_ {
                    path.configure_contents(cfg)?;
                }
                item.items = std::mem::take(&mut item.items).configure_all(cfg)?;
                Ok(())
            }
            Self::Mod(item) => {
                if let Some((_, items)) = &mut item.content {
                    *items = std::mem::take(items).configure_all(cfg)?;
                }
                Ok(())
            }
            Self::Struct(item) => {
                item.generics.configure_contents(cfg)?;
                item.fields.configure_contents(cfg)
            }
            Self::Static(item) => item.ty.configure_contents(cfg),
            Self::Trait(item) => {
                item.generics.configure_contents(cfg)?;
                item.supertraits.configure_contents(cfg)?;
                item.items = std::mem::take(&mut item.items).configure_all(cfg)?;
                Ok(())
            }
            Self::TraitAlias(item) => {
                item.generics.configure_contents(cfg)?;
                item.bounds.configure_contents(cfg)
            }
            Self::Type(item) => {
                item.generics.configure_contents(cfg)?;
                item.ty.configure_contents(cfg)
            }
            Self::Union(item) => {
                item.generics.configure_contents(cfg)?;
                item.fields.named = std::mem::take(&mut item.fields.named).configure_all(cfg)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl ConfigureNode for syn::Variant {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        Some(&mut self.attrs)
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.fields.configure_contents(cfg)
    }
}

impl ConfigureNode for syn::Field {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        Some(&mut self.attrs)
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.ty.configure_contents(cfg)
    }
}

trait ConfigureContents {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError>;
}

impl ConfigureContents for syn::Fields {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Named(fields) => {
                fields.named = std::mem::take(&mut fields.named).configure_all(cfg)?;
            }
            Self::Unnamed(fields) => {
                fields.unnamed = std::mem::take(&mut fields.unnamed).configure_all(cfg)?;
            }
            Self::Unit => {}
        }
        Ok(())
    }
}

impl ConfigureContents for syn::Signature {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.generics.configure_contents(cfg)?;
        self.inputs = std::mem::take(&mut self.inputs).configure_all(cfg)?;
        self.variadic = self
            .variadic
            .take()
            .map(|variadic| variadic.configure(cfg))
            .transpose()?
            .flatten();
        self.output.configure_contents(cfg)
    }
}

impl ConfigureContents for syn::Generics {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.params = std::mem::take(&mut self.params).configure_all(cfg)?;
        Ok(())
    }
}

impl ConfigureContents for syn::ReturnType {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Default => Ok(()),
            Self::Type(_, ty) => ty.configure_contents(cfg),
        }
    }
}

impl ConfigureContents for syn::Type {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Array(ty) => ty.elem.configure_contents(cfg),
            Self::BareFn(ty) => {
                if let Some(lifetimes) = &mut ty.lifetimes {
                    lifetimes.lifetimes =
                        std::mem::take(&mut lifetimes.lifetimes).configure_all(cfg)?;
                }
                ty.inputs = std::mem::take(&mut ty.inputs).configure_all(cfg)?;
                ty.variadic = ty
                    .variadic
                    .take()
                    .map(|variadic| variadic.configure(cfg))
                    .transpose()?
                    .flatten();
                ty.output.configure_contents(cfg)
            }
            Self::Group(ty) => ty.elem.configure_contents(cfg),
            Self::ImplTrait(ty) => ty.bounds.configure_contents(cfg),
            Self::Paren(ty) => ty.elem.configure_contents(cfg),
            Self::Path(ty) => {
                if let Some(qself) = &mut ty.qself {
                    qself.ty.configure_contents(cfg)?;
                }
                ty.path.configure_contents(cfg)
            }
            Self::Ptr(ty) => ty.elem.configure_contents(cfg),
            Self::Reference(ty) => ty.elem.configure_contents(cfg),
            Self::Slice(ty) => ty.elem.configure_contents(cfg),
            Self::TraitObject(ty) => ty.bounds.configure_contents(cfg),
            Self::Tuple(ty) => ty
                .elems
                .iter_mut()
                .try_for_each(|element| element.configure_contents(cfg)),
            _ => Ok(()),
        }
    }
}

impl ConfigureContents for syn::Path {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.segments
            .iter_mut()
            .try_for_each(|segment| segment.arguments.configure_contents(cfg))
    }
}

impl ConfigureContents for syn::PathArguments {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::None => Ok(()),
            Self::AngleBracketed(arguments) => arguments
                .args
                .iter_mut()
                .try_for_each(|argument| argument.configure_contents(cfg)),
            Self::Parenthesized(arguments) => {
                arguments
                    .inputs
                    .iter_mut()
                    .try_for_each(|input| input.configure_contents(cfg))?;
                arguments.output.configure_contents(cfg)
            }
        }
    }
}

impl ConfigureContents for syn::GenericArgument {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Type(ty) => ty.configure_contents(cfg),
            Self::AssocType(association) => {
                if let Some(generics) = &mut association.generics {
                    generics
                        .args
                        .iter_mut()
                        .try_for_each(|argument| argument.configure_contents(cfg))?;
                }
                association.ty.configure_contents(cfg)
            }
            Self::AssocConst(association) => association
                .generics
                .iter_mut()
                .flat_map(|generics| generics.args.iter_mut())
                .try_for_each(|argument| argument.configure_contents(cfg)),
            Self::Constraint(constraint) => {
                if let Some(generics) = &mut constraint.generics {
                    generics
                        .args
                        .iter_mut()
                        .try_for_each(|argument| argument.configure_contents(cfg))?;
                }
                constraint.bounds.configure_contents(cfg)
            }
            _ => Ok(()),
        }
    }
}

impl<P> ConfigureContents for Punctuated<syn::TypeParamBound, P> {
    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.iter_mut().try_for_each(|bound| match bound {
            syn::TypeParamBound::Trait(bound) => {
                if let Some(lifetimes) = &mut bound.lifetimes {
                    lifetimes.lifetimes =
                        std::mem::take(&mut lifetimes.lifetimes).configure_all(cfg)?;
                }
                bound.path.configure_contents(cfg)
            }
            _ => Ok(()),
        })
    }
}

impl ConfigureNode for syn::BareFnArg {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        Some(&mut self.attrs)
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        self.ty.configure_contents(cfg)
    }
}

impl ConfigureNode for syn::BareVariadic {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        Some(&mut self.attrs)
    }

    fn configure_contents(&mut self, _cfg: &ActiveCfg) -> Result<(), ScanError> {
        Ok(())
    }
}

impl ConfigureNode for syn::GenericParam {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Lifetime(param) => Some(&mut param.attrs),
            Self::Type(param) => Some(&mut param.attrs),
            Self::Const(param) => Some(&mut param.attrs),
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Lifetime(_) => Ok(()),
            Self::Type(param) => {
                param.bounds.configure_contents(cfg)?;
                param
                    .default
                    .iter_mut()
                    .try_for_each(|ty| ty.configure_contents(cfg))
            }
            Self::Const(param) => param.ty.configure_contents(cfg),
        }
    }
}

impl ConfigureNode for syn::FnArg {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Receiver(receiver) => Some(&mut receiver.attrs),
            Self::Typed(argument) => Some(&mut argument.attrs),
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Receiver(receiver) => receiver.ty.configure_contents(cfg),
            Self::Typed(argument) => argument.ty.configure_contents(cfg),
        }
    }
}

impl ConfigureNode for syn::Variadic {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        Some(&mut self.attrs)
    }

    fn configure_contents(&mut self, _cfg: &ActiveCfg) -> Result<(), ScanError> {
        Ok(())
    }
}

impl ConfigureNode for syn::TraitItem {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Const(item) => Some(&mut item.attrs),
            Self::Fn(item) => Some(&mut item.attrs),
            Self::Type(item) => Some(&mut item.attrs),
            Self::Macro(item) => Some(&mut item.attrs),
            Self::Verbatim(_) => None,
            _ => None,
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Const(item) => {
                item.generics.configure_contents(cfg)?;
                item.ty.configure_contents(cfg)
            }
            Self::Fn(item) => item.sig.configure_contents(cfg),
            Self::Type(item) => {
                item.generics.configure_contents(cfg)?;
                item.bounds.configure_contents(cfg)?;
                item.default
                    .iter_mut()
                    .try_for_each(|(_, ty)| ty.configure_contents(cfg))
            }
            _ => Ok(()),
        }
    }
}

impl ConfigureNode for syn::ImplItem {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Const(item) => Some(&mut item.attrs),
            Self::Fn(item) => Some(&mut item.attrs),
            Self::Type(item) => Some(&mut item.attrs),
            Self::Macro(item) => Some(&mut item.attrs),
            Self::Verbatim(_) => None,
            _ => None,
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Const(item) => {
                item.generics.configure_contents(cfg)?;
                item.ty.configure_contents(cfg)
            }
            Self::Fn(item) => item.sig.configure_contents(cfg),
            Self::Type(item) => {
                item.generics.configure_contents(cfg)?;
                item.ty.configure_contents(cfg)
            }
            _ => Ok(()),
        }
    }
}

impl ConfigureNode for syn::ForeignItem {
    fn attrs(&mut self) -> Option<&mut Vec<Attribute>> {
        match self {
            Self::Fn(item) => Some(&mut item.attrs),
            Self::Static(item) => Some(&mut item.attrs),
            Self::Type(item) => Some(&mut item.attrs),
            Self::Macro(item) => Some(&mut item.attrs),
            Self::Verbatim(_) => None,
            _ => None,
        }
    }

    fn configure_contents(&mut self, cfg: &ActiveCfg) -> Result<(), ScanError> {
        match self {
            Self::Fn(item) => item.sig.configure_contents(cfg),
            Self::Type(item) => item.generics.configure_contents(cfg),
            Self::Static(item) => item.ty.configure_contents(cfg),
            _ => Ok(()),
        }
    }
}
