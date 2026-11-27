//! Builds a source contract from Rust source text through the same capture and
//! aggregation the `#[export]` and `#[data]` macros use, with type descriptors
//! resolved by name in place of the compiler's `TypeDesc` impls.
//!
//! Every `cfg` counts as off, and `cfg_attr` is not expanded.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use boltffi_ast::{PackageInfo, Primitive, SourceContract};
use boltffi_binding::{RawSourceRecord, SELF_ID, SourceFragment, SourceFragmentError, TypeNode};
use boltffi_scan::{ScanError, SlotSource};

/// Why a source could not become a contract.
#[derive(Debug)]
pub enum Error {
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        error: syn::Error,
    },
    Capture {
        item: String,
        error: ScanError,
    },
    Refused {
        item: String,
        reason: &'static str,
    },
    Undeclared {
        name: String,
    },
    Ambiguous {
        name: String,
    },
    UnsupportedSlot {
        spelling: String,
    },
    Aggregate(SourceFragmentError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(formatter, "{}: {error}", path.display()),
            Self::Parse { path, error } => write!(formatter, "{}: {error}", path.display()),
            Self::Capture { item, error } => {
                write!(formatter, "`{item}` does not capture: {error}")
            }
            Self::Refused { item, reason } => write!(formatter, "`{item}` is refused: {reason}"),
            Self::Undeclared { name } => write!(formatter, "`{name}` is not declared for BoltFFI"),
            Self::Ambiguous { name } => {
                write!(
                    formatter,
                    "`{name}` names more than one type, alias or custom remote"
                )
            }
            Self::UnsupportedSlot { spelling } => {
                write!(formatter, "`{spelling}` has no type descriptor")
            }
            Self::Aggregate(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

/// The contract the macros would record for the crate whose root file is `root`, with its
/// `mod name;` files read the way rustc finds them and every `cfg` off.
pub fn scan_source(root: &Path, package: PackageInfo) -> Result<SourceContract, Error> {
    scan_items(&load(root, &ModuleHome::of(root, None))?, package)
}

/// Where rustc looks for the modules a file or inline module declares: under `directory`,
/// inside the folder named after `owner` when a file other than `mod.rs` declares them.
struct ModuleHome {
    directory: PathBuf,
    owner: Option<String>,
}

impl ModuleHome {
    fn of(file: &Path, owner: Option<String>) -> Self {
        Self {
            directory: file.parent().unwrap_or(Path::new(".")).to_owned(),
            owner,
        }
    }

    fn children(&self) -> PathBuf {
        match &self.owner {
            Some(owner) => self.directory.join(owner),
            None => self.directory.clone(),
        }
    }

    fn inline(&self, name: &str, path: Option<String>) -> Self {
        Self {
            directory: match path {
                Some(path) => self.directory.join(path),
                None => self.children().join(name),
            },
            owner: None,
        }
    }

    /// The file `mod name;` names, and where that file's own modules live.
    fn file(&self, name: &str, path: Option<String>) -> (PathBuf, Self) {
        let flat = self.children().join(format!("{name}.rs"));
        let (file, owner) = match path {
            Some(path) => (self.directory.join(path), None),
            None if flat.exists() => (flat, Some(name.to_owned())),
            None => (self.children().join(name).join("mod.rs"), None),
        };
        let home = Self::of(&file, owner);
        (file, home)
    }
}

fn load(path: &Path, home: &ModuleHome) -> Result<Vec<syn::Item>, Error> {
    let text = std::fs::read_to_string(path).map_err(|error| Error::Read {
        path: path.to_owned(),
        error,
    })?;
    let file = syn::parse_file(&text).map_err(|error| Error::Parse {
        path: path.to_owned(),
        error,
    })?;
    resolve_modules(file.items, home)
}

fn resolve_modules(items: Vec<syn::Item>, home: &ModuleHome) -> Result<Vec<syn::Item>, Error> {
    items
        .into_iter()
        .filter(|item| active(item_attrs(item)))
        .map(|item| match item {
            syn::Item::Mod(mut module) => {
                let name = module.ident.to_string();
                let path = path_attribute(&module.attrs);
                let items = match module.content.take() {
                    Some((_, items)) => resolve_modules(items, &home.inline(&name, path))?,
                    None => {
                        let (file, home) = home.file(&name, path);
                        load(&file, &home)?
                    }
                };
                module.content = Some((Default::default(), items));
                module.semi = None;
                Ok(syn::Item::Mod(module))
            }
            item => Ok(item),
        })
        .collect()
}

fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        syn::Meta::NameValue(pair) if pair.path.is_ident("path") => match &pair.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(path),
                ..
            }) => Some(path.value()),
            _ => None,
        },
        _ => None,
    })
}

