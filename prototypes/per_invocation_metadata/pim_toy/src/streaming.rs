//! A stream: subscribed through the export itself, drained through its generated pop export.

use pim_macros::export;
use pim_runtime::PimStream;

#[export]
pub fn countdown(from: u32) -> PimStream<u32> {
    PimStream::from_iterator((1..=from).rev())
}
