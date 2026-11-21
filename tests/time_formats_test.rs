// tests/time_formats_test.rs — the instants the app reads from the providers
// and keeps in its own files, pinned to the text 0.7.0 read and wrote. Each
// read instant is compared as `AILIMITS_RESET_AT` writes it (RFC 3339 in
// UTC, every nanosecond kept).

use ailimits::providers::{
    antigravity, claude, codex, copilot, cursor, grok, Metric, ProviderData,
};
use ailimits::utc::{parse_rfc3339, to_rfc3339};

fn resets(metrics: &[Metric]) -> Vec<Option<String>> {
    metrics.iter().map(|m| m.reset_at.map(to_rfc3339)).collect()
}

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// A JWT-shaped token around `payload` (unpadded base64url; the signature is
/// never checked).
fn jwt(payload: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in payload.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    format!("eyJhbGciOiJIUzI1NiJ9.{out}.c2ln")
}

// ─── The app's own files ─────────────────────────────────────────

/// `updated_at` of a cached provider: read as chrono read it (leniently:
/// spaces, short numbers, "UTC", an offset without a colon) and written as
/// chrono wrote it ("Z", 0, 3, 6 or 9 fractional digits). None: the entry,
/// and with it the whole cache file, does not read.
fn cached(updated_at: &str) -> Option<String> {
    let text = format!(
        r#"{{"id":"claude","status":"Ok","metrics":[],"updated_at":{}}}"#,
        serde_json::to_string(updated_at).unwrap()
    );
    let data: ProviderData = serde_json::from_str(&text).ok()?;
    let value = serde_json::to_value(&data).unwrap();
    Some(value["updated_at"].as_str().unwrap().to_string())
}

#[test]
fn cached_instants_read_and_write_as_before() {
    for (text, written) in [
        ("2026-11-16T10:00:00Z", "2026-11-16T10:00:00Z"),
        (
            "2026-11-21T07:58:01.123456700Z",
            "2026-11-21T07:58:01.123456700Z",
        ),
        (
            "2026-11-21T07:58:01.1234567Z",
            "2026-11-21T07:58:01.123456700Z",
        ),
        ("2026-11-21T07:58:01.120Z", "2026-11-21T07:58:01.120Z"),
        ("2026-11-21T07:58:01.12Z", "2026-11-21T07:58:01.120Z"),
        ("2026-11-21T07:58:01.000120Z", "2026-11-21T07:58:01.000120Z"),
        ("2026-11-21T07:58:01.000000Z", "2026-11-21T07:58:01Z"),
        (
            "2026-11-16T10:00:00.1234567891z",
            "2026-11-16T10:00:00.123456789Z",
        ),
        ("1970-01-01T00:00:00Z", "1970-01-01T00:00:00Z"),
        ("1969-12-31T23:59:59.5Z", "1969-12-31T23:59:59.500Z"),
        ("2100-01-01T00:00:00Z", "2100-01-01T00:00:00Z"),
        ("0000-01-01T00:00:00+01:00", "-0001-12-31T23:00:00Z"),
        ("-0001-12-31T23:00:00Z", "-0001-12-31T23:00:00Z"),
        // chrono's lenient reading.
        ("2026-11-16 10:00:00 UTC", "2026-11-16T10:00:00Z"),
        ("2026-11-16T10:00:00utc", "2026-11-16T10:00:00Z"),
        ("2026-11-16T12:00:00+02:00", "2026-11-16T10:00:00Z"),
        ("2026-11-16T12:00:00+0200", "2026-11-16T10:00:00Z"),
        ("2026-11-16T12:00:00 +02:00", "2026-11-16T10:00:00Z"),
        ("2026-11-16T12:00:00+02 00", "2026-11-16T10:00:00Z"),
        ("2026-11-16T12:00:00+02::00", "2026-11-16T10:00:00Z"),
        ("2026-11-16T08:00:00\u{2212}02:00", "2026-11-16T10:00:00Z"),
        ("2026-11-16t10:00:00Z", "2026-11-16T10:00:00Z"),
        (" 2026-1-5T1:2:3.5Z ", "2026-01-05T01:02:03.500Z"),
        ("2026 - 11 - 16 10 : 00 : 00 Z", "2026-11-16T10:00:00Z"),
        ("+2026-11-16T10:00:00Z", "2026-11-16T10:00:00Z"),
    ] {
        assert_eq!(cached(text).as_deref(), Some(written), "{text:?}");
    }
    for text in [
        "",
        "2026-11-16",
        "2026-11-16T10:00Z",
        "2026-11-16T10:00:00",
        "2026-11-16T10:00:00+02",
        "2026-11-16T10:00:00+24:00",
        "2026-11-16T10:00:00+02:60",
        "2026-11-16T10:00:00Z x",
        "2026-11-16T10:00:00Z[UTC]",
        "2026-11-16T10:00:00,5Z",
        "2026-11-16T10:00:00.Z",
        "20261116T100000Z",
        "02026-11-16T10:00:00Z",
        "2026-02-30T00:00:00Z",
        "2026-11-16T24:00:00Z",
        "2026-11-16X10:00:00Z",
    ] {
        assert_eq!(cached(text), None, "{text:?}");
    }
    // Not a string at all.
    let number = r#"{"id":"claude","status":"Ok","metrics":[],"updated_at":1794823200}"#;
    assert!(serde_json::from_str::<ProviderData>(number).is_err());
}

