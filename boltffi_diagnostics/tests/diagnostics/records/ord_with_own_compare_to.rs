boltffi::scaffolding!();
#[boltffi::data(ord)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank { pub tier: u32 }
#[boltffi::data(impl)]
impl Rank {
    pub fn compare_to(&self, other: Rank) -> i32 { self.tier as i32 - other.tier as i32 }
}
fn main() {}
