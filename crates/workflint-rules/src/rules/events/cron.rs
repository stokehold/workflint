//! A port of the cron parser of robfig/cron v3.0.1 (`parser.go`, `spec.go`), which the
//! reference's `checkCron` uses, with its error texts, and of `SpecSchedule.Next`.
//!
//! The reference builds the parser with `Minute | Hour | Dom | Month | Dow`: five fields, no
//! seconds field, and no descriptors such as `@daily`. A spec may start with `TZ=` or
//! `CRON_TZ=` and a time zone name, which robfig loads with Go's `time.LoadLocation`.
//!
//! robfig computes the runs of a schedule in the host's local time zone, or in the zone of the
//! prefix. [`Schedule::interval`] computes them in UTC, so the result does not depend on the
//! host (`docs/differences.md#cron-interval-in-utc`).

use workflint_data::is_time_zone;
use workflint_syntax::strconv::atoi;

/// Go's zero `time.Time`, January 1 of year 1 at 00:00:00 UTC, in seconds since the Unix epoch.
/// robfig's `Next` returns it when a schedule does not run within five years.
const ZERO_TIME: i64 = -62_135_596_800;

/// The bit that robfig sets in a field written with `*` or `?`. It changes how the day of the
/// month and the day of the week combine.
const STAR_BIT: u64 = 1 << 63;

/// The values that a field accepts, and the names that it accepts for them.
struct Bounds {
    min: u64,
    max: u64,
    names: &'static [(&'static str, u64)],
}

const MINUTES: Bounds = Bounds {
    min: 0,
    max: 59,
    names: &[],
};

const HOURS: Bounds = Bounds {
    min: 0,
    max: 23,
    names: &[],
};

const DAYS_OF_MONTH: Bounds = Bounds {
    min: 1,
    max: 31,
    names: &[],
};

const MONTHS: Bounds = Bounds {
    min: 1,
    max: 12,
    names: &[
        ("jan", 1),
        ("feb", 2),
        ("mar", 3),
        ("apr", 4),
        ("may", 5),
        ("jun", 6),
        ("jul", 7),
        ("aug", 8),
        ("sep", 9),
        ("oct", 10),
        ("nov", 11),
        ("dec", 12),
    ],
};

const DAYS_OF_WEEK: Bounds = Bounds {
    min: 0,
    max: 6,
    names: &[
        ("sun", 0),
        ("mon", 1),
        ("tue", 2),
        ("wed", 3),
        ("thu", 4),
        ("fri", 5),
        ("sat", 6),
    ],
};

/// A parsed spec, robfig's `SpecSchedule`. Each field has bit `n` set when the field matches
/// `n`. The seconds field is always `0`, so it has no member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Schedule {
    minute: u64,
    hour: u64,
    day_of_month: u64,
    month: u64,
    day_of_week: u64,
}

/// Parses `spec` as robfig's `Parser.Parse` does with the reference's options.
///
/// The error is robfig's error text, such as `expected exactly 5 fields, found 4: [0 */3 * *]`.
pub(crate) fn parse(spec: &str) -> Result<Schedule, String> {
    if spec.is_empty() {
        return Err("empty spec string".to_owned());
    }

    let mut spec = spec;
    if let Some((prefix, prefixed)) = ["TZ=", "CRON_TZ="]
        .into_iter()
        .find_map(|prefix| Some((prefix, spec.strip_prefix(prefix)?)))
    {
        // robfig slices the name up to the first space, and panics when the spec has none
        // (`docs/differences.md#cron-time-zone-without-fields`).
        let Some((name, rest)) = prefixed.split_once(' ') else {
            return Err(format!(
                "{prefix} prefix must be followed by a space and 5 fields"
            ));
        };
        if let Err(err) = load_location(name) {
            return Err(format!("provided bad location {name}: {err}"));
        }
        spec = rest.trim();
    }

    if spec.starts_with('@') {
        return Err(format!("parser does not accept descriptors: {spec}"));
    }

    // Go's `strings.Fields` and Rust's `split_whitespace` split at the same characters.
    let fields: Vec<&str> = spec.split_whitespace().collect();
    let [minute, hour, day_of_month, month, day_of_week] = fields[..] else {
        return Err(format!(
            "expected exactly 5 fields, found {}: [{}]",
            fields.len(),
            fields.join(" ")
        ));
    };
    Ok(Schedule {
        minute: field(minute, &MINUTES)?,
        hour: field(hour, &HOURS)?,
        day_of_month: field(day_of_month, &DAYS_OF_MONTH)?,
        month: field(month, &MONTHS)?,
        day_of_week: field(day_of_week, &DAYS_OF_WEEK)?,
    })
}

