#[cfg(feature = "extras")]
#[pim_macros::data]
pub struct Extra {
    pub enabled: bool,
}

#[cfg(feature = "extras")]
#[pim_macros::export]
pub fn extra_ping(enabled: bool) -> bool {
    !enabled
}
