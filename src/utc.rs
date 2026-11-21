// utc.rs — instants in UTC: the clock, and the exact text the app reads them
// from and writes them in (provider answers, its own files, hook variables,
// file names). The app never uses a local time zone.
//
// The instants are jiff's `Timestamp`, with no time-zone database and no
// system time-zone lookup (jiff's tz features are off). The text is written
// and read here by hand, as chrono (0.7.0) wrote and read it, not by jiff's
// own formatter and parser: jiff trims the fraction ("…00.5Z" where chrono
// wrote "…00.500Z"), reads a wider ISO 8601 (no seconds, "+02", the basic
// format, "[UTC]") and refuses more than nine fractional digits. So a file
// either version wrote reads the same in the other, and every provider
// answer reads as before. The one difference: jiff's range ends at
// 9999-12-30 (chrono's at year 262143), and an instant past it reads as no
// time; a leap second (":60") reads as the second before it.
//
// Outside tests, use `now`, `later` and `earlier` here, never
// `Timestamp::now()` or `+` / `-` between a `Timestamp` and a
// `SignedDuration`: those panic through jiff's `Error`, and formatting it
// links every message jiff has, about 130 KB of the exe. (`Timestamp -
// Timestamp` cannot fail and is fine.)

use jiff::civil::DateTime;
use jiff::tz::Offset;
use std::fmt::Write;

pub use jiff::{SignedDuration, Timestamp};

/// The current instant.
pub fn now() -> Timestamp {
    Timestamp::try_from(std::time::SystemTime::now()).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// `d` after `t`, held at the end of the range rather than panicking.
pub fn later(t: Timestamp, d: SignedDuration) -> Timestamp {
    let end = if d.is_negative() {
        Timestamp::MIN
    } else {
        Timestamp::MAX
    };
    t.checked_add(d).unwrap_or(end)
}

/// `d` before `t`, held at the end of the range rather than panicking.
pub fn earlier(t: Timestamp, d: SignedDuration) -> Timestamp {
    match d.checked_neg() {
        Some(back) => later(t, back),
        None => Timestamp::MAX,
    }
}

/// The instant `secs` whole seconds after the Unix epoch.
pub fn from_unix_secs(secs: i64) -> Option<Timestamp> {
    Timestamp::from_second(secs).ok()
}

/// The instant `ms` milliseconds after the Unix epoch.
pub fn from_unix_millis(ms: i64) -> Option<Timestamp> {
    Timestamp::from_millisecond(ms).ok()
}

/// Whole seconds since the Unix epoch, rounded down (jiff's `as_second`
/// rounds toward zero, which differs before the epoch).
pub fn unix_secs(t: Timestamp) -> i64 {
    t.as_second() - i64::from(t.subsec_nanosecond() < 0)
}

/// An RFC 3339 instant as the providers send them: "2026-11-16T10:00:00Z",
/// "…T12:00:00.123+02:00". Strict: four-digit year, seconds, an offset with
/// a colon (or "Z"), "T", "t" or a space between date and time, nothing
/// after; more than nine fractional digits are cut, not rounded.
pub fn parse_rfc3339(text: &str) -> Option<Timestamp> {
    let b = text.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let two = |i: usize| -> Option<i64> {
        let (hi, lo) = (b[i], b[i + 1]);
        (hi.is_ascii_digit() && lo.is_ascii_digit())
            .then(|| i64::from(hi - b'0') * 10 + i64::from(lo - b'0'))
    };
    let year = two(0)? * 100 + two(2)?;
    let month = two(5)?;
    let day = two(8)?;
    let hour = two(11)?;
    let minute = two(14)?;
    let second = two(17)?;
    let separators = b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b'T' | b't' | b' ')
        && b[13] == b':'
        && b[16] == b':';
    if !separators {
        return None;
    }
    // The first 19 bytes are ASCII: slicing there is safe.
    let mut rest = &text[19..];
    let mut nanos = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        (nanos, rest) = scan_fraction(fraction)?;
    }
    let (offset, rest) = scan_offset(rest, false)?;
    if !rest.is_empty() {
        return None;
    }
    instant([year, month, day, hour, minute, second], nanos, offset)
}

