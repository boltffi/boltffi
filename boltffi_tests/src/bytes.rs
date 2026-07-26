use boltffi::*;

#[export]
pub fn byte_sum(data: Vec<u8>) -> u64 {
    data.iter().copied().map(u64::from).sum()
}

#[export]
pub fn borrowed_byte_sum(data: &[u8]) -> u64 {
    data.iter().copied().map(u64::from).sum()
}

#[export]
pub fn fill_bytes(out: &mut [u8]) -> u32 {
    out.iter_mut()
        .enumerate()
        .for_each(|(index, value)| *value = (index as u8).wrapping_mul(3).wrapping_add(1));
    out.len() as u32
}

#[export]
pub fn echo_bytes(data: Vec<u8>) -> Vec<u8> {
    data
}
