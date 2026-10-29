use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use boltffi_binding::{RawSourceRecord, RecordStore, SELF_ID, SourceFragment, TypeNode};
use boltffi_scan::SlotSource;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{
    Build, Entry, REPR_ARG_OF, Reached, Reference, crate_name, lane_path, lane_paths, raw_json,
    selected_id,
};
use crate::capture::{self, Fragments};

pub struct Store {
    path: PathBuf,
    records: Vec<RawSourceRecord>,
    by_fragment: HashMap<String, usize>,
    by_declaration: HashMap<String, usize>,
}

pub struct Resolution {
    pub references: Vec<Reference>,
    pub checks: TokenStream,
}

thread_local! {
    static LOADED: RefCell<HashMap<PathBuf, Option<&'static Store>>> = RefCell::new(HashMap::new());
}

impl Store {
    pub fn find() -> Option<&'static Self> {
        if Build::current() == Build::SourceRecords {
            return None;
        }
        let path = Path::new(env!("OUT_DIR"))
            .ancestors()
            .map(|directory| directory.join(RecordStore::RELATIVE_PATH))
            .find(|path| path.is_file())?;
        LOADED.with(|loaded| {
            *loaded
                .borrow_mut()
                .entry(path.clone())
                .or_insert_with(|| Self::read(path).map(|store| &*Box::leak(Box::new(store))))
        })
    }

    fn read(path: PathBuf) -> Option<Self> {
        let records = RecordStore::from_json(&std::fs::read_to_string(&path).ok()?)?.records();
        let by_fragment = records
            .iter()
            .enumerate()
            .map(|(index, record)| (String::from_utf8_lossy(&record.json).into_owned(), index))
            .collect();
        let by_declaration = records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| Some((declared_id(record)?, index)))
            .collect();
        Some(Self {
            path,
            records,
            by_fragment,
            by_declaration,
        })
    }
}

impl Store {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn resolve(&self, fragments: &Fragments) -> Option<Result<Resolution, String>> {
        let records = fragments
            .iter()
            .map(|(fragment, _)| self.record(fragment))
            .collect::<Option<Vec<_>>>()?;
        Some(self.resolution(fragments, &records))
    }

    pub fn stale_warning(&self, fragments: &Fragments) -> TokenStream {
        let Ok(krate) = crate_name() else {
            return TokenStream::new();
        };
        fragments
            .iter()
            .filter(|(fragment, _)| self.record(fragment).is_none())
            .filter_map(|(fragment, _)| selected_id(fragment, &krate, || None))
            .filter(|id| self.by_declaration.contains_key(id))
            .map(|id| {
                let name = id.rsplit("::").next().unwrap_or(&id);
                let note = format!(
                    "BoltFFI: `{name}` changed since `boltffi generate`, so the types it names \
                     resolve through their lanes until `boltffi generate` runs again"
                );
                quote! {
                    const _: () = {
                        #[deprecated(note = #note)]
                        const STALE: () = ();
                        STALE
                    };
                }
            })
            .collect()
    }

    fn record(&self, fragment: &SourceFragment) -> Option<&RawSourceRecord> {
        let json = serde_json::to_string(fragment).ok()?;
        self.by_fragment
            .get(&json)
            .map(|index| &self.records[*index])
    }

