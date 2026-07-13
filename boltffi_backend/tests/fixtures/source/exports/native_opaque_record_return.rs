#[export]
pub fn make_snapshot(#[boltffi::borrowed] input: &[u8], count: u32) -> Snapshot {
    let _ = input;
    unimplemented!()
}