/// A "YYYY-MM-DD" date, at midnight UTC. As chrono's "%Y-%m-%d": leading
/// spaces before a number, one-digit months and days and a signed year of
/// any length read; anything after the day does not.
pub fn parse_date(text: &str) -> Option<Timestamp> {
    let mut s = text;
    let year = scan_year(&mut s)?;
    s = s.strip_prefix('-')?;
    let month = scan_number(&mut s, 2)?;
    s = s.strip_prefix('-')?;
    let day = scan_number(&mut s, 2)?;
    if !s.is_empty() {
        return None;
    }
    instant([year, month, day, 0, 0, 0], 0, 0)
}

/// RFC 3339 with "+00:00" and as many fractional digits as it needs (0, 3,
/// 6 or 9): `AILIMITS_RESET_AT`.
pub fn to_rfc3339(t: Timestamp) -> String {
    rfc3339(t, true, "+00:00")
}

/// RFC 3339 in whole seconds with "Z": the crash log.
pub fn to_rfc3339_secs(t: Timestamp) -> String {
    rfc3339(t, false, "Z")
}

/// "20261116-100000": a copy of the config kept aside.
pub fn file_stamp(t: Timestamp) -> String {
    let dt = Offset::UTC.to_datetime(t);
    let mut out = year_text(dt.year());
    let _ = write!(
        out,
        "{:02}{:02}-{:02}{:02}{:02}",
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );
    out
}

/// "2026-11-16 10:00": what `ailimits-auth status` prints.
pub fn minute_text(t: Timestamp) -> String {
    let dt = Offset::UTC.to_datetime(t);
    let mut out = year_text(dt.year());
    let _ = write!(
        out,
        "-{:02}-{:02} {:02}:{:02}",
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute()
    );
    out
}

/// Rounded down to the minute; left as it is when it lies beyond the range
/// of nanoseconds an i64 counts from the epoch (1677-09-21 to 2262-04-11),
/// as chrono's `duration_trunc` left it.
pub fn trunc_to_minute(t: Timestamp) -> Timestamp {
    const MINUTE: i128 = 60_000_000_000;
    let nanos = t.as_nanosecond();
    if i64::try_from(nanos).is_err() {
        return t;
    }
    Timestamp::from_nanosecond(nanos - nanos.rem_euclid(MINUTE)).unwrap_or(t)
}

// ─── Writing ─────────────────────────────────────────────────────

/// The year as chrono writes it: four digits from 0 to 9999, else signed.
fn year_text(year: i16) -> String {
    if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+05}")
    }
}