#[test]
fn a_metric_without_a_reset_reads_and_writes_as_before() {
    let old = r#"{"label":"Session","used":1,"limit":null,"unit":"Percent"}"#;
    let m: Metric = serde_json::from_str(old).unwrap();
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"label":"Session","used":1,"limit":null,"unit":"Percent","reset_at":null,"window":"Session"}"#
    );
    let with = r#"{"label":"Weekly","used":2,"limit":100,"unit":"Percent","reset_at":"2026-11-16T12:00:00.5+02:00","window":"Long"}"#;
    let m: Metric = serde_json::from_str(with).unwrap();
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"label":"Weekly","used":2,"limit":100,"unit":"Percent","reset_at":"2026-11-16T10:00:00.500Z","window":"Long"}"#
    );
}

// ─── Provider answers ────────────────────────────────────────────

#[test]
fn claude_resets_read_as_before() {
    let body = r#"{
        "five_hour": {"utilization": 10, "resets_at": "2026-11-16T12:00:00.123456+02:00"},
        "seven_day": {"utilization": 20, "resets_at": "2026-11-16T10:00Z"},
        "seven_day_opus": {"utilization": 30, "resets_at": "2026-11-21T08:00:00.000000001Z"},
        "seven_day_sonnet": {"utilization": 40, "resets_at": null},
        "limits": [{"kind": "weekly_scoped", "percent": 5,
            "resets_at": "2026-11-21T08:00:00.9876543210Z",
            "scope": {"model": {"display_name": "Fable"}}}]
    }"#;
    assert_eq!(
        resets(&claude::parse_oauth_usage(body).unwrap()),
        [
            some("2026-11-16T10:00:00.123456+00:00"),
            None,
            some("2026-11-21T08:00:00.000000001+00:00"),
            None,
            some("2026-11-21T08:00:00.987654321+00:00"),
        ]
    );
}

#[test]
fn codex_resets_read_as_before() {
    let body = |primary: &str| {
        format!(
            r#"{{"rate_limit":{{"primary_window":{{"used_percent":1,"reset_at":{primary}}},
                "secondary_window":{{"used_percent":28,"reset_at":4102444800}}}}}}"#
        )
    };
    let reset = |primary: &str| resets(&codex::parse_wham_usage(&body(primary)).unwrap());
    assert_eq!(
        reset("1794823200"),
        [
            some("2026-11-16T10:00:00+00:00"),
            some("2100-01-01T00:00:00+00:00")
        ]
    );
    assert_eq!(reset("0")[0], some("1970-01-01T00:00:00+00:00"));
    assert_eq!(reset("-1")[0], some("1969-12-31T23:59:59+00:00"));
    assert_eq!(reset("1794823200.5")[0], None);
    assert_eq!(reset("\"1794823200\"")[0], None);
}

