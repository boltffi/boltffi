boltffi::scaffolding!();
mod a {
    #[boltffi::export]
    pub struct C {
        n: u32,
    }
}
mod b {
    #[boltffi::export]
    pub struct C {
        n: u32,
    }
}
fn main() {}
