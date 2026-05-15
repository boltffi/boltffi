use crate::Counter;
use boltffi::*;

#[export]
pub fn describe_counter(counter: &Counter) -> String {
    format!("Counter(value={})", counter.get())
}