#[test]
fn copilot_resets_read_as_before() {
    let body = |dates: &str| {
        format!(
            r#"{{{dates} "quota_snapshots": {{"chat": {{"entitlement": 200, "remaining": 150,
                "unlimited": false, "has_quota": true}}}}}}"#
        )
    };
    let reset = |dates: &str| resets(&copilot::parse_copilot_quotas(&body(dates)).unwrap());
    assert_eq!(
        reset(
            r#""quota_reset_date_utc": "2026-12-01T00:00:00.000Z", "quota_reset_date": "2026-11-01","#
        ),
        [some("2026-12-01T00:00:00+00:00")]
    );
    assert_eq!(
        reset(r#""quota_reset_date_utc": "2026-12-01T02:00:00.5+02:00","#),
        [some("2026-12-01T00:00:00.500+00:00")]
    );
    // The date alone, or beside a UTC time that does not read: midnight UTC.
    assert_eq!(
        reset(
            r#""quota_reset_date_utc": "2026-12-01T00:00:00", "quota_reset_date": "2026-12-01","#
        ),
        [some("2026-12-01T00:00:00+00:00")]
    );
    assert_eq!(
        reset(r#""quota_reset_date": "2026-1-5","#),
        [some("2026-01-05T00:00:00+00:00")]
    );
    assert_eq!(reset(r#""quota_reset_date": "2026-12-01 ","#), [None]);
    assert_eq!(
        reset(r#""quota_reset_date": "2026-12-01T00:00:00Z","#),
        [None]
    );
    assert_eq!(reset(""), [None]);
}

#[test]
fn grok_times_read_as_before() {
    let auth = r#"{"default": {"key": "k", "expires_at": "2026-11-09T05:43:16.908847600Z"}}"#;
    let before = parse_rfc3339("2026-11-09T05:43:16.908847599Z").unwrap();
    match grok::parse_grok_auth(auth, before).unwrap() {
        grok::GrokLogin::Token { expires_at, .. } => assert_eq!(
            expires_at.map(to_rfc3339),
            some("2026-11-09T05:43:16.908847600+00:00")
        ),
        other => panic!("{other:?}"),
    }
    // Expired at its own instant, to the nanosecond.
    let at = parse_rfc3339("2026-11-09T05:43:16.9088476Z").unwrap();
    assert!(matches!(
        grok::parse_grok_auth(auth, at).unwrap(),
        grok::GrokLogin::Expired
    ));
    let billing = r#"{"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
        "start": "2026-11-24T02:00:00+02:00", "end": "2026-12-01T02:00:00.25+02:00"},
        "creditUsagePercent": 1.0}}"#;
    assert_eq!(
        resets(&grok::parse_billing_credits(billing).unwrap()),
        [some("2026-12-01T00:00:00.250+00:00")]
    );
    let fallback = r#"{"config": {"currentPeriod": {"end": "2026-12-01"},
        "billingPeriodEnd": "2026-12-01T00:00:00Z", "creditUsagePercent": 1.0}}"#;
    assert_eq!(
        resets(&grok::parse_billing_credits(fallback).unwrap()),
        [some("2026-12-01T00:00:00+00:00")]
    );
}

