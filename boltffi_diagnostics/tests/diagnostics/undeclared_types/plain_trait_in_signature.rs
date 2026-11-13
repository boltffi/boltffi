boltffi::scaffolding!();

pub trait Listener {
    fn on_event(&self, code: u32);
}

#[boltffi::export]
pub fn notify(listener: Box<dyn Listener>) {
    listener.on_event(1);
}

fn main() {}
