boltffi::scaffolding!();

pub type Meters = f64;

#[boltffi::export]
pub fn double(length: Meters) -> Meters {
    length * 2.0
}

fn main() {}
