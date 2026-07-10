mod attributes;
mod rustc_args;
mod syntax;

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Path, Token, parenthesized};

pub use rustc_args::{RustcCfgArgs, RustcCfgArgsError};
pub use syntax::ConfiguredFile;

use crate::ScanError;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ActiveCfg {
    names: BTreeSet<String>,
    values: BTreeMap<String, BTreeSet<String>>,
    features: BTreeSet<String>,
    legacy_names: BTreeSet<String>,
    legacy_values: BTreeMap<String, BTreeSet<String>>,
    legacy_features: BTreeSet<String>,
}

impl ActiveCfg {
    pub fn from_cargo_env() -> Self {
        std::env::vars().fold(Self::default(), |mut active, (name, value)| {
            active.observe_cargo_env(&name, &value);
            active
        })
    }

    pub fn with_feature(mut self, feature: impl AsRef<str>) -> Self {
        self.features.insert(feature.as_ref().to_owned());
        self
    }

    pub fn with_features(mut self, features: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.features.extend(
            features
                .into_iter()
                .map(|feature| feature.as_ref().to_owned()),
        );
        self
    }

    pub fn for_features(&self, features: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            names: self.names.clone(),
            values: self.values.clone(),
            legacy_names: self.legacy_names.clone(),
            legacy_values: self.legacy_values.clone(),
            features: features
                .into_iter()
                .map(|feature| feature.as_ref().to_owned())
                .collect(),
            legacy_features: BTreeSet::new(),
        }
    }

    pub fn without_features(&self) -> Self {
        self.for_features(std::iter::empty::<&str>())
    }

    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.names.insert(name.as_ref().to_owned());
        self
    }

    pub fn with_value(mut self, name: impl AsRef<str>, value: impl Into<String>) -> Self {
        self.values
            .entry(name.as_ref().to_owned())
            .or_default()
            .insert(value.into());
        self
    }

    fn observe_cargo_env(&mut self, name: &str, value: &str) {
        if let Some(feature) = name.strip_prefix("CARGO_FEATURE_") {
            self.legacy_features
                .insert(Self::legacy_feature_name(feature));
            return;
        }

        if let Some(name) = name.strip_prefix("CARGO_CFG_") {
            let name = Self::legacy_cfg_name(name);
            if value.is_empty() {
                self.legacy_names.insert(name);
                return;
            }

            value
                .split(',')
                .filter(|value| !value.is_empty())
                .for_each(|value| {
                    if name == "FEATURE" {
                        self.features.insert(value.to_owned());
                    }
                    self.legacy_values
                        .entry(name.clone())
                        .or_default()
                        .insert(value.to_owned());
                });
        }
    }

    fn matches(&self, predicate: &CfgPredicate) -> PredicateMatch {
        match predicate {
            CfgPredicate::True => PredicateMatch::Active,
            CfgPredicate::False => PredicateMatch::Inactive,
            CfgPredicate::Name(name)
                if self.names.contains(name)
                    || self.legacy_names.contains(&Self::legacy_cfg_name(name)) =>
            {
                PredicateMatch::Active
            }
            CfgPredicate::Name(_) => PredicateMatch::Unknown,
            CfgPredicate::Value { name, value } if name == "feature" => PredicateMatch::from_bool(
                self.features.contains(value)
                    || self
                        .legacy_features
                        .contains(&Self::legacy_feature_name(value)),
            ),
            CfgPredicate::Value { name, value } => self
                .values
                .get(name)
                .or_else(|| self.legacy_values.get(&Self::legacy_cfg_name(name)))
                .map_or(PredicateMatch::Unknown, |values| {
                    PredicateMatch::from_bool(values.contains(value))
                }),
            CfgPredicate::All(predicates) => predicates
                .iter()
                .map(|predicate| self.matches(predicate))
                .fold(PredicateMatch::Active, PredicateMatch::and),
            CfgPredicate::Any(predicates) => predicates
                .iter()
                .map(|predicate| self.matches(predicate))
                .fold(PredicateMatch::Inactive, PredicateMatch::or),
            CfgPredicate::Not(predicate) => self.matches(predicate).not(),
        }
    }

    fn legacy_feature_name(feature: &str) -> String {
        feature.replace('-', "_").to_ascii_uppercase()
    }

    fn legacy_cfg_name(name: &str) -> String {
        name.to_ascii_uppercase()
    }

    fn invalid_attribute(tokens: TokenStream) -> ScanError {
        ScanError::InvalidAttribute {
            attribute: tokens.to_string(),
        }
    }
}

