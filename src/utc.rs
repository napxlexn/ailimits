// utc.rs — instants in UTC: the clock, and the exact text the app reads them
// from and writes them in (provider answers, its own files, hook variables,
// file names). The app never uses a local time zone.

use chrono::{DateTime, DurationRound, NaiveDate, SecondsFormat, TimeDelta, Utc};

/// An instant in UTC.
pub type Timestamp = DateTime<Utc>;

/// The current instant.
pub fn now() -> Timestamp {
    Utc::now()
}

/// The instant `secs` whole seconds after the Unix epoch.
pub fn from_unix_secs(secs: i64) -> Option<Timestamp> {
    DateTime::from_timestamp(secs, 0)
}

/// The instant `ms` milliseconds after the Unix epoch.
pub fn from_unix_millis(ms: i64) -> Option<Timestamp> {
    DateTime::from_timestamp_millis(ms)
}

/// Whole seconds since the Unix epoch, rounded down.
pub fn unix_secs(t: Timestamp) -> i64 {
    t.timestamp()
}

/// An RFC 3339 instant as the providers send them: "2026-11-16T10:00:00Z",
/// "…T12:00:00.123+02:00".
pub fn parse_rfc3339(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// A "YYYY-MM-DD" date, at midnight UTC.
pub fn parse_date(text: &str) -> Option<Timestamp> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|t| DateTime::from_naive_utc_and_offset(t, Utc))
}

/// RFC 3339 with "+00:00" and as many fractional digits as it needs (0, 3,
/// 6 or 9): `AILIMITS_RESET_AT`.
pub fn to_rfc3339(t: Timestamp) -> String {
    t.to_rfc3339()
}

/// RFC 3339 in whole seconds with "Z": the crash log.
pub fn to_rfc3339_secs(t: Timestamp) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// "20261116-100000": a copy of the config kept aside.
pub fn file_stamp(t: Timestamp) -> String {
    t.format("%Y%m%d-%H%M%S").to_string()
}

/// "2026-11-16 10:00": what `ailimits-auth status` prints.
pub fn minute_text(t: Timestamp) -> String {
    t.format("%Y-%m-%d %H:%M").to_string()
}