#[test]
fn cursor_times_read_as_before() {
    let exp = |claim: &str| {
        let token = jwt(&format!(r#"{{"sub":"github|user_01","exp":{claim}}}"#));
        cursor::parse_session_claims(&token)
            .unwrap()
            .expires_at
            .map(to_rfc3339)
    };
    assert_eq!(exp("4102444800"), some("2100-01-01T00:00:00+00:00"));
    assert_eq!(exp("1794823200.9"), some("2026-11-16T10:00:00+00:00"));
    assert_eq!(exp("-1.5"), some("1969-12-31T23:59:59+00:00"));
    assert_eq!(exp("0"), some("1970-01-01T00:00:00+00:00"));
    assert_eq!(exp("\"4102444800\""), None);
    assert_eq!(exp("null"), None);

    // GetSandUsageStatus: RFC 3339 text, or epoch seconds or milliseconds
    // (from 10^11 on), as a number or digits; never at or before the epoch.
    let sand = |reset: &str| {
        let body = format!(r#"{{"nextResetTimestampUtc":{reset},"usagePercent":12}}"#);
        cursor::parse_sand_usage(&body)
            .unwrap()
            .reset_at
            .map(to_rfc3339)
    };
    assert_eq!(
        sand(r#"" 2026-11-16T23:01:28.407Z ""#),
        some("2026-11-16T23:01:28.407+00:00")
    );
    assert_eq!(
        sand(r#""2026-11-17T01:01:28.407+02:00""#),
        some("2026-11-16T23:01:28.407+00:00")
    );
    assert_eq!(sand("1794823200000"), some("2026-11-16T10:00:00+00:00"));
    assert_eq!(sand("1794823200"), some("2026-11-16T10:00:00+00:00"));
    assert_eq!(
        sand(r#""1794823200123""#),
        some("2026-11-16T10:00:00.123+00:00")
    );
    assert_eq!(sand(r#"" 1794823200 ""#), some("2026-11-16T10:00:00+00:00"));
    assert_eq!(sand("99999999999"), some("5138-11-16T09:46:39+00:00"));
    assert_eq!(sand("100000000000"), some("1973-03-03T09:46:40+00:00"));
    assert_eq!(sand("1"), some("1970-01-01T00:00:01+00:00"));
    assert_eq!(sand("0"), None);
    assert_eq!(sand("-100000000000"), None);
    assert_eq!(sand(r#""1970-01-01T00:00:00.500Z""#), None);
    assert_eq!(
        sand(r#""1970-01-01T00:00:01Z""#),
        some("1970-01-01T00:00:01+00:00")
    );
    assert_eq!(sand(r#""2026-11-16T10:00Z""#), None);
    assert_eq!(sand("1794823200.5"), None);

    let usage = cursor::parse_period_usage(
        r#"{"billingCycleStart":"1794265288407","billingCycleEnd":"4102444800000","planUsage":{"totalPercentUsed":37.0}}"#,
    )
    .unwrap();
    assert_eq!(
        usage.start.map(to_rfc3339),
        some("2026-11-09T23:01:28.407+00:00")
    );
    assert_eq!(usage.end.map(to_rfc3339), some("2100-01-01T00:00:00+00:00"));
}

#[test]
fn antigravity_times_read_as_before() {
    let login = |expiry: &str| {
        let body = format!(r#"{{"token": {{"access_token": "t", "expiry": "{expiry}"}}}}"#);
        antigravity::parse_antigravity_keyring_token(&body).unwrap()
    };
    assert!(matches!(
        login("2100-01-01T02:00:00+02:00"),
        antigravity::AntigravityLogin::Token(_)
    ));
    assert!(matches!(
        login("2020-01-01T00:00:00Z"),
        antigravity::AntigravityLogin::Expired
    ));
    assert!(matches!(
        login("2100-01-01T00:00:00"),
        antigravity::AntigravityLogin::Expired
    ));

    let summary = r#"{"groups": [{"displayName": "Gemini Models", "buckets": [
        {"bucketId": "a", "displayName": "Weekly Limit", "window": "weekly",
         "resetTime": "2026-11-16T12:00:00.5+02:00", "remainingFraction": 0.5},
        {"bucketId": "b", "displayName": "Daily Limit", "window": "daily",
         "resetTime": "1970-01-01T00:00:00.999Z", "remainingFraction": 1}]}]}"#;
    assert_eq!(
        resets(&antigravity::parse_quota_summary(summary).unwrap()),
        [some("2026-11-16T10:00:00.500+00:00"), None]
    );

    let models = r#"{"models": {"gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro",
        "quotaInfo": {"remainingFraction": 0.63, "resetTime": "2026-11-16T10:00:00.123Z"}}}}"#;
    assert_eq!(
        resets(&antigravity::parse_available_models_quota(models).unwrap()),
        [some("2026-11-16T10:00:00.123+00:00")]
    );
}
