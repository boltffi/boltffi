use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Meta, Token};

use super::{ActiveCfg, CfgPredicate, PredicateMatch};
use crate::{ScanError, marker};

struct ConditionalAttributes {
    predicate: CfgPredicate,
    attrs: Vec<Meta>,
}

impl ConditionalAttributes {
    fn parse(attr: &Attribute) -> Result<Self, ScanError> {
        Self::parse_meta(&attr.meta)
    }

    fn parse_meta(meta: &Meta) -> Result<Self, ScanError> {
        let Meta::List(list) = meta else {
            return Err(ActiveCfg::invalid_attribute(meta.to_token_stream()));
        };
        syn::parse2::<Self>(list.tokens.clone())
            .map_err(|_| ActiveCfg::invalid_attribute(meta.to_token_stream()))
    }

    fn contains_marker(&self) -> Result<bool, ScanError> {
        self.attrs.iter().try_fold(false, |found, meta| {
            if found || marker::is_marker_path(meta.path()) {
                return Ok(true);
            }
            if !meta.path().is_ident("cfg_attr") {
                return Ok(false);
            }
            Self::parse_meta(meta)?.contains_marker()
        })
    }
}

impl Parse for ConditionalAttributes {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let predicate = input.parse()?;
        input.parse::<Token![,]>()?;
        let attrs = Punctuated::<Meta, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { predicate, attrs })
    }
}

pub enum AttributeConfiguration {
    Active(Vec<Attribute>),
    Inactive,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CfgLiveness {
    Active,
    Inactive,
}

impl AttributeConfiguration {
    pub fn resolve(attrs: Vec<Attribute>, cfg: &ActiveCfg) -> Result<Self, ScanError> {
        if Self::liveness(&attrs, cfg)? == PredicateMatch::Inactive {
            return Ok(Self::Inactive);
        }
        let attrs = attrs
            .into_iter()
            .map(|attr| Self::expand(attr, cfg))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if Self::liveness(&attrs, cfg)? == PredicateMatch::Inactive {
            return Ok(Self::Inactive);
        }
        Ok(Self::Active(
            attrs
                .into_iter()
                .map(|attr| {
                    if !attr.path().is_ident("cfg") {
                        return Ok(Some(attr));
                    }
                    Self::predicate(&attr).map(|predicate| {
                        (cfg.matches(&predicate) == PredicateMatch::Unknown).then_some(attr)
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        ))
    }

    pub fn apply(attrs: &mut Vec<Attribute>, cfg: &ActiveCfg) -> Result<CfgLiveness, ScanError> {
        match Self::resolve(std::mem::take(attrs), cfg)? {
            Self::Active(configured) => {
                *attrs = configured;
                Ok(CfgLiveness::Active)
            }
            Self::Inactive => {
                *attrs = Vec::new();
                Ok(CfgLiveness::Inactive)
            }
        }
    }

    fn expand(attr: Attribute, cfg: &ActiveCfg) -> Result<Vec<Attribute>, ScanError> {
        if !attr.path().is_ident("cfg_attr") {
            return Ok(vec![attr]);
        }
        let conditional = ConditionalAttributes::parse(&attr)?;
        match cfg.matches(&conditional.predicate) {
            PredicateMatch::Inactive => return Ok(Vec::new()),
            PredicateMatch::Unknown if conditional.contains_marker()? => {
                return Err(ScanError::UnresolvedConditionalMarker {
                    attribute: crate::spelling::attr(&attr),
                });
            }
            PredicateMatch::Unknown => return Ok(vec![attr]),
            PredicateMatch::Active => {}
        }
        conditional
            .attrs
            .into_iter()
            .map(|meta| {
                let mut expanded = attr.clone();
                expanded.meta = meta;
                Self::expand(expanded, cfg)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|attrs| attrs.into_iter().flatten().collect())
    }

    fn predicate(attr: &Attribute) -> Result<CfgPredicate, ScanError> {
        attr.parse_args::<CfgPredicate>()
            .map_err(|_| ActiveCfg::invalid_attribute(attr.meta.to_token_stream()))
    }

    fn liveness(attrs: &[Attribute], cfg: &ActiveCfg) -> Result<PredicateMatch, ScanError> {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .map(|attr| Self::predicate(attr).map(|predicate| cfg.matches(&predicate)))
            .try_fold(PredicateMatch::Active, |liveness, matches| {
                matches.map(|matches| liveness.and(matches))
            })
    }
}
