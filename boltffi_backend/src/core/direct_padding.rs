use boltffi_binding::{DirectRecordDecl, Surface};

use super::error::{Error, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DirectPadding {
    before: Vec<u64>,
    trailing: u64,
}

impl DirectPadding {
    pub(crate) fn of<S: Surface>(record: &DirectRecordDecl<S>) -> Result<Self> {
        let layout = record.layout();
        if layout.fields().len() != record.fields().len() {
            return Err(Self::broken(
                "direct record field count differs from its layout",
            ));
        }
        let mut end = 0_u64;
        let mut before = Vec::with_capacity(record.fields().len());
        for (field, field_layout) in record.fields().iter().zip(layout.fields()) {
            if field.key() != field_layout.key() {
                return Err(Self::broken("direct record field layout mismatch"));
            }
            let offset = field_layout.offset().get();
            before.push(
                offset
                    .checked_sub(end)
                    .ok_or_else(|| Self::broken("direct record field layout moves backwards"))?,
            );
            end = offset + field.ty().primitive().byte_size::<S>().get();
        }
        let trailing = layout
            .size()
            .get()
            .checked_sub(end)
            .ok_or_else(|| Self::broken("direct record size is smaller than its fields"))?;
        Ok(Self { before, trailing })
    }

    pub(crate) fn before(&self, index: usize) -> u64 {
        self.before.get(index).copied().unwrap_or(0)
    }

    pub(crate) fn trailing(&self) -> u64 {
        self.trailing
    }

    fn broken(invariant: &'static str) -> Error {
        Error::BrokenBridgeContract {
            bridge: "direct record layout",
            invariant,
        }
    }
}
