use boltffi::*;

use crate::FixtureStatus;

#[export]
pub const FIXTURE_LIMIT: u32 = 42;

#[export]
pub const FIXTURE_LABEL: &str = "fixture";

#[export]
pub const FIXTURE_DEFAULT_STATUS: FixtureStatus = FixtureStatus::Pending;
