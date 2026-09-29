custom_type!(
    pub Limit,
    remote = LimitRust,
    repr = Option<u32>,
    into_ffi = limit_into_ffi,
    try_from_ffi = limit_from_ffi
);

#[data]
pub struct Quota {
    #[boltffi::default(None)]
    pub limit: LimitRust,
}

#[export]
pub fn throttle(#[boltffi::default(None)] limit: LimitRust) -> LimitRust {
    limit
}
