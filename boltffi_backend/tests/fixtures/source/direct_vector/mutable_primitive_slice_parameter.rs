#[export]
pub fn increment(values: &mut [u64]) {
    values.iter_mut().for_each(|value| *value += 1);
}
