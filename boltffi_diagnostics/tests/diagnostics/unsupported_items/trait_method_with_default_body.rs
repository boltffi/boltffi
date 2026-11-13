boltffi::scaffolding!();

#[boltffi::export]
pub trait Listener {
    fn on_event(&self, code: u32) {
        let _ = code;
    }
}

fn main() {}
