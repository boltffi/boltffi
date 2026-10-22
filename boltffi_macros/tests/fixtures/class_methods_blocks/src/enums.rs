use boltffi::export;

#[export]
impl gearbox::Gear {
    pub fn moving(&self) -> bool {
        self.speed() > 0
    }
}

pub mod gearbox {
    use boltffi::export;

    #[export(single_threaded)]
    pub enum Gear {
        Park,
        Drive(u32),
    }

    #[export]
    impl Gear {
        pub fn new() -> Self {
            Self::Park
        }

        pub fn shift(&mut self, speed: u32) {
            *self = Self::Drive(speed);
        }
    }

    pub mod readout {
        use boltffi::export;

        use super::Gear;

        #[export]
        impl Gear {
            pub fn speed(&self) -> u32 {
                match self {
                    Gear::Park => 0,
                    Gear::Drive(speed) => *speed,
                }
            }
        }
    }
}

#[export]
pub enum Signal {
    Off,
    On(u8),
    #[cfg(any())]
    Blink(Missing),
}

#[export]
impl Signal {
    pub fn new(level: u8) -> Self {
        match level {
            0 => Self::Off,
            level => Self::On(level),
        }
    }

    pub fn follows(&self, gear: &gearbox::Gear) -> bool {
        matches!(self, Self::On(_)) == gear.moving()
    }
}

pub fn every_enum_block_exports_its_members() -> [*const (); 6] {
    [
        gearbox::boltffi_init_class_class_methods_blocks_gear_new as *const (),
        gearbox::boltffi_method_class_class_methods_blocks_gear_shift as *const (),
        gearbox::readout::boltffi_method_class_class_methods_blocks_gear_speed as *const (),
        boltffi_method_class_class_methods_blocks_gear_moving as *const (),
        boltffi_init_class_class_methods_blocks_signal_new as *const (),
        boltffi_method_class_class_methods_blocks_signal_follows as *const (),
    ]
}
