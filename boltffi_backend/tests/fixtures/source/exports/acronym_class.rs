pub struct GPSSimulator {
    enabled: bool,
}

#[export]
impl GPSSimulator {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
