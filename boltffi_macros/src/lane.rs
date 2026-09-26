//! Declaration lanes: how one macro invocation learns the declarations it refers to.
//!
//! Every type BoltFFI declares also defines a hidden `macro_rules!` under the type's own
//! name. Wherever a path names the type, the same path names its lane, so the compiler
//! resolves both together. A lane hands back the type's source fragment plus the
//! fragments it refers to, already resolved at the type's own site. An invocation
//! gathers the lanes its signature names, then aggregates, lowers, and expands exactly
//! as bindgen does with the same fragments.
//!
//! `docs/contributors/per-invocation-expansion.md` walks one crate through every step.

use std::collections::HashSet;

use boltffi_ast::PackageInfo;
use boltffi_binding::{
    Native, RawSourceRecord, SELF_ID, SourceFragment, SurfaceLower, Wasm32, aggregate_invocation,
    lower_invocation,
};
use boltffi_scan::SlotSource;
use proc_macro2::{Delimiter, Literal, Span, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::capture::{self, Fragments, ImplCapture};
use crate::expansion::{contract::Expansion, expander::Expander};

/// Numbers lane macros, method-block modules and configuration probes apart. A lane is only ever
/// named by the `use` beside it, so two same-named types in different scopes need distinct crate-root names.
pub(crate) static LANES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One declaration a lane carries, in the shape of the source record bindgen reads.
/// A lookup entry keeps its slots unresolved and only answers id, kind, and shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    module: String,
    slots: Vec<Box<RawValue>>,
    json: Box<RawValue>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    lookup: bool,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.module == other.module
            && self.lookup == other.lookup
            && self.json.get() == other.json.get()
            && self
                .slots
                .iter()
                .map(|slot| slot.get())
                .eq(other.slots.iter().map(|slot| slot.get()))
    }
}

/// What a lane expands to: the declared type's id and every declaration it needs.
#[derive(Debug, Serialize, Deserialize)]
struct Lane {
    id: String,
    entries: Vec<Entry>,
}

/// The macro an invocation came from, which decides what its expansion emits.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Kind {
    Data,
    Error,
    Export,
    DataImpl,
    CustomFfi,
    CustomType,
    Pool,
}

impl Kind {
    pub(crate) const fn keyword(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Error => "error",
            Self::Export => "export",
            Self::DataImpl => "data_impl",
            Self::CustomFfi => "custom_ffi",
            Self::CustomType => "custom_type",
            Self::Pool => "pool",
        }
    }

    pub(crate) fn parse(keyword: &str) -> Option<Self> {
        [
            Self::Data,
            Self::Error,
            Self::Export,
            Self::DataImpl,
            Self::CustomFfi,
            Self::CustomType,
            Self::Pool,
        ]
        .into_iter()
        .find(|kind| kind.keyword() == keyword)
    }

    fn impl_capture(self, attribute: &TokenStream) -> ImplCapture {
        match self {
            Self::DataImpl => ImplCapture::Methods,
            _ => ImplCapture::Class(attribute.clone()),
        }
    }
}

/// The fragments one invocation declares, whatever macro it came from.
fn fragments(kind: Kind, attribute: &TokenStream, item: &TokenStream) -> Result<Fragments, String> {
    match kind {
        Kind::Pool => boltffi_scan::capture_interned_string_pool(item.clone())
            .map(|pool| {
                vec![(
                    SourceFragment::InternedStringPool {
                        id: format!("{SELF_ID}::{}", pool.name),
                        values: pool.values,
                    },
                    Vec::new(),
                )]
            })
            .map_err(|error| error.to_string()),
        Kind::CustomType => boltffi_scan::capture_custom(item.clone())
            .map(|captured| vec![(SourceFragment::Custom(captured.def), captured.slots)])
            .map_err(|error| error.to_string()),
        Kind::CustomFfi => {
            let item =
                syn::parse2::<syn::ItemImpl>(item.clone()).map_err(|error| error.to_string())?;
            boltffi_scan::capture_custom_ffi(&item)
                .map(|captured| vec![(SourceFragment::Custom(captured.def), captured.slots)])
                .map_err(|error| error.to_string())
        }
        _ => {
            let parsed =
                syn::parse2::<syn::Item>(item.clone()).map_err(|error| error.to_string())?;
            capture::fragments(&parsed, &kind.impl_capture(attribute), kind == Kind::Error)
        }
    }
}

