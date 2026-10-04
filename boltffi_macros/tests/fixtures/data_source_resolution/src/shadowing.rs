use crate::stamps::Stamp;

#[boltffi::export]
pub fn negate(negate: i32) -> i32 {
    negate.wrapping_neg()
}

#[boltffi::export]
pub fn stamp(stamp: Stamp) -> Stamp {
    stamp
}

#[boltffi::export]
pub async fn settle(settle: u32) -> u32 {
    settle
}
