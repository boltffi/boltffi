boltffi::scaffolding!();

#[boltffi::export]
pub fn sum((left, right): (i32, i32)) -> i32 {
    left + right
}

fn main() {}
