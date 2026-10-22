#[boltffi::data(impl)]
impl crate::GeographicCoordinate {
    pub fn origin() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
            #[cfg(feature = "experimental")]
            altitude: 0.0,
        }
    }
}

#[boltffi::data(impl)]
impl crate::GeographicCoordinate {
    pub fn latitude(&self) -> f64 {
        self.latitude
    }
}

pub mod nested {
    #[boltffi::data(impl)]
    impl super::super::GeographicCoordinate {
        pub fn longitude(&self) -> f64 {
            self.longitude
        }
    }
}
