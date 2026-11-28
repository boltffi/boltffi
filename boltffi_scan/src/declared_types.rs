use std::cell::RefCell;
use std::collections::HashMap;

use crate::capture::SlotSource;
use crate::spelling;

/// The named type references one macro invocation makes, each deferred to a slot whose
/// descriptor the compiler resolves.
#[derive(Default)]
pub(super) struct DeclaredTypes {
    slots: RefCell<SlotTable>,
}

#[derive(Default)]
struct SlotTable {
    sources: Vec<SlotSource>,
    by_spelling: HashMap<String, usize>,
}

impl DeclaredTypes {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn defer_slot(&self, path: &syn::Path) -> usize {
        self.defer_slot_keyed(spelling::path(path), || {
            SlotSource::Type(Box::new(syn::Type::Path(syn::TypePath {
                attrs: Vec::new(),
                qself: None,
                path: path.clone(),
            })))
        })
    }

    /// Defers a trait bound's path as a value-namespace slot, so the descriptor
    /// resolves through the const the trait's definition site emits.
    pub(super) fn defer_trait_slot(&self, path: &syn::Path) -> usize {
        self.defer_slot_keyed(format!("trait {}", spelling::path(path)), || {
            SlotSource::TraitValue(path.clone())
        })
    }

    fn defer_slot_keyed(&self, spelling: String, source: impl FnOnce() -> SlotSource) -> usize {
        let mut slots = self.slots.borrow_mut();
        if let Some(index) = slots.by_spelling.get(&spelling) {
            return *index;
        }
        let index = slots.sources.len();
        slots.sources.push(source());
        slots.by_spelling.insert(spelling, index);
        index
    }

    pub(super) fn take_slots(&mut self) -> Vec<SlotSource> {
        std::mem::take(self.slots.get_mut()).sources
    }
}