/// Whether every `#[cfg]` on an item holds when no cfg name or feature is set.
fn active(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .all(|attr| attr.parse_args_with(holds).unwrap_or(false))
}

/// Whether what carries `attrs` survives configuration, dropping its `#[cfg]`s as the
/// compiler does for what it keeps.
fn configured(attrs: &mut Vec<syn::Attribute>) -> bool {
    let kept = active(attrs);
    attrs.retain(|attr| !attr.path().is_ident("cfg"));
    kept
}

/// Leaves `item` as the compiler hands it to a macro: without the members whose `#[cfg]`
/// is off.
fn configure(item: &mut syn::Item) {
    if let Some(attrs) = attrs_mut(item) {
        configured(attrs);
    }
    match item {
        syn::Item::Struct(item) => configure_fields(&mut item.fields),
        syn::Item::Enum(item) => {
            item.variants = std::mem::take(&mut item.variants)
                .into_iter()
                .filter_map(|mut variant| configured(&mut variant.attrs).then_some(variant))
                .collect();
            item.variants
                .iter_mut()
                .for_each(|variant| configure_fields(&mut variant.fields));
        }
        syn::Item::Impl(item) => item.items.retain_mut(|member| match member {
            syn::ImplItem::Fn(member) => configured(&mut member.attrs),
            syn::ImplItem::Const(member) => configured(&mut member.attrs),
            _ => true,
        }),
        syn::Item::Trait(item) => item.items.retain_mut(|member| match member {
            syn::TraitItem::Fn(member) => configured(&mut member.attrs),
            syn::TraitItem::Const(member) => configured(&mut member.attrs),
            _ => true,
        }),
        _ => {}
    }
}

fn configure_fields(fields: &mut syn::Fields) {
    let fields = match fields {
        syn::Fields::Named(fields) => &mut fields.named,
        syn::Fields::Unnamed(fields) => &mut fields.unnamed,
        syn::Fields::Unit => return,
    };
    *fields = std::mem::take(fields)
        .into_iter()
        .filter_map(|mut field| configured(&mut field.attrs).then_some(field))
        .collect();
}

fn holds(input: syn::parse::ParseStream<'_>) -> syn::Result<bool> {
    use syn::ext::IdentExt;
    if input.peek(syn::LitBool) {
        return input.parse::<syn::LitBool>().map(|literal| literal.value);
    }
    let name = input.call(syn::Ident::parse_any)?;
    if input.peek(syn::token::Paren) {
        let content;
        syn::parenthesized!(content in input);
        let nested = content.parse_terminated(holds, syn::Token![,])?;
        return Ok(match name.to_string().as_str() {
            "not" => !nested.iter().any(|held| *held),
            "all" => nested.iter().all(|held| *held),
            "any" => nested.iter().any(|held| *held),
            _ => false,
        });
    }
    if input.parse::<Option<syn::Token![=]>>()?.is_some() {
        input.parse::<syn::Lit>()?;
    }
    Ok(false)
}

/// The contract the macros would record for `file` compiled as the root of `package`.
pub fn scan_file(file: syn::File, package: PackageInfo) -> Result<SourceContract, Error> {
    scan_items(&file.items, package)
}

fn scan_items(items: &[syn::Item], package: PackageInfo) -> Result<SourceContract, Error> {
    let mut invocations = Vec::new();
    let mut names = Names {
        krate: package.name.replace('-', "_"),
        ..Names::default()
    };
    let root = names.krate.clone();
    collect(items, &root, &mut invocations, &mut names);
    let records = invocations
        .into_iter()
        .map(|invocation| invocation.record(&names, &package))
        .collect::<Result<Vec<_>, _>>()?;
    boltffi_binding::aggregate_records(&records, package).map_err(Error::Aggregate)
}

/// The names a descriptor can resolve to, as rustc would see them in the crate.
#[derive(Default)]
struct Names {
    krate: String,
    declared: HashSet<String>,
    aliases: HashMap<String, Option<String>>,
    remotes: HashMap<String, Option<String>>,
}