/// Checks `name` as Go's `time.LoadLocation` does, with the names of the tzdata that the
/// reference embeds (`docs/differences.md#host-time-zones`). The error is Go's error text.
fn load_location(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "UTC" || name == "Local" {
        return Ok(());
    }
    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
        return Err("time: invalid location name".to_owned());
    }
    if is_time_zone(name) {
        Ok(())
    } else {
        Err(format!("unknown time zone {name}"))
    }
}

/// robfig's `getField`: the bits of a comma-separated list of ranges. Empty items are skipped.
fn field(field: &str, bounds: &Bounds) -> Result<u64, String> {
    let mut bits = 0;
    for expr in field.split(',').filter(|expr| !expr.is_empty()) {
        bits |= range(expr, bounds)?;
    }
    Ok(bits)
}

/// robfig's `getRange`: the bits of `number`, `number-number`, or `*`, with an optional
/// `/step`.
fn range(expr: &str, bounds: &Bounds) -> Result<u64, String> {
    let mut slashes = expr.split('/');
    let low_and_high = slashes.next().unwrap_or_default();
    let steps: Vec<&str> = slashes.collect();
    let mut hyphens = low_and_high.split('-');
    let low = hyphens.next().unwrap_or_default();
    let highs: Vec<&str> = hyphens.collect();

    // `*` ignores what follows a hyphen, as robfig does.
    let (start, mut end, mut extra) = if low == "*" || low == "?" {
        (bounds.min, bounds.max, STAR_BIT)
    } else {
        let start = int_or_name(low, bounds)?;
        let end = match highs[..] {
            [] => start,
            [high] => int_or_name(high, bounds)?,
            _ => return Err(format!("too many hyphens: {expr}")),
        };
        (start, end, 0)
    };

    let step = match steps[..] {
        [] => 1,
        [step] => {
            let step = parse_uint(step)?;
            // `N/step` means `N-max/step`.
            if highs.is_empty() {
                end = bounds.max;
            }
            if step > 1 {
                extra = 0;
            }
            step
        }
        _ => return Err(format!("too many slashes: {expr}")),
    };

    if start < bounds.min {
        return Err(format!(
            "beginning of range ({start}) below minimum ({}): {expr}",
            bounds.min
        ));
    }
    if end > bounds.max {
        return Err(format!(
            "end of range ({end}) above maximum ({}): {expr}",
            bounds.max
        ));
    }
    if start > end {
        return Err(format!(
            "beginning of range ({start}) beyond end of range ({end}): {expr}"
        ));
    }
    if step == 0 {
        return Err(format!("step of range should be a positive number: {expr}"));
    }
    Ok(bits(start, end, step) | extra)
}

/// robfig's `parseIntOrName`. robfig lowercases `expr` with `strings.ToLower`. No other
/// character lowercases to an ASCII letter of a name, so ASCII case folding gives the same
/// result.
fn int_or_name(expr: &str, bounds: &Bounds) -> Result<u64, String> {
    match bounds
        .names
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(expr))
    {
        Some(&(_, value)) => Ok(value),
        None => parse_uint(expr),
    }
}

/// robfig's `mustParseInt`.
fn parse_uint(expr: &str) -> Result<u64, String> {
    let number = atoi(expr).map_err(|err| format!("failed to parse int from {expr}: {err}"))?;
    u64::try_from(number).map_err(|_| format!("negative number ({number}) not allowed: {expr}"))
}

