use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, Lit, Meta, MetaList, MetaNameValue, Path, Token};

use crate::ScanError;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ActiveCfg {
    names: BTreeSet<String>,
    values: BTreeMap<String, BTreeSet<String>>,
    features: BTreeSet<String>,
}

impl ActiveCfg {
    pub fn from_cargo_env() -> Self {
        std::env::vars().fold(Self::default(), |mut active, (name, value)| {
            active.observe_cargo_env(&name, &value);
            active
        })
    }

    pub fn with_feature(mut self, feature: impl AsRef<str>) -> Self {
        self.features.insert(Self::feature_name(feature.as_ref()));
        self
    }

    pub fn with_features(mut self, features: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.features.extend(
            features
                .into_iter()
                .map(|feature| Self::feature_name(feature.as_ref())),
        );
        self
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.names.insert(Self::cfg_name(name.as_ref()));
        self
    }

    pub fn with_value(mut self, name: impl AsRef<str>, value: impl Into<String>) -> Self {
        self.values
            .entry(Self::cfg_name(name.as_ref()))
            .or_default()
            .insert(value.into());
        self
    }

    pub fn matches_attrs(&self, attrs: &[Attribute]) -> Result<bool, ScanError> {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .map(|attr| self.matches_attr(attr))
            .try_fold(true, |active, matches| {
                matches.map(|matches| active && matches)
            })
    }

    fn observe_cargo_env(&mut self, name: &str, value: &str) {
        if let Some(feature) = name.strip_prefix("CARGO_FEATURE_") {
            self.features.insert(Self::feature_name(feature));
            return;
        }

        if let Some(name) = name.strip_prefix("CARGO_CFG_") {
            let name = Self::cfg_name(name);
            if value.is_empty() {
                self.names.insert(name);
                return;
            }

            value
                .split(',')
                .filter(|value| !value.is_empty())
                .for_each(|value| {
                    if name == "feature" {
                        self.features.insert(Self::feature_name(value));
                    }
                    self.values
                        .entry(name.clone())
                        .or_default()
                        .insert(value.to_owned());
                });
        }
    }

    fn matches_attr(&self, attr: &Attribute) -> Result<bool, ScanError> {
        attr.parse_args::<Meta>()
            .map_err(|_| Self::invalid_attribute(attr.meta.to_token_stream()))
            .and_then(|meta| self.matches_meta(&meta))
    }

    fn matches_meta(&self, meta: &Meta) -> Result<bool, ScanError> {
        match meta {
            Meta::Path(path) => Ok(self.matches_name(path)),
            Meta::NameValue(value) => self.matches_value(value),
            Meta::List(list) if list.path.is_ident("all") => self.matches_all(list),
            Meta::List(list) if list.path.is_ident("any") => self.matches_any(list),
            Meta::List(list) if list.path.is_ident("not") => self.matches_not(list),
            Meta::List(list) => Err(Self::invalid_attribute(list.to_token_stream())),
        }
    }

    fn matches_name(&self, path: &Path) -> bool {
        path.get_ident()
            .map(|ident| self.names.contains(&Self::cfg_name(&ident.to_string())))
            .unwrap_or(false)
    }

    fn matches_value(&self, value: &MetaNameValue) -> Result<bool, ScanError> {
        let Some(name) = value.path.get_ident().map(ToString::to_string) else {
            return Ok(false);
        };
        let Some(value) = Self::string_value(&value.value) else {
            return Err(Self::invalid_attribute(value.to_token_stream()));
        };

        if name == "feature" {
            return Ok(self.features.contains(&Self::feature_name(&value)));
        }

        Ok(self
            .values
            .get(&Self::cfg_name(&name))
            .is_some_and(|values| values.contains(&value)))
    }

    fn matches_all(&self, list: &MetaList) -> Result<bool, ScanError> {
        self.predicates(list)?
            .iter()
            .map(|meta| self.matches_meta(meta))
            .try_fold(true, |active, matches| {
                matches.map(|matches| active && matches)
            })
    }

    fn matches_any(&self, list: &MetaList) -> Result<bool, ScanError> {
        self.predicates(list)?
            .iter()
            .map(|meta| self.matches_meta(meta))
            .try_fold(false, |active, matches| {
                matches.map(|matches| active || matches)
            })
    }

    fn matches_not(&self, list: &MetaList) -> Result<bool, ScanError> {
        let predicates = self.predicates(list)?;
        match predicates.len() {
            1 => self.matches_meta(&predicates[0]).map(|active| !active),
            _ => Err(Self::invalid_attribute(list.to_token_stream())),
        }
    }

    fn predicates(&self, list: &MetaList) -> Result<Vec<Meta>, ScanError> {
        Punctuated::<Meta, Token![,]>::parse_terminated
            .parse2(list.tokens.clone())
            .map(|items| items.into_iter().collect())
            .map_err(|_| Self::invalid_attribute(list.to_token_stream()))
    }

    fn string_value(value: &Expr) -> Option<String> {
        match value {
            Expr::Lit(value) => match &value.lit {
                Lit::Str(value) => Some(value.value()),
                _ => None,
            },
            _ => None,
        }
    }

    fn feature_name(feature: &str) -> String {
        feature.replace('-', "_").to_ascii_uppercase()
    }

    fn cfg_name(name: &str) -> String {
        name.to_ascii_lowercase()
    }

    fn invalid_attribute(tokens: TokenStream) -> ScanError {
        ScanError::InvalidAttribute {
            attribute: tokens.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ActiveCfg;

    fn matches(active: &ActiveCfg, source: &str) -> bool {
        let source = format!("{source} struct Demo;");
        let file = syn::parse_str::<syn::File>(&source).expect("valid item");
        let syn::Item::Struct(item) = &file.items[0] else {
            panic!("expected struct");
        };
        active.matches_attrs(&item.attrs).expect("cfg evaluation")
    }

    #[test]
    fn feature_cfg_uses_cargo_feature_normalization() {
        let active = ActiveCfg::default().with_feature("native_ffi");

        assert!(matches(&active, "#[cfg(feature = \"native-ffi\")]"));
    }

    #[test]
    fn cfg_predicates_match_active_names_and_values() {
        let active = ActiveCfg::default()
            .with_name("unix")
            .with_value("target_os", "ios");

        assert!(matches(
            &active,
            "#[cfg(all(unix, any(target_os = \"ios\", target_os = \"macos\")))]"
        ));
        assert!(!matches(&active, "#[cfg(not(unix))]"));
    }
}
