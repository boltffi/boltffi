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
//!
//! A lane is reached only through a path that names it, so it cannot follow a `type`
//! alias or the remote path of a custom type. `boltffi generate` therefore keeps the
//! records of its records build, where rustc resolved every spelling, in Cargo's build
//! directory.
//! An invocation the record store holds takes its references from the store instead of
//! a lane chain, and checks in const context that each spelling still resolves to the
//! declaration the store recorded. Lowering and expansion are the same either way.

mod store;

use std::collections::HashSet;

use boltffi_ast::{PackageInfo, Path, PathRoot, PathSegment};
use boltffi_binding::{
    Native, RawSourceRecord, SELF_ID, SLOT_ID_PREFIX, SourceFragment, SurfaceLower, Wasm32,
    aggregate_invocation, lower_invocation,
};
use boltffi_scan::SlotSource;
use proc_macro2::{Delimiter, Literal, Span, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::capture::{self, Capture, Fragments};
use crate::expansion::{contract::Expansion, expander::Expander};

pub(crate) use self::store::Store;

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
    ExportMethods,
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
            Self::ExportMethods => "export_methods",
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
            Self::ExportMethods,
            Self::DataImpl,
            Self::CustomFfi,
            Self::CustomType,
            Self::Pool,
        ]
        .into_iter()
        .find(|kind| kind.keyword() == keyword)
    }

    pub(crate) fn capture(self, attribute: &TokenStream) -> Capture {
        match self {
            Self::Export => Capture::Class(attribute.clone()),
            Self::DataImpl => Capture::Methods,
            Self::ExportMethods => Capture::ClassMethods,
            _ => Capture::Item,
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
            capture::fragments(&parsed, &kind.capture(attribute), kind == Kind::Error)
        }
    }
}

const SOURCE_RECORDS_ENV: &str = "BOLTFFI_SOURCE_RECORDS";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Build {
    SourceRecords,
    Wrappers,
}

impl Build {
    fn current() -> Self {
        match std::env::var_os(SOURCE_RECORDS_ENV) {
            Some(value) if value == "1" => Self::SourceRecords,
            _ => Self::Wrappers,
        }
    }
}

/// Expands `item`, whose tokens the caller emits separately. A declared type's lane is
/// defined here, before anything resolves, so types that refer to each other never wait on
/// one another; only customs, whose repr lowers through them, define theirs once their
/// references resolve. An item the record store holds resolves its references from the
/// store; any other item resolves them through a lane chain. Bindgen's records build
/// emits records only: no lane, no chain, no wrapper.
pub(crate) fn start(kind: Kind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    if let Some(name) = own_name(kind, &item).filter(is_builtin_name) {
        return compile_error(&format!(
            "a declared type named `{name}` shadows the builtin `{name}`, which every signature \
             spelling `{name}` crosses as; rename the type"
        ));
    }
    let scaffolded = (kind != Kind::Pool).then(|| {
        quote! {
            const _: ::core::marker::PhantomData<crate::__BoltffiTag> = ::core::marker::PhantomData;
        }
    });
    if Build::current() == Build::SourceRecords {
        return quote! { #scaffolded };
    }
    let fragments = match fragments(kind, &attribute, &item) {
        Ok(fragments) => fragments,
        Err(message) => return compile_error(&message),
    };
    let store = Store::find();
    let expanded = match store.and_then(|store| store.resolve(&fragments)) {
        Some(resolution) => resolution.and_then(|resolution| {
            let expanded = finish(
                kind,
                &attribute,
                &item,
                &resolution.references,
                Resolved::FromStore,
            )?;
            let checks = resolution.checks;
            Ok(quote! {
                #expanded
                #checks
            })
        }),
        None => immediate_lane(kind, &fragments, &item).map(|lane| {
            let stale = store
                .map(|store| store.stale_warning(&fragments))
                .unwrap_or_default();
            let chain = step(kind, &attribute, &item, Vec::new(), lane_paths(&fragments));
            quote! {
                #lane
                #stale
                #chain
            }
        }),
    };
    let expanded = expanded.unwrap_or_else(|message| compile_error(&message));
    quote! {
        #scaffolded
        #expanded
    }
}

/// Where an invocation's references resolved, which decides when its lane is defined.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Resolved {
    /// Through lane chains; a declared type defined its lane before the chain ran.
    ThroughLanes,
    /// From the record store, with no chain to wait on, so every lane carries resolved
    /// entries.
    FromStore,
}