/// Rounded down to the minute; left as it is when it lies beyond the range
/// of nanoseconds an i64 counts from the epoch (1677-09-21 to 2262-04-11).
pub fn trunc_to_minute(t: Timestamp) -> Timestamp {
    t.duration_trunc(TimeDelta::minutes(1)).unwrap_or(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        parse_rfc3339(text).unwrap_or_else(|| panic!("{text} does not read"))
    }

    /// Provider answers: what reads, as which instant (written back with
    /// `to_rfc3339`, which keeps every nanosecond), and its epoch second.
    #[test]
    fn rfc3339_answers_read_to_the_nanosecond() {
        let cases = [
            // Grok's auth.json: nine fractional digits.
            (
                "2026-11-09T05:43:16.908847600Z",
                "2026-11-09T05:43:16.908847600+00:00",
                1_794_202_996,
            ),
            // Cursor: milliseconds.
            (
                "2026-11-16T23:01:28.407Z",
                "2026-11-16T23:01:28.407+00:00",
                1_794_870_088,
            ),
            (
                "2026-11-16T10:00:00Z",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T10:00:00.000Z",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T10:00:00.5Z",
                "2026-11-16T10:00:00.500+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T10:00:00.123456Z",
                "2026-11-16T10:00:00.123456+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T10:00:00.1234567Z",
                "2026-11-16T10:00:00.123456700+00:00",
                1_794_823_200,
            ),
            // Past nine digits the rest is dropped, not rounded.
            (
                "2026-11-16T10:00:00.1234567899Z",
                "2026-11-16T10:00:00.123456789+00:00",
                1_794_823_200,
            ),
            // Offsets are taken off: every instant is UTC.
            (
                "2026-11-16T12:00:00+02:00",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T04:30:00-05:30",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T12:00:00.5+02:00",
                "2026-11-16T10:00:00.500+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16T10:00:00-00:00",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            // U+2212 MINUS SIGN.
            (
                "2026-11-16T08:00:00\u{2212}02:00",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-17T09:59:00+23:59",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            // Lower case and a space are allowed.
            (
                "2026-11-16t10:00:00z",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            (
                "2026-11-16 10:00:00Z",
                "2026-11-16T10:00:00+00:00",
                1_794_823_200,
            ),
            ("1970-01-01T00:00:00Z", "1970-01-01T00:00:00+00:00", 0),
            (
                "2100-01-01T00:00:00Z",
                "2100-01-01T00:00:00+00:00",
                4_102_444_800,
            ),
            (
                "2024-02-29T23:59:59Z",
                "2024-02-29T23:59:59+00:00",
                1_709_251_199,
            ),
            // Before the epoch the second rounds down.
            (
                "1969-12-31T23:59:59.250Z",
                "1969-12-31T23:59:59.250+00:00",
                -1,
            ),
            (
                "0000-01-01T00:00:00+01:00",
                "-0001-12-31T23:00:00+00:00",
                -62_167_222_800,
            ),
            (
                "9999-01-01T00:00:00Z",
                "9999-01-01T00:00:00+00:00",
                253_370_764_800,
            ),
        ];
        for (text, written, secs) in cases {
            let t = at(text);
            assert_eq!(to_rfc3339(t), written, "{text}");
            assert_eq!(unix_secs(t), secs, "{text}");
        }
    }

    #[test]
    fn rfc3339_refuses_what_is_not_strictly_rfc3339() {
        for text in [
            "",
            "2026-11-16",
            "2026-11-16T10:00Z",
            "2026-11-16T10:00:00",
            "2026-11-16T10:00:00+0200",
            "2026-11-16T10:00:00+02",
            "2026-11-16T10:00:00+24:00",
            "2026-11-16T10:00:00+02:60",
            "2026-11-16T10:00:00+02:00:00",
            "2026-11-16T10:00:00 Z",
            "2026-11-16T10:00:00Z ",
            " 2026-11-16T10:00:00Z",
            "2026-11-16T10:00:00Z[UTC]",
            "2026-11-16T10:00:00UTC",
            "2026-11-16T10:00:00,5Z",
            "2026-11-16T10:00:00.Z",
            "20261116T100000Z",
            "+2026-11-16T10:00:00Z",
            "2026-1-16T10:00:00Z",
            "2026-02-30T00:00:00Z",
            "2026-11-16T24:00:00Z",
            "2026-11-16T10:60:00Z",
            "2026-11-16T10:00:61Z",
            "2026-11-16X10:00:00Z",
        ] {
            assert_eq!(parse_rfc3339(text), None, "{text:?}");
        }
    }

    #[test]
    fn epoch_numbers_read_as_instants() {
        let text = |t: Option<Timestamp>| t.map(to_rfc3339);
        assert_eq!(
            text(from_unix_secs(0)).as_deref(),
            Some("1970-01-01T00:00:00+00:00")
        );
        assert_eq!(
            text(from_unix_secs(4_102_444_800)).as_deref(),
            Some("2100-01-01T00:00:00+00:00")
        );
        assert_eq!(
            text(from_unix_secs(-1)).as_deref(),
            Some("1969-12-31T23:59:59+00:00")
        );
        assert_eq!(from_unix_secs(i64::MAX), None);
        assert_eq!(from_unix_secs(i64::MIN), None);
        assert_eq!(
            text(from_unix_millis(1_700_000_000_123)).as_deref(),
            Some("2023-11-14T22:13:20.123+00:00")
        );
        assert_eq!(
            text(from_unix_millis(0)).as_deref(),
            Some("1970-01-01T00:00:00+00:00")
        );
        assert_eq!(
            text(from_unix_millis(-1)).as_deref(),
            Some("1969-12-31T23:59:59.999+00:00")
        );
        assert_eq!(unix_secs(from_unix_millis(-1).unwrap()), -1);
        assert_eq!(unix_secs(from_unix_millis(1_999).unwrap()), 1);
    }

    /// Copilot's `quota_reset_date`: chrono's "%Y-%m-%d" reading, lenient
    /// about leading spaces, short numbers and a signed year, strict about
    /// anything after the day.
    #[test]
    fn dates_read_at_midnight_utc() {
        for (text, written) in [
            ("2026-12-01", "2026-12-01T00:00:00+00:00"),
            ("2026-1-5", "2026-01-05T00:00:00+00:00"),
            (" 2026-12-01", "2026-12-01T00:00:00+00:00"),
            ("2026-12- 1", "2026-12-01T00:00:00+00:00"),
            ("+2026-12-01", "2026-12-01T00:00:00+00:00"),
            ("-0001-01-01", "-0001-01-01T00:00:00+00:00"),
            ("2024-02-29", "2024-02-29T00:00:00+00:00"),
        ] {
            assert_eq!(
                parse_date(text).map(to_rfc3339).as_deref(),
                Some(written),
                "{text:?}"
            );
        }
        for text in [
            "",
            "2026-12-01 ",
            "2026-12-01T00:00:00Z",
            "2026 -12-01",
            "02026-12-01",
            "2026-02-30",
            "2026-13-01",
            "2026/12/01",
            "20261201",
        ] {
            assert_eq!(parse_date(text), None, "{text:?}");
        }
    }

    #[test]
    fn instants_write_as_before() {
        assert_eq!(
            to_rfc3339(at("2026-11-16T10:00:00.999Z")),
            "2026-11-16T10:00:00.999+00:00"
        );
        assert_eq!(
            to_rfc3339_secs(at("2026-11-16T10:00:00Z")),
            "2026-11-16T10:00:00Z"
        );
        assert_eq!(
            to_rfc3339_secs(at("2026-11-16T10:00:00.999Z")),
            "2026-11-16T10:00:00Z"
        );
        assert_eq!(
            to_rfc3339_secs(from_unix_millis(-1).unwrap()),
            "1969-12-31T23:59:59Z"
        );
        assert_eq!(
            to_rfc3339_secs(at("0000-01-01T00:00:00+01:00")),
            "-0001-12-31T23:00:00Z"
        );
        assert_eq!(file_stamp(at("2026-11-16T09:05:07.9Z")), "20261116-090507");
        assert_eq!(file_stamp(from_unix_secs(0).unwrap()), "19700101-000000");
        assert_eq!(
            minute_text(at("2026-11-16T09:05:59.9Z")),
            "2026-11-16 09:05"
        );
        assert_eq!(
            minute_text(at("0000-01-01T00:00:00+01:00")),
            "-0001-12-31 23:00"
        );
    }

    /// The cache compares resets to the minute (`cache_meaning`): down, also
    /// before the epoch, and not at all out of the i64-nanosecond range.
    #[test]
    fn minutes_truncate_down() {
        for (text, written) in [
            ("2026-11-16T10:00:59.999Z", "2026-11-16T10:00:00+00:00"),
            ("2026-11-16T10:00:00Z", "2026-11-16T10:00:00+00:00"),
            ("1969-12-31T23:59:30Z", "1969-12-31T23:59:00+00:00"),
            ("1969-12-31T23:59:00.5Z", "1969-12-31T23:59:00+00:00"),
            ("2262-04-11T23:47:16Z", "2262-04-11T23:47:00+00:00"),
            ("2262-04-11T23:47:17Z", "2262-04-11T23:47:17+00:00"),
            ("2300-01-01T00:00:30Z", "2300-01-01T00:00:30+00:00"),
            ("1677-09-21T00:12:44Z", "1677-09-21T00:12:00+00:00"),
            ("1677-09-21T00:12:43Z", "1677-09-21T00:12:43+00:00"),
        ] {
            assert_eq!(to_rfc3339(trunc_to_minute(at(text))), written, "{text}");
        }
    }
}