enum CfgPredicate {
    True,
    False,
    Name(String),
    Value { name: String, value: String },
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PredicateMatch {
    Active,
    Inactive,
    Unknown,
}

impl PredicateMatch {
    fn from_bool(value: bool) -> Self {
        if value { Self::Active } else { Self::Inactive }
    }

    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Inactive, _) | (_, Self::Inactive) => Self::Inactive,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Active, Self::Active) => Self::Active,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Active, _) | (_, Self::Active) => Self::Active,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Inactive, Self::Inactive) => Self::Inactive,
        }
    }

    fn not(self) -> Self {
        match self {
            Self::Active => Self::Inactive,
            Self::Inactive => Self::Active,
            Self::Unknown => Self::Unknown,
        }
    }
}

impl CfgPredicate {
    fn name(path: &Path) -> Option<String> {
        path.get_ident().map(ToString::to_string)
    }
}

impl Parse for CfgPredicate {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(syn::LitBool) {
            return input
                .parse::<syn::LitBool>()
                .map(|value| if value.value { Self::True } else { Self::False });
        }

        let path = input.parse::<Path>()?;
        let name =
            Self::name(&path).ok_or_else(|| input.error("cfg name must be an identifier"))?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            return input.parse::<syn::LitStr>().map(|value| Self::Value {
                name,
                value: value.value(),
            });
        }
        if !input.peek(syn::token::Paren) {
            return Ok(Self::Name(name));
        }

        let content;
        parenthesized!(content in input);
        let mut predicates = Punctuated::<Self, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect::<Vec<_>>();
        match name.as_str() {
            "all" => Ok(Self::All(predicates)),
            "any" => Ok(Self::Any(predicates)),
            "not" if predicates.len() == 1 => Ok(Self::Not(Box::new(predicates.remove(0)))),
            _ => Err(input.error("unsupported cfg predicate")),
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;

    use super::ActiveCfg;

    fn matches(active: &ActiveCfg, source: &str) -> bool {
        let source = format!("{source} struct Demo;");
        active
            .configure(syn::parse_str(&source).expect("valid item"))
            .expect("cfg evaluation")
            .syntax()
            .items
            .iter()
            .any(|item| matches!(item, syn::Item::Struct(_)))
    }

    fn configure(active: &ActiveCfg, source: &str) -> String {
        active
            .configure(syn::parse_str(source).expect("valid source"))
            .expect("cfg configuration")
            .syntax()
            .to_token_stream()
            .to_string()
    }

    #[test]
    fn feature_cfg_preserves_exact_feature_identity() {
        let hyphenated = ActiveCfg::default().with_feature("native-ffi");
        let underscored = ActiveCfg::default().with_feature("native_ffi");

        assert!(matches(&hyphenated, "#[cfg(feature = \"native-ffi\")]"));
        assert!(!matches(&hyphenated, "#[cfg(feature = \"native_ffi\")]"));
        assert!(matches(&underscored, "#[cfg(feature = \"native_ffi\")]"));
        assert!(!matches(&underscored, "#[cfg(feature = \"native-ffi\")]"));
    }

    #[test]
    fn authoritative_features_clear_lossy_environment_fallback() {
        let mut cargo = ActiveCfg::default();
        cargo.observe_cargo_env("CARGO_FEATURE_NATIVE_FFI", "1");

        assert!(matches(&cargo, "#[cfg(feature = \"native-ffi\")]"));
        assert!(matches(&cargo, "#[cfg(feature = \"native_ffi\")]"));

        let exact = cargo.for_features(["native-ffi"]);

        assert!(matches(&exact, "#[cfg(feature = \"native-ffi\")]"));
        assert!(!matches(&exact, "#[cfg(feature = \"native_ffi\")]"));
    }

    #[test]
    fn custom_cfg_names_preserve_exact_case() {
        let configured = configure(
            &ActiveCfg::default().with_name("Foo"),
            "#[cfg(Foo)] struct Upper; #[cfg(foo)] struct Lower;",
        );

        assert_eq!(configured, "struct Upper ; # [cfg (foo)] struct Lower ;");
    }

    #[test]
    fn cargo_cfg_name_fallback_is_explicitly_lossy() {
        let mut cargo = ActiveCfg::default();
        cargo.observe_cargo_env("CARGO_CFG_FOO", "");

        assert_eq!(
            configure(
                &cargo,
                "#[cfg(Foo)] struct Upper; #[cfg(foo)] struct Lower;"
            ),
            "struct Upper ; struct Lower ;"
        );
    }

    #[test]
    fn package_features_replace_root_features_without_losing_target_cfg() {
        let root = ActiveCfg::default()
            .with_feature("root")
            .with_name("unix")
            .with_value("target_os", "ios");
        let dependency = root.for_features(["dependency"]);

        assert!(!matches(&dependency, "#[cfg(feature = \"root\")]"));
        assert!(matches(&dependency, "#[cfg(feature = \"dependency\")]"));
        assert!(matches(
            &dependency,
            "#[cfg(all(unix, target_os = \"ios\"))]"
        ));
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
        assert!(matches(&active, "#[cfg(true)]"));
        assert!(!matches(&active, "#[cfg(false)]"));
    }

    #[test]
    fn cfg_attr_expands_multiple_attributes_in_source_order() {
        let configured = configure(
            &ActiveCfg::default().with_feature("ffi"),
            "#[first] #[cfg_attr(feature = \"ffi\", boltffi::data, deprecated)] #[last] struct Signal;",
        );

        assert_eq!(
            configured,
            "# [first] # [boltffi :: data] # [deprecated] # [last] struct Signal ;"
        );
    }

    #[test]
    fn nested_cfg_attr_is_resolved_recursively() {
        let active = ActiveCfg::default().with_feature("ffi").with_name("unix");
        let configured = configure(
            &active,
            "#[cfg_attr(feature = \"ffi\", cfg_attr(unix, boltffi::data))] struct Signal;",
        );

        assert_eq!(configured, "# [boltffi :: data] struct Signal ;");
    }

    #[test]
    fn cfg_created_by_cfg_attr_controls_liveness() {
        let configured = configure(
            &ActiveCfg::default()
                .with_feature("ffi")
                .with_value("target_os", "macos"),
            "#[cfg_attr(feature = \"ffi\", cfg(target_os = \"ios\"))] struct Hidden; struct Visible;",
        );

        assert_eq!(configured, "struct Visible ;");
    }

    #[test]
    fn unobserved_non_feature_cfg_remains_unresolved() {
        let configured = configure(
            &ActiveCfg::default(),
            "#[cfg(target_os = \"ios\")] struct Conditional;",
        );

        assert_eq!(
            configured,
            "# [cfg (target_os = \"ios\")] struct Conditional ;"
        );
    }

    #[test]
    fn unobserved_non_marker_cfg_attr_remains_unresolved() {
        let configured = configure(
            &ActiveCfg::default(),
            "#[cfg_attr(target_os = \"ios\", deprecated)] struct Conditional;",
        );

        assert_eq!(
            configured,
            "# [cfg_attr (target_os = \"ios\" , deprecated)] struct Conditional ;"
        );
    }

    #[test]
    fn unobserved_conditional_marker_is_rejected_locally() {
        let source = syn::parse_str(
            "#[cfg_attr(all(feature = \"ffi\", target_os = \"ios\"), boltffi::data)] struct Signal;",
        )
        .expect("valid source");
        let result = ActiveCfg::default().with_feature("ffi").configure(source);
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("conditional marker needs complete cfg"),
        };

        assert_eq!(
            error,
            crate::ScanError::UnresolvedConditionalMarker {
                attribute:
                    "cfg_attr(all (feature = \"ffi\" , target_os = \"ios\") , boltffi :: data)"
                        .to_owned()
            }
        );
    }

    #[test]
    fn inactive_cfg_prunes_before_unresolved_conditional_marker() {
        let configured = configure(
            &ActiveCfg::default(),
            "#[cfg(any())] #[cfg_attr(target_os = \"ios\", boltffi::data)] struct Never; struct Always;",
        );

        assert_eq!(configured, "struct Always ;");
    }

    #[test]
    fn configuration_prunes_inactive_members_and_parameters() {
        let configured = configure(
            &ActiveCfg::default().with_feature("ffi"),
            "enum Signal { #[cfg(false)] Hidden, #[cfg_attr(feature = \"ffi\", deprecated)] Ping } \
             struct Packet { #[cfg(false)] hidden: i32, value: i32 } \
             impl Packet { #[cfg(false)] fn hidden() {} fn send(#[cfg(false)] ignored: i32, value: i32) {} } \
             type Callback = fn(#[cfg(false)] i32, bool);",
        );

        assert!(!configured.contains("Hidden"));
        assert!(!configured.contains("hidden"));
        assert!(!configured.contains("ignored"));
        assert!(configured.contains("# [deprecated] Ping"));
        assert!(configured.contains("fn send (value : i32)"));
        assert!(configured.contains("type Callback = fn (bool)"));
    }
}
