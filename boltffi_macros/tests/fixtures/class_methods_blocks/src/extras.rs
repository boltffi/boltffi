pub mod by_path {
    use boltffi::export;

    #[export(methods)]
    impl crate::Counter {
        pub async fn settled(&self) -> i32 {
            self.get()
        }

        pub fn twin(&self) -> crate::Counter {
            crate::Counter::starting_at(self.get())
        }
    }
}

pub mod by_import {
    use std::sync::Arc;

    use boltffi::{EventSubscription, export};

    use crate::Counter;

    #[export(methods)]
    impl Counter {
        pub fn reset(&self) {
            *self.value.lock().expect("counter lock") = 0;
        }

        #[cfg(any())]
        pub fn absent(&self) -> Missing {
            unreachable!()
        }

        #[boltffi::ffi_stream(item = i32)]
        pub fn resets(&self) -> Arc<EventSubscription<i32>> {
            self.changes.subscribe()
        }
    }
}
