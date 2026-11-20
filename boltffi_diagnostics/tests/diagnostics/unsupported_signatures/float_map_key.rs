boltffi::scaffolding!();

use std::collections::HashMap;

#[boltffi::export]
pub fn histogram(samples: Vec<f64>) -> HashMap<f64, u32> {
    let _ = samples;
    HashMap::new()
}

fn main() {}