/// "2026-11-16T10:00:00" in UTC, the fraction when asked (none, or 3, 6 or
/// 9 digits, the fewest that keep it whole), then `zone`.
fn rfc3339(t: Timestamp, fraction: bool, zone: &str) -> String {
    let dt = Offset::UTC.to_datetime(t);
    let mut out = String::with_capacity(36);
    out.push_str(&year_text(dt.year()));
    let _ = write!(
        out,
        "-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );
    let nanos = dt.subsec_nanosecond();
    if fraction && nanos != 0 {
        let _ = if nanos % 1_000_000 == 0 {
            write!(out, ".{:03}", nanos / 1_000_000)
        } else if nanos % 1_000 == 0 {
            write!(out, ".{:06}", nanos / 1_000)
        } else {
            write!(out, ".{nanos:09}")
        };
    }
    out.push_str(zone);
    out
}

// ─── Reading ─────────────────────────────────────────────────────

/// The instant of a date and time (`[year, month, day, hour, minute,
/// second]`) at an offset of `offset` seconds east of UTC. None for a date
/// or time that does not exist, an offset of a day or more, or an instant
/// out of jiff's range.
fn instant(fields: [i64; 6], nanos: i32, offset: i32) -> Option<Timestamp> {
    let [year, month, day, hour, minute, second] = fields;
    if offset.abs() >= 86_400 {
        return None;
    }
    // A leap second reads as the second before it.
    let second = if second == 60 { 59 } else { second };
    let small = |n: i64| i8::try_from(n).ok();
    let dt = DateTime::new(
        i16::try_from(year).ok()?,
        small(month)?,
        small(day)?,
        small(hour)?,
        small(minute)?,
        small(second)?,
        nanos,
    )
    .ok()?;
    Offset::from_seconds(offset).ok()?.to_timestamp(dt).ok()
}

/// A fraction after its ".": at least one digit, nine kept, any further
/// digits skipped. Returns nanoseconds and the rest.
fn scan_fraction(s: &str) -> Option<(i32, &str)> {
    let digits = s.bytes().take(9).take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let value: i32 = s[..digits].parse().ok()?;
    let nanos = value * 10i32.pow(9 - digits as u32);
    let rest = s[digits..].trim_start_matches(|c: char| c.is_ascii_digit());
    Some((nanos, rest))
}

/// "Z", "z" or a signed "HH:MM" ("+", "-" or U+2212 MINUS SIGN), in seconds
/// east of UTC. `lenient` (chrono's serde reading) takes any run of colons
/// and spaces between hours and minutes, none included; else one colon.
fn scan_offset(s: &str, lenient: bool) -> Option<(i32, &str)> {
    if let Some(rest) = s.strip_prefix(['Z', 'z']) {
        return Some((0, rest));
    }
    let (negative, s) = match s.strip_prefix('+') {
        Some(rest) => (false, rest),
        None => (true, s.strip_prefix(['-', '\u{2212}'])?),
    };
    let b = s.as_bytes();
    if b.len() < 2 || !b[0].is_ascii_digit() || !b[1].is_ascii_digit() {
        return None;
    }
    let hours = i32::from(b[0] - b'0') * 10 + i32::from(b[1] - b'0');
    let s = if lenient {
        s[2..].trim_start_matches(|c: char| c == ':' || c.is_whitespace())
    } else {
        s[2..].strip_prefix(':')?
    };
    let b = s.as_bytes();
    if b.len() < 2 || !matches!(b[0], b'0'..=b'5') || !b[1].is_ascii_digit() {
        return None;
    }
    let minutes = i32::from(b[0] - b'0') * 10 + i32::from(b[1] - b'0');
    let secs = hours * 3600 + minutes * 60;
    Some((if negative { -secs } else { secs }, &s[2..]))
}

/// One to `max` digits, after any leading white space.
fn scan_number(s: &mut &str, max: usize) -> Option<i64> {
    *s = s.trim_start();
    scan_digits(s, max)
}

/// One to `max` digits, as an i64.
fn scan_digits(s: &mut &str, max: usize) -> Option<i64> {
    let digits = s.bytes().take(max).take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let value = s[..digits].parse().ok()?;
    *s = &s[digits..];
    Some(value)
}

/// A year after any leading white space: up to four digits, or a sign and
/// any number of them.
fn scan_year(s: &mut &str) -> Option<i64> {
    *s = s.trim_start();
    if let Some(rest) = s.strip_prefix('-') {
        *s = rest;
        scan_digits(s, usize::MAX).map(|v| -v)
    } else if let Some(rest) = s.strip_prefix('+') {
        *s = rest;
        scan_digits(s, usize::MAX)
    } else {
        scan_digits(s, 4)
    }
}

/// An instant as chrono's serde read it (its `FromStr`): RFC 3339, also
/// with spaces around the numbers and before the offset, one-digit fields,
/// a signed year, "UTC" for the offset, an offset without its colon and
/// white space at the end.
fn parse_lenient(text: &str) -> Option<Timestamp> {
    let mut s = text;
    let year = scan_year(&mut s)?;
    s = s.trim_start().strip_prefix('-')?;
    let month = scan_number(&mut s, 2)?;
    s = s.trim_start().strip_prefix('-')?;
    let day = scan_number(&mut s, 2)?;
    s = s.strip_prefix(['T', 't', ' '])?;
    let hour = scan_number(&mut s, 2)?;
    s = s.trim_start().strip_prefix(':')?;
    let minute = scan_number(&mut s, 2)?;
    s = s.trim_start().strip_prefix(':')?;
    let second = scan_number(&mut s, 2)?;
    let mut nanos = 0;
    if let Some(fraction) = s.strip_prefix('.') {
        (nanos, s) = scan_fraction(fraction)?;
    }
    s = s.trim_start();
    let offset;
    if s.len() >= 3 && s.as_bytes()[..3].eq_ignore_ascii_case(b"UTC") {
        offset = 0;
        s = &s[3..];
    } else {
        (offset, s) = scan_offset(s, true)?;
    }
    if !s.trim_start().is_empty() {
        return None;
    }
    instant([year, month, day, hour, minute, second], nanos, offset)
}

// ─── serde ───────────────────────────────────────────────────────

/// Serde for the app's own files (`provider-cache.json`, `alerts.json`), in
/// chrono's text: written "2026-11-16T10:00:00.500Z" (UTC, "Z", 0, 3, 6 or 9
/// fractional digits), read leniently (`parse_lenient`). Use as
/// `#[serde(with = "crate::utc::as_text")]`, or its `option` and
/// `vec_option` for `Option<Timestamp>` and `Vec<Option<Timestamp>>`.
pub mod as_text {
    use super::Timestamp;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt;

    /// One instant as text.
    struct Text(Timestamp);

    impl Serialize for Text {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&super::rfc3339(self.0, true, "Z"))
        }
    }

    impl<'de> Deserialize<'de> for Text {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl de::Visitor<'_> for Visitor {
                type Value = Text;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("an RFC 3339 formatted date and time string")
                }

                fn visit_str<E: de::Error>(self, value: &str) -> Result<Text, E> {
                    super::parse_lenient(value)
                        .map(Text)
                        .ok_or_else(|| E::custom("not an RFC 3339 date and time"))
                }
            }
            deserializer.deserialize_str(Visitor)
        }
    }

    pub fn serialize<S: Serializer>(t: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
        Text(*t).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
        Ok(Text::deserialize(deserializer)?.0)
    }

    /// For an `Option<Timestamp>`: None is `null`. Pair it with
    /// `#[serde(default)]` so that a missing field reads as None.
    pub mod option {
        use super::{Text, Timestamp};
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(
            t: &Option<Timestamp>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            t.map(Text).serialize(serializer)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Timestamp>, D::Error> {
            Ok(Option::<Text>::deserialize(deserializer)?.map(|t| t.0))
        }
    }

    /// For a `Vec<Option<Timestamp>>`.
    pub mod vec_option {
        use super::{Text, Timestamp};
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(
            v: &[Option<Timestamp>],
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            serializer.collect_seq(v.iter().map(|t| t.map(Text)))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec<Option<Timestamp>>, D::Error> {
            let texts = Vec::<Option<Text>>::deserialize(deserializer)?;
            Ok(texts.into_iter().map(|t| t.map(|t| t.0)).collect())
        }
    }
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

    /// `later` and `earlier` are `+` and `-`, held at the ends of the range.
    #[test]
    fn arithmetic_holds_at_the_ends_instead_of_panicking() {
        let t = at("2026-11-16T10:00:00Z");
        let d = SignedDuration::from_secs(90);
        assert_eq!(later(t, d), t + d);
        assert_eq!(earlier(t, d), t - d);
        assert_eq!(later(t, -d), t - d);
        assert_eq!(later(Timestamp::MAX, d), Timestamp::MAX);
        assert_eq!(earlier(Timestamp::MIN, d), Timestamp::MIN);
        assert_eq!(later(Timestamp::MIN, -d), Timestamp::MIN);
        assert_eq!(earlier(t, SignedDuration::MIN), Timestamp::MAX);
        assert!(now() > at("2026-01-01T00:00:00Z"));
    }

    /// Where jiff differs from chrono: its range ends at 9999-12-30 (an
    /// instant past it reads as none), and a leap second reads as the second
    /// before it. No provider sends either.
    #[test]
    fn the_range_ends_in_9999_and_a_leap_second_reads_as_59() {
        assert_eq!(parse_rfc3339("9999-12-31T00:00:00Z"), None);
        assert_eq!(from_unix_secs(253_402_300_799), None);
        assert_eq!(
            to_rfc3339(at("2016-12-31T23:59:60.5Z")),
            "2016-12-31T23:59:59.500+00:00"
        );
    }
}
