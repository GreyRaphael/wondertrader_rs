use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Result, WtCoreError};

/// Nanoseconds since Unix epoch in UTC.
pub type TsNs = i64;

pub fn system_time_to_ts_ns(time: SystemTime) -> Result<TsNs> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WtCoreError::TimeBeforeUnixEpoch)?;
    Ok(duration_to_ts_ns(duration))
}

pub fn unix_ts_ns_now() -> TsNs {
    system_time_to_ts_ns(SystemTime::now()).expect("system clock must be after unix epoch")
}

fn duration_to_ts_ns(duration: Duration) -> TsNs {
    duration.as_secs() as TsNs * 1_000_000_000 + duration.subsec_nanos() as TsNs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_unix_epoch() {
        assert_eq!(system_time_to_ts_ns(UNIX_EPOCH).unwrap(), 0);
    }

    #[test]
    fn converts_seconds_and_nanos() {
        let time = UNIX_EPOCH + Duration::new(2, 1_000_000);
        assert_eq!(system_time_to_ts_ns(time).unwrap(), 2_001_000_000);
    }
}