/// robfig's `getBits`: the bits from `min` to `max`, every `step`. The callers keep `max` below
/// 64.
fn bits(min: u64, max: u64, step: u64) -> u64 {
    let mut bits = 0;
    let mut value = min;
    while value <= max {
        bits |= bit(value);
        match value.checked_add(step) {
            Some(next) => value = next,
            None => break,
        }
    }
    bits
}

/// The bit for `value`, or no bit when `value` is 64 or more.
fn bit(value: u64) -> u64 {
    u32::try_from(value)
        .ok()
        .and_then(|shift| 1_u64.checked_shl(shift))
        .unwrap_or(0)
}

/// Returns whether `bits` has the bit for `value`.
fn has(bits: u64, value: u32) -> bool {
    bits & bit(u64::from(value)) != 0
}

impl Schedule {
    /// The seconds between the first two runs after the Unix epoch, as the reference's
    /// `checkCron` computes them. It is 0 when the schedule does not run within five years of
    /// the epoch, since robfig then returns Go's zero time twice.
    pub(crate) fn interval(&self) -> i64 {
        let start = self.next(0);
        self.next(start).saturating_sub(start)
    }

    /// robfig's `SpecSchedule.Next` in UTC: the first run after `t`, in seconds since the Unix
    /// epoch, or [`ZERO_TIME`] when there is none within five years.
    fn next(&self, t: i64) -> i64 {
        // Start at the next second.
        let mut t = t.saturating_add(1);
        let mut added = false;
        let year_limit = Civil::at(t).year + 5;

        'wrap: loop {
            if Civil::at(t).year > year_limit {
                return ZERO_TIME;
            }

            while !has(self.month, Civil::at(t).month) {
                let c = Civil::at(t);
                if !added {
                    added = true;
                    t = date(c.year, c.month, 1, 0, 0, 0);
                }
                let c = Civil::at(t);
                t = date(c.year, c.month + 1, c.day, c.hour, c.minute, c.second);
                if Civil::at(t).month == 1 {
                    continue 'wrap;
                }
            }

            while !self.day_matches(Civil::at(t)) {
                if !added {
                    added = true;
                    let c = Civil::at(t);
                    t = date(c.year, c.month, c.day, 0, 0, 0);
                }
                let c = Civil::at(t);
                t = date(c.year, c.month, c.day + 1, c.hour, c.minute, c.second);
                // robfig moves to midnight when a change to daylight saving time has moved
                // the hour. UTC has no such change, but the step is kept to match the port.
                let hour = i64::from(Civil::at(t).hour);
                if hour > 12 {
                    t += (24 - hour) * SECONDS_PER_HOUR;
                } else {
                    t -= hour * SECONDS_PER_HOUR;
                }
                if Civil::at(t).day == 1 {
                    continue 'wrap;
                }
            }

            while !has(self.hour, Civil::at(t).hour) {
                if !added {
                    added = true;
                    t -= t.rem_euclid(SECONDS_PER_HOUR);
                }
                t += SECONDS_PER_HOUR;
                if Civil::at(t).hour == 0 {
                    continue 'wrap;
                }
            }

            while !has(self.minute, Civil::at(t).minute) {
                if !added {
                    added = true;
                    t -= t.rem_euclid(SECONDS_PER_MINUTE);
                }
                t += SECONDS_PER_MINUTE;
                if Civil::at(t).minute == 0 {
                    continue 'wrap;
                }
            }

            // The seconds field matches only 0.
            while Civil::at(t).second != 0 {
                added = true;
                t += 1;
                if Civil::at(t).second == 0 {
                    continue 'wrap;
                }
            }

            return t;
        }
    }

    /// robfig's `dayMatches`. When either day field has `*`, both must match. Otherwise
    /// either may match.
    fn day_matches(&self, c: Civil) -> bool {
        let day_of_month = has(self.day_of_month, c.day);
        let day_of_week = has(self.day_of_week, c.weekday);
        if (self.day_of_month | self.day_of_week) & STAR_BIT != 0 {
            day_of_month && day_of_week
        } else {
            day_of_month || day_of_week
        }
    }
}

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

/// A time in UTC, split into the fields of the proleptic Gregorian calendar, as Go's `time`
/// package uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Civil {
    year: i64,
    /// 1 to 12.
    month: u32,
    /// 1 to 31.
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    /// 0 for Sunday to 6 for Saturday.
    weekday: u32,
}

