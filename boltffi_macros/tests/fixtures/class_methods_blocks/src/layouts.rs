use boltffi::export;

#[export]
impl engine::Engine {
    pub fn warm(&self) -> bool {
        self.rpm() > 0
    }
}

pub mod engine {
    use std::sync::atomic::{AtomicU32, Ordering};

    use boltffi::export;

    #[export]
    pub struct Engine {
        rpm: AtomicU32,
    }

    #[export]
    impl Engine {
        pub fn new() -> Self {
            Self {
                rpm: AtomicU32::new(0),
            }
        }

        pub fn rpm(&self) -> u32 {
            self.rpm.load(Ordering::Relaxed)
        }
    }

    pub mod tuning {
        use std::sync::atomic::Ordering;

        use boltffi::export;

        #[export]
        impl super::Engine {
            pub fn boost(&self, by: u32) {
                self.rpm.fetch_add(by, Ordering::Relaxed);
            }
        }
    }
}

pub mod ffi {
    use boltffi::export;

    use crate::layouts::engine::Engine as Motor;

    #[export]
    impl Motor {
        pub fn idle(&self) -> bool {
            self.rpm() == 0
        }
    }

    #[cfg(any())]
    #[export]
    impl Motor {
        pub fn absent(&self) -> Missing {
            unreachable!()
        }
    }

    #[cfg(all())]
    #[export]
    impl Motor {
        pub fn present(&self) -> bool {
            true
        }
    }
}

pub mod api {
    use boltffi::export;

    #[export]
    pub fn engine_rpm(engine: &crate::layouts::engine::Engine) -> u32 {
        engine.rpm()
    }
}

macro_rules! named_class {
    ($name:ident) => {
        #[export]
        pub struct $name;

        #[export]
        impl $name {
            pub fn new() -> Self {
                Self
            }

            pub fn name(&self) -> String {
                stringify!($name).to_owned()
            }
        }
    };
}

named_class!(Generated);
