use boltffi::export;

#[export(methods)]
impl crate::Cursor {
    pub fn skip(&mut self) {
        self.position += 1;
    }
}

#[export(sideways)]
impl crate::Cursor {
    pub fn back(&mut self) {
        self.position -= 1;
    }
}
