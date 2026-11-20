boltffi::scaffolding!();

#[boltffi::export]
pub fn scale(value: f64, #[cfg(unix)] factor: f64) -> f64 {
    value * factor
}

fn main() {}
