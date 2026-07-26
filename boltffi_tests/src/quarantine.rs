#[cfg(boltffi_pending_closure_return)]
use boltffi::*;

#[cfg(boltffi_pending_closure_return)]
#[export]
pub fn try_make_adder(fail: bool) -> Result<Box<dyn Fn(u32) -> u32>, String> {
    if fail {
        Err("adder unavailable".to_string())
    } else {
        Ok(Box::new(|value| value + 1))
    }
}
