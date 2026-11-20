use boltffi::export;

#[export(single_threaded)]
impl crate::Cursor {
    pub fn rewind(&mut self) {
        self.position = 0;
    }
}

#[export(single_threaded)]
impl crate::enums::gearbox::Gear {
    pub fn park(&mut self) {
        *self = Self::Park;
    }
}
