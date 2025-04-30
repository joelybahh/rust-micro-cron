//! Time-zone helpers shared across the crate.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Australia::Sydney;
use chrono_tz::Tz;

/// Return the current instant expressed in the Australia/Sydney zone.
///
/// Always call this (not `Utc::now()`) when advancing cron schedules
/// so jobs fire in local Sydney time.
pub fn now_sydney() -> DateTime<Tz> {
    let utc = Utc::now();
    Sydney.from_utc_datetime(&utc.naive_utc())
}
