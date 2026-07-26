#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const NANOS_PER_SECOND: i128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DurationWireValue {
    pub(crate) seconds: u64,
    pub(crate) nanos: u32,
}

impl From<Duration> for DurationWireValue {
    fn from(duration: Duration) -> Self {
        Self {
            seconds: duration.as_secs(),
            nanos: duration.subsec_nanos(),
        }
    }
}

impl DurationWireValue {
    pub(crate) fn is_valid(self) -> bool {
        self.nanos < NANOS_PER_SECOND as u32
    }

    pub(crate) fn into_duration(self) -> Option<Duration> {
        self.is_valid()
            .then(|| Duration::new(self.seconds, self.nanos))
    }

    pub(crate) fn write_to(self, buffer: &mut [u8]) -> usize {
        buffer[..8].copy_from_slice(&self.seconds.to_le_bytes());
        buffer[8..12].copy_from_slice(&self.nanos.to_le_bytes());
        12
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EpochTimestampWireValue {
    pub(crate) seconds: i64,
    pub(crate) nanos: u32,
}

impl From<SystemTime> for EpochTimestampWireValue {
    fn from(system_time: SystemTime) -> Self {
        let total_nanos = match system_time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                (duration.as_secs() as i128) * NANOS_PER_SECOND + duration.subsec_nanos() as i128
            }
            Err(error) => {
                let duration = error.duration();
                -((duration.as_secs() as i128) * NANOS_PER_SECOND + duration.subsec_nanos() as i128)
            }
        };

        Self {
            seconds: total_nanos.div_euclid(NANOS_PER_SECOND) as i64,
            nanos: total_nanos.rem_euclid(NANOS_PER_SECOND) as u32,
        }
    }
}

#[cfg(feature = "chrono")]
impl From<DateTime<Utc>> for EpochTimestampWireValue {
    fn from(date_time: DateTime<Utc>) -> Self {
        Self {
            seconds: date_time.timestamp(),
            nanos: date_time.timestamp_subsec_nanos(),
        }
    }
}

impl EpochTimestampWireValue {
    pub(crate) fn is_valid(self) -> bool {
        self.nanos < NANOS_PER_SECOND as u32
    }

    pub(crate) fn into_system_time(self) -> Option<SystemTime> {
        if !self.is_valid() {
            return None;
        }
        let total_nanos = (self.seconds as i128) * NANOS_PER_SECOND + self.nanos as i128;

        if total_nanos >= 0 {
            let duration = Duration::new(
                (total_nanos / NANOS_PER_SECOND) as u64,
                (total_nanos % NANOS_PER_SECOND) as u32,
            );
            UNIX_EPOCH.checked_add(duration)
        } else {
            let absolute_nanos = (-total_nanos) as u128;
            let absolute_seconds = (absolute_nanos / NANOS_PER_SECOND as u128) as u64;
            let absolute_fraction = (absolute_nanos % NANOS_PER_SECOND as u128) as u32;
            UNIX_EPOCH.checked_sub(Duration::new(absolute_seconds, absolute_fraction))
        }
    }

    #[cfg(feature = "chrono")]
    pub(crate) fn into_date_time_utc(self) -> Option<DateTime<Utc>> {
        self.is_valid()
            .then(|| DateTime::from_timestamp(self.seconds, self.nanos))
            .flatten()
    }

    pub(crate) fn write_to(self, buffer: &mut [u8]) -> usize {
        buffer[..8].copy_from_slice(&self.seconds.to_le_bytes());
        buffer[8..12].copy_from_slice(&self.nanos.to_le_bytes());
        12
    }
}