const fn chains_before_lane(kind: Kind) -> bool {
    matches!(kind, Kind::CustomType | Kind::CustomFfi)
}

/// The lane a declaration defines before its chain runs.
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
        return fragments(kind, attribute, item)
            .and_then(|fragments| Reference::through_lanes(&fragments, &resolved))
            .and_then(|references| {
                finish(kind, attribute, item, &references, Resolved::ThroughLanes)
            })
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

pub(crate) fn lane_path(slot: &SlotSource) -> TokenStream {
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

/// What one spelling an invocation writes resolves to: the slot descriptor its own record
/// takes, the declarations the spelling reaches, and the entries those declarations need
/// to aggregate.
pub(crate) struct Reference {
    path: String,
    descriptor: Box<RawValue>,
    reached: Vec<Reached>,
    entries: Vec<Entry>,
}

/// A declaration a spelling reaches: how this site names it, and the tag its declaring
/// crate keys its conversions to.
pub(crate) struct Reached {
    id: String,
    path: String,
    tag: String,
}

impl Reference {
    fn through_lanes(fragments: &Fragments, resolved: &[Literal]) -> Result<Vec<Self>, String> {
        lane_paths(fragments)
            .iter()
            .zip(resolved)
            .map(|(path, literal)| {
                let lane = serde_json::from_str::<Lane>(&raw_text(literal)?)
                    .map_err(|error| error.to_string())?;
                let path = path.to_string();
                Ok(Self {
                    descriptor: raw_json(serde_json::json!({ "id": lane.id }).to_string())?,
                    reached: vec![Reached {
                        tag: format!("{path}!(@tag)"),
                        id: lane.id,
                        path: path.clone(),
                    }],
                    entries: lane.entries,
                    path,
                })
            })
            .collect()
    }

    /// The declaration the spelling names itself, rather than one a container it names holds.
    fn named(&self) -> Option<&Reached> {
        self.reached
            .first()
            .filter(|reached| reached.path == self.path)
    }

    fn of<'reference>(
        references: &'reference [Self],
        slot: &SlotSource,
    ) -> Result<&'reference Self, String> {
        let path = lane_path(slot).to_string();
        references
            .iter()
            .find(|reference| reference.path == path)
            .ok_or_else(|| format!("no reference resolves `{path}`"))
    }
}