impl Civil {
    /// Splits `t`, in seconds since the Unix epoch.
    fn at(t: i64) -> Self {
        let days = t.div_euclid(SECONDS_PER_DAY);
        let seconds = t.rem_euclid(SECONDS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        Self {
            year,
            month,
            day,
            hour: small(seconds / SECONDS_PER_HOUR),
            minute: small(seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE),
            second: small(seconds % SECONDS_PER_MINUTE),
            // January 1, 1970 was a Thursday.
            weekday: small((days + 4).rem_euclid(7)),
        }
    }
}

/// Converts a value that the caller keeps below `u32::MAX`.
fn small(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

/// Go's `time.Date` in UTC, in seconds since the Unix epoch. A month or day out of range
/// carries into the next field, as in Go, so month 13 is January of the next year.
fn date(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
    let month0 = i64::from(month) - 1;
    let year = year + month0.div_euclid(12);
    let month = month0.rem_euclid(12) + 1;
    let days = days_from_civil(year, month) + i64::from(day) - 1;
    days * SECONDS_PER_DAY
        + i64::from(hour) * SECONDS_PER_HOUR
        + i64::from(minute) * SECONDS_PER_MINUTE
        + i64::from(second)
}

/// The days from the Unix epoch to the first day of `month` (1 to 12) in `year`, from Howard
/// Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The year, month (1 to 12), and day (1 to 31) of the day `days` after the Unix epoch, from
/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, small(month), small(day))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn go_zero_time_is_january_1_of_year_1() {
        let c = Civil::at(ZERO_TIME);
        assert_eq!((c.year, c.month, c.day, c.hour), (1, 1, 1, 0));
        // January 1 of year 1 was a Monday in the proleptic Gregorian calendar.
        assert_eq!(c.weekday, 1);
    }

    #[test]
    fn date_carries_months_and_days_into_the_next_field() {
        assert_eq!(date(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(date(1969, 13, 1, 0, 0, 0), 0);
        assert_eq!(date(1970, 1, 32, 0, 0, 0), date(1970, 2, 1, 0, 0, 0));
        assert_eq!(date(2024, 2, 30, 0, 0, 0), date(2024, 3, 1, 0, 0, 0));
    }

    #[test]
    fn civil_round_trips_every_day_across_leap_years() {
        for days in -800_000..800_000 {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(
                days_from_civil(year, i64::from(month)) + i64::from(day) - 1,
                days
            );
        }
    }

    #[test]
    fn next_finds_the_first_run_after_the_given_time() {
        let at = |spec: &str, t: i64| parse(spec).unwrap().next(t);
        // 1970-01-01 is a Thursday.
        assert_eq!(at("0 0 * * *", 0), SECONDS_PER_DAY);
        assert_eq!(at("*/5 * * * *", 0), 300);
        assert_eq!(at("30 5 * * 1-5", 0), 5 * SECONDS_PER_HOUR + 30 * 60);
        assert_eq!(at("0 0 * * sun", 0), 3 * SECONDS_PER_DAY);
        assert_eq!(at("0 0 1 3 *", 0), date(1970, 3, 1, 0, 0, 0));
        assert_eq!(at("0 0 29 2 *", 0), date(1972, 2, 29, 0, 0, 0));
        // Both day fields without `*`: either may match.
        assert_eq!(at("0 0 15 * mon", 0), 4 * SECONDS_PER_DAY);
        assert_eq!(at("0 0 30 2 *", 0), ZERO_TIME);
        assert_eq!(at("0 0 30 2 *", ZERO_TIME), ZERO_TIME);
    }

    #[test]
    fn interval_is_between_the_first_two_runs_after_the_epoch() {
        let interval = |spec: &str| parse(spec).unwrap().interval();
        assert_eq!(interval("*/4 * * * *"), 240);
        assert_eq!(interval("0 0 * * *"), SECONDS_PER_DAY);
        assert_eq!(interval("0,1 0 * * *"), SECONDS_PER_DAY - 60);
        assert_eq!(interval(", * * * *"), 0);
    }
}
