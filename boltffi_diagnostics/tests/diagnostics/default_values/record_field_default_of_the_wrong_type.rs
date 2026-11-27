boltffi::scaffolding!();
#[boltffi::data]
#[derive(Clone)]
pub struct Config {
    #[boltffi::default(1.5)]
    pub verbose: bool,
}
fn main() {}