/// Starts the lane chain for `item`, whose tokens the caller emits separately. A
/// declared type's lane is defined here, before any chain runs, so types that refer to
/// each other never wait on one another; only customs, whose repr lowers through them,
/// define theirs once their chain finishes.
pub(crate) fn start(kind: Kind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    if let Some(name) = own_name(kind, &item).filter(is_builtin_name) {
        return compile_error(&format!(
            "a declared type named `{name}` shadows the builtin `{name}`, which every signature \
             spelling `{name}` crosses as; rename the type"
        ));
    }
    let fragments = match fragments(kind, &attribute, &item) {
        Ok(fragments) => fragments,
        Err(message) => return compile_error(&message),
    };
    let lane = match immediate_lane(kind, &fragments, &item) {
        Ok(lane) => lane,
        Err(message) => return compile_error(&message),
    };
    let pending = lane_paths(&fragments);
    let chain = step(kind, &attribute, &item, Vec::new(), pending);
    quote! {
        #lane
        #chain
    }
}

const fn chains_before_lane(kind: Kind) -> bool {
    matches!(kind, Kind::CustomType | Kind::CustomFfi)
}

fn immediate_lane(
    kind: Kind,
    fragments: &Fragments,
    item: &TokenStream,
) -> Result<TokenStream, String> {
    if chains_before_lane(kind) {
        return Ok(TokenStream::new());
    }
    let parsed = syn::parse2::<syn::Item>(item.clone()).ok();
    let Some((vis, name)) = lane_target(kind, parsed.as_ref(), item) else {
        return Ok(TokenStream::new());
    };
    let krate = crate_name()?;
    let entries = fragments
        .iter()
        .map(|(fragment, _)| {
            Ok(Entry {
                module: krate.clone(),
                slots: Vec::new(),
                json: raw_json(
                    serde_json::to_string(fragment).map_err(|error| error.to_string())?,
                )?,
                lookup: true,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    lane_definition(&vis, &name, &krate, entries)
}

/// Continues a chain after a lane appended its literal.
pub(crate) fn resume(input: TokenStream) -> TokenStream {
    match Resume::parse(input) {
        Ok(resume) => {
            let mut resolved = resume.resolved;
            resolved.push(resume.appended);
            step(
                resume.kind,
                &resume.attribute,
                &resume.item,
                resolved,
                resume.pending,
            )
        }
        Err(message) => compile_error(message),
    }
}

fn step(
    kind: Kind,
    attribute: &TokenStream,
    item: &TokenStream,
    resolved: Vec<Literal>,
    pending: Vec<TokenStream>,
) -> TokenStream {
    let Some((next, rest)) = pending.split_first() else {
        return finish(kind, attribute, item, &resolved)
            .unwrap_or_else(|message| compile_error(&message));
    };
    let facade = capture::facade();
    let keyword = format_ident!("{}", kind.keyword());
    quote! {
        #next! {
            [#facade::__private::lane_resume]
            { #keyword { #attribute } { #item } [#(#resolved)*] [#((#rest))*] }
        }
    }
}

struct Resume {
    kind: Kind,
    attribute: TokenStream,
    item: TokenStream,
    resolved: Vec<Literal>,
    pending: Vec<TokenStream>,
    appended: Literal,
}

impl Resume {
    fn parse(input: TokenStream) -> Result<Self, &'static str> {
        const MALFORMED: &str = "malformed BoltFFI lane continuation";
        let mut tokens = input.into_iter();
        let kind = match tokens.next() {
            Some(TokenTree::Ident(ident)) => Kind::parse(&ident.to_string()).ok_or(MALFORMED)?,
            _ => return Err(MALFORMED),
        };
        let mut group = |delimiter| match tokens.next() {
            Some(TokenTree::Group(group)) if group.delimiter() == delimiter => Ok(group.stream()),
            _ => Err(MALFORMED),
        };
        let attribute = group(Delimiter::Brace)?;
        let item = group(Delimiter::Brace)?;
        let resolved = group(Delimiter::Bracket)?
            .into_iter()
            .map(|token| match token {
                TokenTree::Literal(literal) => Ok(literal),
                _ => Err(MALFORMED),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let pending = group(Delimiter::Bracket)?
            .into_iter()
            .map(|token| match token {
                TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                    Ok(group.stream())
                }
                _ => Err(MALFORMED),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let appended = match tokens.next() {
            Some(TokenTree::Literal(literal)) => literal,
            _ => return Err(MALFORMED),
        };
        Ok(Self {
            kind,
            attribute,
            item,
            resolved,
            pending,
            appended,
        })
    }
}

/// The path of each distinct lane the fragments' slots name, in first-use order.
fn lane_paths(fragments: &Fragments) -> Vec<TokenStream> {
    let mut seen = HashSet::new();
    fragments
        .iter()
        .flat_map(|(_, slots)| slots)
        .map(lane_path)
        .filter(|path| seen.insert(path.to_string()))
        .collect()
}

/// The name the invocation's own declaration goes by, when it declares a type.
fn own_name(kind: Kind, item: &TokenStream) -> Option<syn::Ident> {
    let parsed = syn::parse2::<syn::Item>(item.clone()).ok();
    lane_target(kind, parsed.as_ref(), item).map(|(_, name)| name)
}

/// Leaf names every invocation reads as a builtin, whatever is in scope.
fn is_builtin_name(name: &syn::Ident) -> bool {
    ["Duration", "SystemTime", "Uuid", "Url"]
        .iter()
        .any(|builtin| name == builtin)
}

fn lane_path(slot: &SlotSource) -> TokenStream {
    let mut path = match slot {
        SlotSource::Type(ty) => match &**ty {
            syn::Type::Path(path) => path.path.clone(),
            other => return other.to_token_stream(),
        },
        SlotSource::TraitValue(path) => path.clone(),
    };
    path.segments
        .iter_mut()
        .for_each(|segment| segment.arguments = syn::PathArguments::None);
    path.to_token_stream()
}

fn finish(
    kind: Kind,
    attribute: &TokenStream,
    item: &TokenStream,
    resolved: &[Literal],
) -> Result<TokenStream, String> {
    let parsed = syn::parse2::<syn::Item>(item.clone()).ok();
    let fragments = fragments(kind, attribute, item)?;
    let lanes = resolved
        .iter()
        .map(|literal| {
            serde_json::from_str::<Lane>(&raw_text(literal)?).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let paths = lane_paths(&fragments)
        .into_iter()
        .map(|path| path.to_string())
        .collect::<Vec<_>>();
    let krate = crate_name()?;
    let own = fragments
        .iter()
        .map(|(fragment, slots)| {
            let slots = slots
                .iter()
                .map(|slot| {
                    let path = lane_path(slot).to_string();
                    let index = paths
                        .iter()
                        .position(|candidate| *candidate == path)
                        .ok_or("lane continuation lost a slot")?;
                    raw_json(serde_json::json!({ "id": lanes[index].id }).to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Entry {
                module: krate.clone(),
                slots,
                json: raw_json(
                    serde_json::to_string(fragment).map_err(|error| error.to_string())?,
                )?,
                lookup: false,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut entries = own.clone();
    lanes
        .iter()
        .flat_map(|lane| &lane.entries)
        .for_each(|entry| {
            if !entries.contains(entry) {
                entries.push(entry.clone());
            }
        });
    let selections = fragments
        .iter()
        .enumerate()
        .filter_map(|(index, (fragment, _))| {
            let id = selected_id(fragment, &krate, || {
                let path = lane_path(fragments[index].1.first()?).to_string();
                let index = paths.iter().position(|candidate| *candidate == path)?;
                Some(lanes[index].id.clone())
            })?;
            Some((id, matches!(fragment, SourceFragment::Methods { .. })))
        })
        .collect::<Vec<_>>();
    let selected = selections
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let shallow = selections
        .iter()
        .filter(|(_, methods)| *methods)
        .map(|(id, _)| id.clone())
        .collect::<HashSet<_>>();
    let customs = paths
        .iter()
        .zip(&lanes)
        .map(|(path, lane)| (lane.id.clone(), path.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let expanded = match kind {
        Kind::CustomFfi | Kind::CustomType | Kind::Pool => TokenStream::new(),
        _ => render(
            kind,
            &entries,
            &selected,
            &shallow,
            parsed.as_ref(),
            &customs,
        )?,
    };
    let lane = match lane_target(kind, parsed.as_ref(), item) {
        Some((vis, name)) if chains_before_lane(kind) => {
            lane_definition(&vis, &name, &krate, entries)?
        }
        _ => TokenStream::new(),
    };
    Ok(quote! {
        #expanded
        #lane
    })
}

/// The id a fragment's declaration takes once aggregated, which is what gets expanded.
fn selected_id(
    fragment: &SourceFragment,
    krate: &str,
    target: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let id = match fragment {
        SourceFragment::Record(def) => def.id.as_str(),
        SourceFragment::Enum(def) => def.id.as_str(),
        SourceFragment::Function(def) => def.id.as_str(),
        SourceFragment::Class(def) => def.id.as_str(),
        SourceFragment::Trait(def) => def.id.as_str(),
        SourceFragment::Stream(def) => def.id.as_str(),
        SourceFragment::Constant(def) => def.id.as_str(),
        SourceFragment::Custom(def) => def.id.as_str(),
        SourceFragment::Methods { .. } => return target(),
        SourceFragment::InternedStringPool { .. } | SourceFragment::Unsupported { .. } => {
            return None;
        }
    };
    id.strip_prefix(SELF_ID)
        .map(|rest| format!("{krate}{rest}"))
}

fn render(
    kind: Kind,
    entries: &[Entry],
    selected: &[String],
    shallow: &HashSet<String>,
    item: Option<&syn::Item>,
    customs: &std::collections::HashMap<String, String>,
) -> Result<TokenStream, String> {
    let package = PackageInfo::new(
        std::env::var("CARGO_PKG_NAME").map_err(|_| "`CARGO_PKG_NAME` is not set")?,
        std::env::var("CARGO_PKG_VERSION")
            .ok()
            .filter(|version| !version.is_empty()),
    );
    let records = |lookup: bool| {
        entries
            .iter()
            .filter(|entry| entry.lookup == lookup)
            .map(|entry| RawSourceRecord {
                package: package.clone(),
                module: entry.module.clone(),
                slots: entry
                    .slots
                    .iter()
                    .map(|slot| slot.get().to_owned())
                    .collect(),
                json: entry.json.get().as_bytes().to_vec(),
            })
            .collect::<Vec<_>>()
    };
    let mut contract = aggregate_invocation(&records(false), &records(true), package.clone())
        .map_err(|error| error.to_string())?;
    contract
        .customs
        .iter_mut()
        .for_each(|custom| convert_through_site_path(custom, customs));
    let native = render_surface::<Native>(kind, &contract, selected, shallow)?;
    let wasm32 = render_surface::<Wasm32>(kind, &contract, selected, shallow)?;
    let Some(module) = item.and_then(|item| invocation_module(kind, item)) else {
        return Ok(quote! {
            #[cfg(not(target_arch = "wasm32"))]
            const _: () = {
                #native
            };
            #[cfg(target_arch = "wasm32")]
            const _: () = {
                #wasm32
            };
        });
    };
    let native = one_module_deeper(native);
    let wasm32 = one_module_deeper(wasm32);
    let target = item.and_then(|item| methods_target_import(kind, item));
    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports)]
        #[cfg(not(target_arch = "wasm32"))]
        pub mod #module {
            use super::*;
            #target
            #native
        }
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports)]
        #[cfg(target_arch = "wasm32")]
        pub mod #module {
            use super::*;
            #target
            #wasm32
        }
        #[doc(hidden)]
        #[allow(unused_imports)]
        pub use #module::*;
    })
}

/// The module an export's wrappers live in, named after the item so tests and Rust
/// callers can reach them. Data runtimes need no name and stay in the item's own scope.
/// A type may carry several method blocks in one module, so each block's name is numbered.
fn invocation_module(kind: Kind, item: &syn::Item) -> Option<syn::Ident> {
    let (family, name) = match (kind, item) {
        (Kind::Export, syn::Item::Fn(item)) => ("fn", item.sig.ident.to_string()),
        (Kind::Export, syn::Item::Const(item)) => ("const", item.ident.to_string()),
        (Kind::Export, syn::Item::Trait(item)) => ("trait", item.ident.to_string()),
        (Kind::Export | Kind::DataImpl, syn::Item::Impl(item)) => {
            let syn::Type::Path(path) = &*item.self_ty else {
                return None;
            };
            let family = if kind == Kind::Export {
                "class"
            } else {
                "methods"
            };
            (family, path.path.segments.last()?.ident.to_string())
        }
        _ => return None,
    };
    let name = name.trim_start_matches("r#");
    Some(match kind {
        Kind::DataImpl => format_ident!(
            "__boltffi_{family}_{name}_{}",
            LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ),
        _ => format_ident!("__boltffi_{family}_{name}"),
    })
}

/// Brings a method block's target into its wrapper module under the target's own name,
/// through the path the block wrote, since the wrappers name the type bare.
fn methods_target_import(kind: Kind, item: &syn::Item) -> Option<TokenStream> {
    let (Kind::DataImpl, syn::Item::Impl(item)) = (kind, item) else {
        return None;
    };
    let syn::Type::Path(path) = &*item.self_ty else {
        return None;
    };
    let mut path = path.path.clone();
    if path.leading_colon.is_none() && path.segments.len() == 1 {
        return None;
    }
    path.segments
        .iter_mut()
        .for_each(|segment| segment.arguments = syn::PathArguments::None);
    let rooted = path.leading_colon.is_some()
        || ["crate", "self", "super"]
            .iter()
            .any(|root| path.segments[0].ident == root);
    let path = match rooted {
        true => one_module_deeper(path.to_token_stream()),
        false => quote! { super::#path },
    };
    Some(quote! { use #path; })
}

/// Rewrites path roots for code moved one module below where it was written.
fn one_module_deeper(tokens: TokenStream) -> TokenStream {
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        let after_path_separator = index >= 2
            && matches!(&tokens[index - 1], TokenTree::Punct(punct) if punct.as_char() == ':')
            && matches!(&tokens[index - 2], TokenTree::Punct(punct) if punct.as_char() == ':');
        let before_path_separator = matches!(tokens.get(index + 1), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
            && matches!(tokens.get(index + 2), Some(TokenTree::Punct(punct)) if punct.as_char() == ':');
        match token {
            TokenTree::Ident(ident)
                if !after_path_separator && before_path_separator && ident == "self" =>
            {
                output.push(TokenTree::Ident(proc_macro2::Ident::new(
                    "super",
                    ident.span(),
                )));
            }
            TokenTree::Ident(ident)
                if !after_path_separator && before_path_separator && ident == "super" =>
            {
                output.extend(quote::quote_spanned!(ident.span() => super::));
                output.push(token.clone());
            }
            TokenTree::Group(group) => {
                let mut deeper =
                    proc_macro2::Group::new(group.delimiter(), one_module_deeper(group.stream()));
                deeper.set_span(group.span());
                output.push(TokenTree::Group(deeper));
            }
            _ => output.push(token.clone()),
        }
    }
    output.into_iter().collect()
}

fn render_surface<S: SurfaceLower>(
    kind: Kind,
    contract: &boltffi_ast::SourceContract,
    selected: &[String],
    shallow: &HashSet<String>,
) -> Result<TokenStream, String>
where
    for<'lowered> Expander<'lowered>: SurfaceRender<'lowered, S>,
{
    let selection = selected.iter().cloned().collect::<HashSet<_>>();
    let lowered =
        lower_invocation::<S>(contract, &selection, shallow).map_err(|error| error.to_string())?;
    let expansion = Expansion::new(&lowered).with_lookup_traits(contract);
    let expander = Expander::invocation(contract, selected.iter().cloned());
    match kind {
        Kind::Data | Kind::Error => selected
            .iter()
            .map(|id| {
                let runtime = if contract
                    .records
                    .iter()
                    .any(|record| record.id.as_str() == id)
                {
                    expander.record_runtime(&boltffi_ast::RecordId::new(id.clone()), &expansion)
                } else {
                    expander.enumeration_runtime(&boltffi_ast::EnumId::new(id.clone()), &expansion)
                };
                runtime.map_err(|error| error.to_string())
            })
            .collect(),
        Kind::Export | Kind::DataImpl => expander
            .surface(&expansion)
            .map_err(|error| error.to_string()),
        Kind::CustomFfi | Kind::CustomType | Kind::Pool => Ok(TokenStream::new()),
    }
}

/// The per-surface entry point of [`Expander`], so lane rendering can stay generic.
pub(crate) trait SurfaceRender<'lowered, S: SurfaceLower> {
    fn surface(
        &self,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, crate::expansion::error::Error>;
}

impl<'lowered> SurfaceRender<'lowered, Native> for Expander<'lowered> {
    fn surface(
        &self,
        expansion: &Expansion<'lowered, Native>,
    ) -> Result<TokenStream, crate::expansion::error::Error> {
        self.native(expansion)
    }
}

impl<'lowered> SurfaceRender<'lowered, Wasm32> for Expander<'lowered> {
    fn surface(
        &self,
        expansion: &Expansion<'lowered, Wasm32>,
    ) -> Result<TokenStream, crate::expansion::error::Error> {
        self.wasm32(expansion)
    }
}

/// Custom conversions called through the path this site wrote for the custom type, so
/// they resolve here no matter which module declared them.
fn convert_through_site_path(
    custom: &mut boltffi_ast::CustomTypeDef,
    customs: &std::collections::HashMap<String, String>,
) {
    let Some(path) = customs.get(custom.id.as_str()) else {
        return;
    };
    let convert = |converter: &mut boltffi_ast::CustomTypeConverter, method: &str| {
        let source = match converter {
            boltffi_ast::CustomTypeConverter::TraitMethod(_) => {
                format!("<{path} as ::boltffi::CustomFfiConvertible>::{method}")
            }
            _ => format!(
                "<{path} as ::boltffi::__private::CustomType<crate::__BoltffiTag>>::{method}"
            ),
        };
        *converter =
            boltffi_ast::CustomTypeConverter::Expr(boltffi_ast::CustomConverterExpr::new(source));
    };
    convert(&mut custom.converters.into_ffi, "into_ffi");
    convert(&mut custom.converters.try_from_ffi, "try_from_ffi");
}

/// The visibility and name a declaration's lane takes, when it defines one.
fn lane_target(
    kind: Kind,
    item: Option<&syn::Item>,
    tokens: &TokenStream,
) -> Option<(syn::Visibility, syn::Ident)> {
    let public = || syn::Visibility::Public(Default::default());
    let impl_target = |item: &syn::ItemImpl| match &*item.self_ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.clone()),
        _ => None,
    };
    match (kind, item) {
        (Kind::CustomType, _) => crate::custom::r#type::parse_spec(tokens.clone().into())
            .ok()
            .map(|spec| {
                let vis = match spec.names_remote() {
                    true => public(),
                    false => spec.vis,
                };
                (vis, spec.name)
            }),
        (Kind::Pool, _) => syn::parse2::<crate::interned_string::PoolSpec>(tokens.clone())
            .ok()
            .map(|spec| (spec.visibility, spec.name)),
        (Kind::DataImpl, _) => None,
        (_, Some(syn::Item::Struct(item))) => Some((item.vis.clone(), item.ident.clone())),
        (_, Some(syn::Item::Enum(item))) => Some((item.vis.clone(), item.ident.clone())),
        (_, Some(syn::Item::Trait(item))) => Some((item.vis.clone(), item.ident.clone())),
        (_, Some(syn::Item::Impl(item))) => impl_target(item).map(|name| (public(), name)),
        _ => None,
    }
}

/// Defines a type's lane, and a crate-root marker so two declarations of one name fail to build.
fn lane_definition(
    vis: &syn::Visibility,
    name: &syn::Ident,
    krate: &str,
    entries: Vec<Entry>,
) -> Result<TokenStream, String> {
    let lane = Lane {
        id: format!("{krate}::{name}"),
        entries,
    };
    let literal = raw_literal(&serde_json::to_string(&lane).map_err(|error| error.to_string())?)?;
    let macro_name = format_ident!(
        "__boltffi_lane_{krate}_{name}_{}",
        LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let declared = format_ident!(
        "__boltffi_declared_{}",
        name.to_string().trim_start_matches("r#")
    );
    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_local_definitions)]
        #[macro_export]
        macro_rules! #declared { () => {} }
        #[doc(hidden)]
        #[allow(non_local_definitions)]
        #[macro_export]
        macro_rules! #macro_name {
            ([$($callback:tt)*] { $($state:tt)* }) => {
                $($callback)*! { $($state)* #literal }
            };
        }
        #[doc(hidden)]
        #vis use #macro_name as #name;
    })
}

fn raw_json(json: String) -> Result<Box<RawValue>, String> {
    RawValue::from_string(json).map_err(|error| error.to_string())
}

/// A raw string literal, so the lane's JSON crosses macro hops without escaping.
fn raw_literal(text: &str) -> Result<Literal, String> {
    let hashes = (1..)
        .map(|count| "#".repeat(count))
        .find(|hashes| !text.contains(&format!("\"{hashes}")))
        .unwrap_or_default();
    format!("r{hashes}\"{text}\"{hashes}")
        .parse::<Literal>()
        .map_err(|error| error.to_string())
}

fn raw_text(literal: &Literal) -> Result<String, String> {
    let text = literal.to_string();
    let raw = text
        .strip_prefix('r')
        .ok_or("malformed BoltFFI lane literal")?;
    let hashes = raw.len() - raw.trim_start_matches('#').len();
    raw.get(hashes + 1..raw.len() - hashes - 1)
        .map(str::to_owned)
        .ok_or_else(|| "malformed BoltFFI lane literal".to_owned())
}

fn crate_name() -> Result<String, String> {
    std::env::var("CARGO_CRATE_NAME").map_err(|_| "`CARGO_CRATE_NAME` is not set".to_owned())
}

fn compile_error(message: &str) -> TokenStream {
    let message = format!("BoltFFI: {message}");
    quote::quote_spanned! { Span::call_site() => compile_error!(#message); }
}

#[cfg(test)]
mod tests {
    use super::{raw_literal, raw_text};

    #[test]
    fn lane_literals_carry_json_that_closes_raw_strings() {
        let json = r###"{"doc":"ends with \"# and \"## here"}"###;

        let literal = raw_literal(json).expect("raw literal");

        assert_eq!(raw_text(&literal).expect("raw text"), json);
    }
}