    fn resolution(
        &self,
        fragments: &Fragments,
        records: &[&RawSourceRecord],
    ) -> Result<Resolution, String> {
        let krate = crate_name()?;
        let alias = format!("{}::__private::{REPR_ARG_OF}", capture::facade());
        let references = lane_paths(fragments)
            .iter()
            .map(|path| {
                let path = path.to_string();
                let descriptor = fragments
                    .iter()
                    .zip(records)
                    .find_map(|((_, slots), record)| {
                        slots
                            .iter()
                            .position(|slot| lane_path(slot).to_string() == path)
                            .and_then(|index| record.slots.get(index))
                    })
                    .ok_or_else(|| format!("the record store resolves no `{path}`"))?;
                let node = type_node(descriptor)?;
                let mut reached = Vec::new();
                declarations(&node, &mut Vec::new(), &mut |id, positions| {
                    let path = positions.iter().fold(path.clone(), |container, position| {
                        format!("{alias}<{container}, {position}>")
                    });
                    reached.push(Reached {
                        tag: crate_tag(id, &krate).to_string(),
                        id: id.to_owned(),
                        path,
                    });
                });
                let mut seen = HashSet::new();
                let entries = reached
                    .iter()
                    .try_fold(Vec::new(), |mut entries, reached| {
                        entries.extend(self.entries(&reached.id, &mut seen)?);
                        Ok::<_, String>(entries)
                    })?;
                Ok(Reference {
                    descriptor: raw_json(descriptor.clone())?,
                    path,
                    reached,
                    entries,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let checks = fragments
            .iter()
            .zip(records)
            .flat_map(|((_, slots), record)| slots.iter().zip(&record.slots))
            .map(|(slot, descriptor)| resolution_check(slot, descriptor))
            .collect();
        Ok(Resolution { references, checks })
    }

    /// A declaration's entry, resolved as rustc resolved it, and the entries of every
    /// declaration its slots reach, so no copy of it classifies with its slots unresolved.
    fn entries(&self, id: &str, seen: &mut HashSet<String>) -> Result<Vec<Entry>, String> {
        if !seen.insert(id.to_owned()) {
            return Ok(Vec::new());
        }
        let record = self
            .by_declaration
            .get(id)
            .map(|index| &self.records[*index])
            .ok_or_else(|| format!("the record store declares no `{id}`"))?;
        let own = Entry {
            module: crate_of(record).to_owned(),
            slots: record
                .slots
                .iter()
                .map(|descriptor| raw_json(descriptor.clone()))
                .collect::<Result<Vec<_>, _>>()?,
            json: raw_json(String::from_utf8_lossy(&record.json).into_owned())?,
            lookup: false,
        };
        let mut reached = Vec::new();
        record.slots.iter().try_for_each(|descriptor| {
            declarations(
                &type_node(descriptor)?,
                &mut Vec::new(),
                &mut |slot_id, _| {
                    reached.push(slot_id.to_owned());
                },
            );
            Ok::<_, String>(())
        })?;
        reached.iter().try_fold(vec![own], |mut entries, slot_id| {
            entries.extend(self.entries(slot_id, seen)?);
            Ok(entries)
        })
    }
}

pub fn crate_tag(id: &str, krate: &str) -> TokenStream {
    match id.split_once("::").map(|(owner, _)| owner) {
        Some(owner) if owner != krate => {
            let owner = format_ident!("{owner}");
            quote! { ::#owner::__BoltffiTag }
        }
        _ => quote! { crate::__BoltffiTag },
    }
}

fn resolution_check(slot: &SlotSource, descriptor: &str) -> TokenStream {
    let facade = capture::facade();
    let found = match slot {
        SlotSource::Type(ty) => quote! {
            <#ty as #facade::__private::capture::TypeDesc<_>>::DESC
        },
        SlotSource::TraitValue(path) => quote! {
            #facade::__private::capture::trait_desc(&#path)
        },
    };
    let message = format!(
        "BoltFFI: `{}` names a different or changed declaration than when bindings were \
         generated; run `boltffi generate`",
        lane_path(slot)
    );
    quote! {
        const _: () = #facade::__private::capture::expect_descriptor(
            #found.as_str(),
            #descriptor,
            #message,
        );
    }
}

fn fragment_of(record: &RawSourceRecord) -> Option<SourceFragment> {
    serde_json::from_slice(&record.json).ok()
}

fn crate_of(record: &RawSourceRecord) -> &str {
    record.module.split("::").next().unwrap_or(&record.module)
}

fn declared_id(record: &RawSourceRecord) -> Option<String> {
    let krate = crate_of(record);
    match fragment_of(record)? {
        SourceFragment::InternedStringPool { id, .. } => id
            .strip_prefix(SELF_ID)
            .map(|rest| format!("{krate}{rest}")),
        fragment => selected_id(&fragment, krate, || None),
    }
}

fn type_node(descriptor: &str) -> Result<TypeNode, String> {
    serde_json::from_str(descriptor).map_err(|error| error.to_string())
}

/// Visits each declaration a slot descriptor names, depth first, with the argument
/// positions that lead to it from the slot's own type.
fn declarations(
    node: &TypeNode,
    positions: &mut Vec<usize>,
    visit: &mut impl FnMut(&str, &[usize]),
) {
    match node {
        TypeNode::Id { id } => visit(&crate_scoped(id), positions),
        TypeNode::Prim { .. } => {}
        TypeNode::Shape { args, .. } => args.iter().enumerate().for_each(|(position, arg)| {
            positions.push(position);
            declarations(arg, positions, visit);
            positions.pop();
        }),
    }
}

fn crate_scoped(id: &str) -> String {
    match (id.split_once("::"), id.rsplit_once("::")) {
        (Some((krate, _)), Some((_, name))) => format!("{krate}::{name}"),
        _ => id.to_owned(),
    }
}