impl Names {
    fn id(&self, name: &str) -> TypeNode {
        TypeNode::Id {
            id: format!("{}::{name}", self.krate),
        }
    }

    fn declaration(&self, name: String) -> Result<TypeNode, Error> {
        match self.declared.contains(&name) {
            true => Ok(self.id(&name)),
            false => Err(Error::Undeclared { name }),
        }
    }
}

/// Records `value` under `name`, which stops resolving once it holds two different values.
fn claim(table: &mut HashMap<String, Option<String>>, name: String, value: String) {
    table
        .entry(name)
        .and_modify(|held| {
            if held.as_ref() != Some(&value) {
                *held = None;
            }
        })
        .or_insert(Some(value));
}

struct Invocation {
    module: String,
    fragment: Result<(SourceFragment, Vec<SlotSource>), Error>,
}

impl Invocation {
    fn record(self, names: &Names, package: &PackageInfo) -> Result<RawSourceRecord, Error> {
        let (fragment, slots) = self.fragment?;
        Ok(RawSourceRecord {
            package: package.clone(),
            module: self.module,
            slots: slots
                .iter()
                .map(|slot| {
                    descriptor(slot, names)
                        .map(|node| serde_json::to_string(&node).expect("descriptor serializes"))
                })
                .collect::<Result<_, _>>()?,
            json: serde_json::to_vec(&fragment).expect("fragment serializes"),
        })
    }
}

