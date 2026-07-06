#[boltffi::export]
pub fn make_counter() -> impl Fn(u32) -> u32 {
    move |value| value + 1
}
