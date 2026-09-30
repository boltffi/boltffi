boltffi::scaffolding!();

#[boltffi::data]
#[derive(Clone, Copy)]
pub enum SystemTime {
    Utc,
    Local,
}

fn main() {}