fn collect(
    items: &[syn::Item],
    module: &str,
    invocations: &mut Vec<Invocation>,
    names: &mut Names,
) {
    for item in items.iter().filter(|item| active(item_attrs(item))) {
        if let syn::Item::Mod(inner) = item
            && let Some((_, content)) = &inner.content
        {
            collect(
                content,
                &format!("{module}::{}", inner.ident),
                invocations,
                names,
            );
            continue;
        }
        if let syn::Item::Type(alias) = item
            && alias.generics.params.is_empty()
        {
            let target = &alias.ty;
            claim(
                &mut names.aliases,
                alias.ident.to_string(),
                quote::quote!(#target).to_string(),
            );
        }
        let Some((kind, attribute, mut item)) = invocation(item) else {
            continue;
        };
        let refused = refusal(kind, &attribute, &item);
        configure(&mut item);
        let refused = refused.or_else(|| {
            conditional_parameter(&item)
                .then_some("BoltFFI does not support `#[cfg]` on parameters")
        });
        declare(kind, &item, names);
        invocations.push(Invocation {
            module: module.to_owned(),
            fragment: match refused {
                Some(reason) => Err(Error::Refused {
                    item: item_name(&item),
                    reason,
                }),
                None => fragment(kind, attribute, &item).map_err(|error| Error::Capture {
                    item: item_name(&item),
                    error,
                }),
            },
        });
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Data,
    DataImpl,
    Error,
    Export,
    ExportMethods,
    CustomFfi,
    CustomType,
    Pool,
}

/// The macro `item` invokes, its attribute arguments, and the item as the macro receives it:
/// without the invoking attribute.
fn invocation(item: &syn::Item) -> Option<(Kind, proc_macro2::TokenStream, syn::Item)> {
    if let syn::Item::Macro(call) = item {
        let kind = match last_segment(&call.mac.path).as_deref() {
            Some("custom_type") => Kind::CustomType,
            Some("interned_string_pool") => Kind::Pool,
            _ => return None,
        };
        return Some((kind, proc_macro2::TokenStream::new(), item.clone()));
    }
    let mut item = item.clone();
    let attrs = attrs_mut(&mut item)?;
    let (index, marker) = attrs
        .iter()
        .enumerate()
        .find_map(|(index, attr)| boltffi_marker(attr).map(|marker| (index, marker)))?;
    let attr = attrs.remove(index);
    let arguments = match &attr.meta {
        syn::Meta::List(list) => list.tokens.clone(),
        _ => proc_macro2::TokenStream::new(),
    };
    let kind = match (marker, &item) {
        ("data", syn::Item::Impl(_)) => Kind::DataImpl,
        ("data", _) => Kind::Data,
        ("error", _) => Kind::Error,
        ("export", syn::Item::Impl(_)) => Kind::ExportMethods,
        ("export", _) => Kind::Export,
        ("custom_ffi", _) => Kind::CustomFfi,
        _ => return None,
    };
    Some((kind, arguments, item))
}

/// Why the macro behind `kind` rejects `item` or the `arguments` it is given.
fn refusal(
    kind: Kind,
    arguments: &proc_macro2::TokenStream,
    item: &syn::Item,
) -> Option<&'static str> {
    let own = match kind {
        Kind::Data => "data",
        Kind::Error => "error",
        _ => "export",
    };
    let second_marker = item_attrs(item)
        .iter()
        .filter_map(boltffi_scan::owned_name)
        .any(|marker| marker != own && matches!(marker, "data" | "error" | "export"));
    match (kind, item) {
        (_, syn::Item::Union(_)) => Some("BoltFFI can't pass a union"),
        (
            Kind::Data | Kind::Error,
            syn::Item::Fn(_) | syn::Item::Trait(_) | syn::Item::Const(_),
        ) => Some("`#[data]` and `#[error]` declare a struct or enum"),
        (Kind::Data | Kind::Error, syn::Item::Enum(item)) if item.variants.is_empty() => {
            Some("a `#[data]` or `#[error]` enum needs at least one variant")
        }
        (_, syn::Item::Struct(_) | syn::Item::Enum(_)) if second_marker => {
            Some("a struct or enum is `#[data]`, `#[error]` or `#[export]`, not two of them")
        }
        (Kind::DataImpl, _) if arguments.to_string() == "impl" => None,
        (Kind::DataImpl, _) => Some("`#[data]` on an `impl` block is written `#[data(impl)]`"),
        _ if arguments.is_empty() => None,
        (Kind::Data, _) => Some("`#[data]` takes no arguments"),
        (Kind::Error, _) => Some("`#[error]` takes no arguments"),
        (Kind::ExportMethods, _) => Some("`#[export] impl` takes no arguments"),
        (Kind::Export, syn::Item::Const(_) | syn::Item::Fn(_) | syn::Item::Trait(_)) => {
            Some("`#[export]` on a constant, function or callback trait takes no arguments")
        }
        _ => None,
    }
}

/// Whether a function `item` declares has a `#[cfg]` on a parameter, which the macros refuse.
fn conditional_parameter(item: &syn::Item) -> bool {
    let conditional = |signature: &syn::Signature| {
        signature
            .inputs
            .iter()
            .flat_map(|input| match input {
                syn::FnArg::Receiver(receiver) => &receiver.attrs,
                syn::FnArg::Typed(typed) => &typed.attrs,
            })
            .any(|attr| attr.path().is_ident("cfg"))
    };
    match item {
        syn::Item::Fn(item) => conditional(&item.sig),
        syn::Item::Impl(item) => item
            .items
            .iter()
            .any(|member| matches!(member, syn::ImplItem::Fn(member) if conditional(&member.sig))),
        syn::Item::Trait(item) => item
            .items
            .iter()
            .any(|member| matches!(member, syn::TraitItem::Fn(member) if conditional(&member.sig))),
        _ => false,
    }
}

fn fragment(
    kind: Kind,
    attribute: proc_macro2::TokenStream,
    item: &syn::Item,
) -> Result<(SourceFragment, Vec<SlotSource>), ScanError> {
    use boltffi_scan::*;
    fn of<Def>(
        captured: CapturedItem<Def>,
        fragment: fn(Def) -> SourceFragment,
    ) -> (SourceFragment, Vec<SlotSource>) {
        (fragment(captured.def), captured.slots)
    }
    if let (Kind::Export, Some(declaration)) = (kind, ClassDeclaration::of(item)) {
        return Ok(of(
            capture_class(declaration, attribute)?,
            SourceFragment::Class,
        ));
    }
    Ok(match (kind, item) {
        (Kind::CustomType, syn::Item::Macro(call)) => of(
            capture_custom(call.mac.tokens.clone())?,
            SourceFragment::Custom,
        ),
        (Kind::Pool, syn::Item::Macro(call)) => {
            let pool = capture_interned_string_pool(call.mac.tokens.clone())?;
            (
                SourceFragment::InternedStringPool {
                    id: format!("{SELF_ID}::{}", pool.name),
                    values: pool.values,
                },
                Vec::new(),
            )
        }
        (Kind::CustomFfi, syn::Item::Impl(item)) => {
            of(capture_custom_ffi(item)?, SourceFragment::Custom)
        }
        (Kind::Error, syn::Item::Struct(item)) => {
            of(capture_error_struct(item)?, SourceFragment::Record)
        }
        (Kind::Error, syn::Item::Enum(item)) => of(capture_error_enum(item)?, SourceFragment::Enum),
        (_, syn::Item::Struct(item)) => of(capture_struct(item)?, SourceFragment::Record),
        (_, syn::Item::Enum(item)) => of(capture_enum(item)?, SourceFragment::Enum),
        (_, syn::Item::Fn(item)) => of(capture_function(item)?, SourceFragment::Function),
        (_, syn::Item::Trait(item)) => of(capture_trait(item)?, SourceFragment::Trait),
        (_, syn::Item::Const(item)) => of(capture_constant(item)?, SourceFragment::Constant),
        (kind, syn::Item::Impl(item)) => {
            let captured = match kind {
                Kind::ExportMethods => capture_class_methods(item),
                _ => capture_methods(item),
            }?;
            (
                SourceFragment::Methods {
                    target: captured.target,
                    spelling: captured.spelling,
                    methods: captured.methods,
                    constants: captured.constants,
                    streams: captured.streams,
                    doc: captured.doc,
                },
                captured.slots,
            )
        }
        _ => unreachable!("`invocation` yields only items a macro takes"),
    })
}

/// Records what a declaring invocation gives a `TypeDesc` impl to, as its expansion would.
fn declare(kind: Kind, item: &syn::Item, names: &mut Names) {
    match (kind, item) {
        (Kind::Data | Kind::Error | Kind::Export, syn::Item::Struct(_) | syn::Item::Enum(_))
        | (Kind::Export, syn::Item::Trait(_)) => {
            names.declared.insert(item_name(item));
        }
        (Kind::CustomType, syn::Item::Macro(call)) => {
            if let Some((name, remote)) = custom_spec(&call.mac.tokens) {
                names.declared.insert(name.clone());
                claim(&mut names.remotes, leaf_spelling(&remote), name);
            }
        }
        (Kind::Pool, syn::Item::Macro(call)) => {
            if let Ok(pool) = boltffi_scan::capture_interned_string_pool(call.mac.tokens.clone()) {
                names.declared.insert(pool.name);
            }
        }
        (Kind::CustomFfi, syn::Item::Impl(item)) => {
            if let syn::Type::Path(path) = &*item.self_ty
                && let Some(name) = last_segment(&path.path)
            {
                names.declared.insert(name.clone());
                claim(&mut names.remotes, leaf_spelling(&item.self_ty), name);
            }
        }
        _ => {}
    }
}

/// `custom_type!(pub Name, remote = Remote, ..)` as `(Name, Remote)`.
fn custom_spec(tokens: &proc_macro2::TokenStream) -> Option<(String, syn::Type)> {
    let tokens = tokens.clone().into_iter().collect::<Vec<_>>();
    let name = tokens.iter().find_map(|token| match token {
        proc_macro2::TokenTree::Ident(ident) if ident != "pub" => Some(ident.to_string()),
        _ => None,
    })?;
    let start = tokens.windows(2).position(
        |pair| matches!(&pair[0], proc_macro2::TokenTree::Ident(ident) if ident == "remote"),
    )? + 2;
    let mut depth = 0;
    let remote = tokens[start..]
        .iter()
        .take_while(|token| {
            let punct = match token {
                proc_macro2::TokenTree::Punct(punct) => punct.as_char(),
                _ => return true,
            };
            depth += match punct {
                '<' => 1,
                '>' => -1,
                _ => 0,
            };
            punct != ',' || depth > 0
        })
        .cloned()
        .collect::<proc_macro2::TokenStream>();
    Some((name, syn::parse2(remote).ok()?))
}

/// `ty` with every path cut to its last segment, the spelling a custom remote is found by.
fn leaf_spelling(ty: &syn::Type) -> String {
    let segment = match ty {
        syn::Type::Path(path) => path.path.segments.last(),
        _ => None,
    };
    let Some(segment) = segment else {
        return quote::quote!(#ty).to_string();
    };
    let arguments = match &segment.arguments {
        syn::PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .map(|argument| match argument {
                syn::GenericArgument::Type(ty) => leaf_spelling(ty),
                other => quote::quote!(#other).to_string(),
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    match arguments.is_empty() {
        true => segment.ident.to_string(),
        false => format!("{}<{}>", segment.ident, arguments.join(", ")),
    }
}

/// The descriptor rustc would compute for `slot` from the crate's `TypeDesc` impls.
fn descriptor(slot: &SlotSource, names: &Names) -> Result<TypeNode, Error> {
    match slot {
        SlotSource::Type(ty) => type_descriptor(ty, names),
        SlotSource::TraitValue(path) => names.declaration(last_segment(path).unwrap_or_default()),
    }
}

fn type_descriptor(ty: &syn::Type, names: &Names) -> Result<TypeNode, Error> {
    let unsupported = || Error::UnsupportedSlot {
        spelling: quote::quote!(#ty).to_string(),
    };
    let syn::Type::Path(path) = ty else {
        return Err(unsupported());
    };
    let segment = path.path.segments.last().ok_or_else(unsupported)?;
    let name = segment.ident.to_string();
    let spelling = leaf_spelling(ty);
    match names.remotes.get(&spelling) {
        Some(Some(custom)) if *custom == spelling || !names.declared.contains(&spelling) => {
            return Ok(names.id(custom));
        }
        Some(_) => return Err(Error::Ambiguous { name: spelling }),
        None => {}
    }
    let args = match &segment.arguments {
        syn::PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .filter_map(|argument| match argument {
                syn::GenericArgument::Type(ty) => Some(type_descriptor(ty, names)),
                _ => None,
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    const SHAPES: &[(&str, usize)] = &[
        ("Vec", 1),
        ("Option", 1),
        ("Box", 1),
        ("Arc", 1),
        ("HashMap", 2),
        ("BTreeMap", 2),
        ("Result", 2),
    ];
    if args.is_empty() && (name == "String" || Primitive::from_rust_name(&name).is_some()) {
        return Ok(TypeNode::Prim { prim: name });
    }
    if SHAPES.contains(&(name.as_str(), args.len())) {
        return Ok(TypeNode::Shape { shape: name, args });
    }
    match names.aliases.get(&name) {
        Some(Some(target)) => {
            let target = syn::parse_str::<syn::Type>(target).expect("an alias target parses");
            let renames_itself = matches!(
                &target,
                syn::Type::Path(path)
                    if path.path.segments.last().is_some_and(|segment| segment.ident == name)
            );
            match (renames_itself, names.declared.contains(&name)) {
                (true, _) => names.declaration(name),
                (false, true) => Err(Error::Ambiguous { name }),
                (false, false) => type_descriptor(&target, names),
            }
        }
        Some(None) => Err(Error::Ambiguous { name }),
        None => names.declaration(name),
    }
}

fn boltffi_marker(attribute: &syn::Attribute) -> Option<&'static str> {
    boltffi_scan::owned_name(attribute)
        .filter(|name| matches!(*name, "data" | "error" | "export" | "custom_ffi"))
}

fn last_segment(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn attrs_mut(item: &mut syn::Item) -> Option<&mut Vec<syn::Attribute>> {
    match item {
        syn::Item::Struct(item) => Some(&mut item.attrs),
        syn::Item::Enum(item) => Some(&mut item.attrs),
        syn::Item::Fn(item) => Some(&mut item.attrs),
        syn::Item::Trait(item) => Some(&mut item.attrs),
        syn::Item::Const(item) => Some(&mut item.attrs),
        syn::Item::Impl(item) => Some(&mut item.attrs),
        syn::Item::Union(item) => Some(&mut item.attrs),
        _ => None,
    }
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn item_name(item: &syn::Item) -> String {
    match item {
        syn::Item::Struct(item) => item.ident.to_string(),
        syn::Item::Enum(item) => item.ident.to_string(),
        syn::Item::Fn(item) => item.sig.ident.to_string(),
        syn::Item::Trait(item) => item.ident.to_string(),
        syn::Item::Const(item) => item.ident.to_string(),
        syn::Item::Union(item) => item.ident.to_string(),
        syn::Item::Impl(item) => {
            let ty = &item.self_ty;
            format!("impl {}", quote::quote!(#ty))
        }
        syn::Item::Macro(call) => format!("{}!", last_segment(&call.mac.path).unwrap_or_default()),
        _ => String::new(),
    }
}