fn finish(
    kind: Kind,
    attribute: &TokenStream,
    item: &TokenStream,
    references: &[Reference],
    resolved: Resolved,
) -> Result<TokenStream, String> {
    let parsed = syn::parse2::<syn::Item>(item.clone()).ok();
    let fragments = fragments(kind, attribute, item)?;
    let krate = crate_name()?;
    check_methods_target(kind, &fragments, references)?;
    let own = fragments
        .iter()
        .map(|(fragment, slots)| {
            let slots = slots
                .iter()
                .map(|slot| {
                    Reference::of(references, slot).map(|reference| reference.descriptor.clone())
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
    references
        .iter()
        .flat_map(|reference| &reference.entries)
        .for_each(|entry| {
            if !entries.contains(entry) {
                entries.push(entry.clone());
            }
        });
    let selections = fragments
        .iter()
        .filter_map(|(fragment, slots)| {
            let id = selected_id(fragment, &krate, || {
                let reference = Reference::of(references, slots.first()?).ok()?;
                reference.named().map(|reached| reached.id.clone())
            })?;
            Some((id, fragment))
        })
        .flat_map(|(id, fragment)| {
            let streams = match fragment {
                SourceFragment::Methods { streams, .. } => streams.as_slice(),
                _ => &[],
            };
            let streams = streams
                .iter()
                .map(|stream| (format!("{id}::{}", stream.name.spelling()), false))
                .collect::<Vec<_>>();
            let methods = matches!(fragment, SourceFragment::Methods { .. });
            std::iter::once((id, methods)).chain(streams)
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
    let customs = references
        .iter()
        .flat_map(|reference| &reference.reached)
        .map(|reached| {
            let reach = CustomReach {
                path: reached.path.clone(),
                tag: reached.tag.clone(),
            };
            (reached.id.clone(), reach)
        })
        .collect::<std::collections::HashMap<_, _>>();
    let expanded = match kind {
        Kind::CustomFfi | Kind::CustomType | Kind::Pool => TokenStream::new(),
        _ => render(
            kind,
            &entries,
            &selected,
            &shallow,
            parsed.as_ref(),
            customs,
        )?,
    };
    let lane = match lane_target(kind, parsed.as_ref(), item) {
        Some((vis, name)) if chains_before_lane(kind) || resolved == Resolved::FromStore => {
            lane_definition(&vis, &name, &krate, entries)?
        }
        _ => TokenStream::new(),
    };
    let repr_tag = repr_tag(kind, item, &fragments, references)?;
    Ok(quote! {
        #expanded
        #lane
        #repr_tag
    })
}

/// Refuses a methods block written with the marker of the other kind of declaration.
fn check_methods_target(
    kind: Kind,
    fragments: &Fragments,
    references: &[Reference],
) -> Result<(), String> {
    let Some((SourceFragment::Methods { spelling, .. }, slots)) = fragments.first() else {
        return Ok(());
    };
    let target = slots
        .first()
        .and_then(|slot| Reference::of(references, slot).ok())
        .filter(|reference| reference.named().is_some())
        .and_then(|reference| reference.entries.first())
        .ok_or("lane continuation lost a methods target")?;
    let target = serde_json::from_str::<SourceFragment>(target.json.get()).ok();
    let class = matches!(target, Some(SourceFragment::Class(_)));
    match (kind, class) {
        _ if matches!(target, Some(SourceFragment::Custom(_))) => Err(format!(
            "`{spelling}` is a custom type, and a custom type takes no methods blocks; \
             its methods stay in Rust"
        )),
        (Kind::ExportMethods, false) => Err(format!(
            "`#[export] impl` adds methods to a class declared by an `#[export]` struct or enum, \
             and `{spelling}` is not one; use `#[data(impl)]` on a `#[data]` type"
        )),
        (Kind::DataImpl, true) => Err(format!(
            "`#[data(impl)]` adds methods to a `#[data]` type, and `{spelling}` is an exported \
             class; use `#[export] impl`"
        )),
        _ => Ok(()),
    }
}

/// Names the tag a custom's representation converts under, when that representation is
/// another declared type, through that type's lane.
fn repr_tag(
    kind: Kind,
    item: &TokenStream,
    fragments: &Fragments,
    references: &[Reference],
) -> Result<TokenStream, String> {
    let Some((SourceFragment::Custom(custom), slots)) = fragments.first() else {
        return Ok(TokenStream::new());
    };
    let mut repr_slots = Vec::new();
    declared_in_repr(&custom.repr, &mut Vec::new(), &mut |expr, _| {
        let slot = match expr {
            boltffi_ast::TypeExpr::Record { id, .. } => id
                .as_str()
                .strip_prefix(SLOT_ID_PREFIX)
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| slots.get(index)),
            _ => None,
        };
        repr_slots.push(slot);
    });
    let tags = repr_slots
        .into_iter()
        .map(|slot| match slot {
            Some(slot) => Reference::of(references, slot)?
                .reached
                .iter()
                .map(|reached| {
                    syn::parse_str::<TokenStream>(&reached.tag)
                        .map(Some)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, String>>(),
            None => Ok(vec![None]),
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let target = match kind {
        Kind::CustomType => crate::custom::r#type::parse_spec(item.clone().into())
            .ok()
            .map(|spec| spec.remote),
        Kind::CustomFfi => syn::parse2::<syn::ItemImpl>(item.clone())
            .ok()
            .map(|item| *item.self_ty),
        _ => None,
    };
    let Some(target) = target else {
        return Ok(TokenStream::new());
    };
    Ok(tags
        .into_iter()
        .enumerate()
        .filter_map(|(position, tag)| Some((position, tag?)))
        .map(|(position, tag)| {
            quote! {
                impl ::boltffi::__private::CustomReprTag<crate::__BoltffiTag, #position> for #target {
                    type Tag = #tag;
                }
            }
        })
        .collect())
}

/// Visits the declared types a representation names in source order, each with the
/// argument positions leading to it.
fn declared_in_repr<'expr>(
    expr: &'expr boltffi_ast::TypeExpr,
    steps: &mut Vec<usize>,
    visit: &mut impl FnMut(&'expr boltffi_ast::TypeExpr, &[usize]),
) {
    use boltffi_ast::TypeExpr;
    let args: Vec<&TypeExpr> = match expr {
        TypeExpr::Record { .. }
        | TypeExpr::Enum { .. }
        | TypeExpr::Class { .. }
        | TypeExpr::Custom { .. } => return visit(expr, steps),
        TypeExpr::Vec(inner)
        | TypeExpr::Option(inner)
        | TypeExpr::Boxed(inner)
        | TypeExpr::Arc(inner) => {
            vec![inner]
        }
        TypeExpr::Result { ok, err } => vec![ok, err],
        TypeExpr::Map { key, value, .. } => vec![key, value],
        TypeExpr::Tuple(elements) => elements.iter().collect(),
        _ => return,
    };
    args.into_iter().enumerate().for_each(|(position, arg)| {
        steps.push(position);
        declared_in_repr(arg, steps, visit);
        steps.pop();
    });
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
    customs: std::collections::HashMap<String, CustomReach>,
) -> Result<TokenStream, String> {
    let package = PackageInfo::new(
        crate_name()?,
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
    let mut contract = aggregate_invocation(
        &records(false),
        &records(true),
        package.clone(),
        &argument_alias(),
    )
    .map_err(|error| error.to_string())?;
    let customs = reach_through_reprs(&contract.customs, customs);
    contract
        .customs
        .iter_mut()
        .for_each(|custom| convert_through_site_path(custom, &customs));
    let native = render_surface::<Native>(kind, &contract, selected, shallow)?;
    let wasm32 = render_surface::<Wasm32>(kind, &contract, selected, shallow)?;
    let Some(module) = item.and_then(|item| invocation_module(kind, item)) else {
        return Ok(quote! {
            #[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
            #[allow(deprecated)]
            const _: () = {
                #native
            };
            #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
            #[allow(deprecated)]
            const _: () = {
                #wasm32
            };
        });
    };
    let native = one_module_deeper(native);
    let wasm32 = one_module_deeper(wasm32);
    let target = item
        .zip(shallow.iter().next())
        .and_then(|(item, target)| methods_target_import(kind, item, target));
    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports, deprecated)]
        #[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
        pub mod #module {
            use super::*;
            #target
            #native
        }
        #[doc(hidden)]
        #[allow(non_snake_case, unused_imports, deprecated)]
        #[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
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
        (
            Kind::Export,
            syn::Item::Struct(syn::ItemStruct { ident, .. })
            | syn::Item::Enum(syn::ItemEnum { ident, .. }),
        ) => ("class", ident.to_string()),
        (Kind::ExportMethods | Kind::DataImpl, syn::Item::Impl(item)) => {
            let syn::Type::Path(path) = &*item.self_ty else {
                return None;
            };
            ("methods", path.path.segments.last()?.ident.to_string())
        }
        _ => return None,
    };
    let name = name.trim_start_matches("r#");
    Some(match kind {
        Kind::DataImpl | Kind::ExportMethods => format_ident!(
            "__boltffi_{family}_{name}_{}",
            LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ),
        _ => format_ident!("__boltffi_{family}_{name}"),
    })
}

/// Brings a method block's target into its wrapper module under its declared name,
/// through the path the block wrote, since the wrappers name the type bare.
fn methods_target_import(kind: Kind, item: &syn::Item, target: &str) -> Option<TokenStream> {
    let (Kind::DataImpl | Kind::ExportMethods, syn::Item::Impl(item)) = (kind, item) else {
        return None;
    };
    let syn::Type::Path(path) = &*item.self_ty else {
        return None;
    };
    let declared = syn::parse_str::<syn::Ident>(target.rsplit("::").next()?).ok()?;
    let mut path = path.path.clone();
    if path.leading_colon.is_none()
        && path.segments.len() == 1
        && path.segments[0].ident == declared
    {
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
    Some(quote! { use #path as #declared; })
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
    let expansion = Expansion::invocation(&lowered).with_lookup_traits(contract);
    let expander = Expander::invocation(contract, selected.iter().cloned());
    let expander = match kind {
        Kind::ExportMethods => expander.extending_classes(),
        _ => expander,
    };
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
        Kind::Export | Kind::ExportMethods | Kind::DataImpl => expander
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

/// The facade's `ReprArgOf`, through which a site names a declaration that a container
/// written as a type alias holds.
fn argument_alias() -> Path {
    Path::new(
        PathRoot::Absolute,
        [capture::facade_name().as_str(), "__private", REPR_ARG_OF]
            .into_iter()
            .map(PathSegment::new)
            .collect(),
    )
}

const REPR_ARG_OF: &str = "ReprArgOf";

/// How this site reaches a custom: the type path, and the tag its declaring crate keys
/// the conversion to.
struct CustomReach {
    path: String,
    tag: String,
}

/// Names a custom this site reaches only as another custom's representation through
/// that custom's associated representation type, keyed to the crate that declared it.
fn reach_through_reprs(
    defs: &[boltffi_ast::CustomTypeDef],
    mut reached: std::collections::HashMap<String, CustomReach>,
) -> std::collections::HashMap<String, CustomReach> {
    while let Some((id, reach)) = defs.iter().find_map(|custom| {
        let outer = reached.get(custom.id.as_str())?;
        let repr = match custom.converters.into_ffi {
            boltffi_ast::CustomTypeConverter::TraitMethod(_) => {
                format!(
                    "<{} as ::boltffi::CustomFfiConvertible>::FfiRepr",
                    outer.path
                )
            }
            _ => format!(
                "<{} as ::boltffi::__private::CustomType<{}>>::Repr",
                outer.path, outer.tag
            ),
        };
        let mut inner = Vec::new();
        declared_in_repr(&custom.repr, &mut Vec::new(), &mut |expr, steps| {
            let position = inner.len();
            let boltffi_ast::TypeExpr::Custom { id, .. } = expr else {
                return inner.push(None);
            };
            let path = steps.iter().fold(repr.clone(), |path, step| {
                format!("<{path} as ::boltffi::__private::ReprArg<{step}>>::Arg")
            });
            let tag = format!(
                "<{} as ::boltffi::__private::CustomReprTag<{}, {position}>>::Tag",
                outer.path, outer.tag
            );
            inner.push(Some((id.as_str().to_owned(), CustomReach { path, tag })));
        });
        inner
            .into_iter()
            .flatten()
            .find(|(id, _)| !reached.contains_key(id.as_str()))
    }) {
        reached.insert(id, reach);
    }
    reached
}

/// Custom conversions called through the path this site wrote for the custom type, so
/// they resolve here no matter which module or crate declared them.
fn convert_through_site_path(
    custom: &mut boltffi_ast::CustomTypeDef,
    customs: &std::collections::HashMap<String, CustomReach>,
) {
    let Some(CustomReach { path, tag }) = customs.get(custom.id.as_str()) else {
        return;
    };
    let convert = |converter: &mut boltffi_ast::CustomTypeConverter, method: &str| {
        let source = match converter {
            boltffi_ast::CustomTypeConverter::TraitMethod(_) => {
                format!("<{path} as ::boltffi::CustomFfiConvertible>::{method}")
            }
            _ => format!("<{path} as ::boltffi::__private::CustomType<{tag}>>::{method}"),
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
        (Kind::DataImpl | Kind::ExportMethods, _) => None,
        (_, Some(syn::Item::Struct(item))) => Some((item.vis.clone(), item.ident.clone())),
        (_, Some(syn::Item::Enum(item))) => Some((item.vis.clone(), item.ident.clone())),
        (_, Some(syn::Item::Trait(item))) => Some((item.vis.clone(), item.ident.clone())),
        (Kind::CustomFfi, Some(syn::Item::Impl(item))) => match &*item.self_ty {
            syn::Type::Path(path) => path
                .path
                .segments
                .last()
                .map(|segment| (public(), segment.ident.clone())),
            _ => None,
        },
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
    let bare = name.to_string().trim_start_matches("r#").to_owned();
    let macro_name = format_ident!(
        "__boltffi_lane_{krate}_{bare}_{}",
        LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let declared = format_ident!("__boltffi_declared_{bare}");
    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_local_definitions)]
        #[macro_export]
        macro_rules! #declared { () => {} }
        #[doc(hidden)]
        #[allow(non_local_definitions)]
        #[macro_export]
        macro_rules! #macro_name {
            (@tag) => { $crate::__BoltffiTag };
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
