// tests/provider_mock_test.rs — provider logic tests, no network involved.

use ailimits::providers::{
    Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
};
use chrono::Utc;

// Helper: a metric with used/limit.
fn metric(used: u64, limit: Option<u64>) -> Metric {
    Metric {
        label: "Session".to_string(),
        used,
        limit,
        unit: MetricUnit::Requests,
        reset_at: None,
        window: MetricWindow::Session,
    }
}

#[test]
fn percentage_is_computed_correctly() {
    assert_eq!(metric(84, Some(200)).percentage(), Some(42.0));
    // No limit — the percentage is unknown.
    assert_eq!(metric(84, None).percentage(), None);
    // A zero limit must not divide by zero.
    assert_eq!(metric(84, Some(0)).percentage(), Some(0.0));
    // Over the limit — clamp to 100.
    assert_eq!(metric(300, Some(200)).percentage(), Some(100.0));
}

#[test]
fn token_display_text_uses_short_format() {
    let m = Metric {
        label: "Tokens".to_string(),
        used: 112_000,
        limit: Some(1_000_000),
        unit: MetricUnit::Tokens,
        reset_at: None,
        window: MetricWindow::Session,
    };
    // 112000 → "112k", 1000000 → "1.0M".
    assert_eq!(m.display_text(), "112k / 1.0M");
}

#[test]
fn oauth_usage_response_parses_to_metrics() {
    // A real /api/oauth/usage response, verified 2026-06-10.
    let body = r#"{"five_hour":{"utilization":83.0,"resets_at":"2026-06-10T17:20:01.101562+00:00"},"seven_day":{"utilization":6.0,"resets_at":"2026-06-13T12:00:01.101579+00:00"},"seven_day_oauth_apps":null,"seven_day_opus":null,"seven_day_sonnet":{"utilization":3.0,"resets_at":"2026-06-13T12:00:01.101585+00:00"},"extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"utilization":null,"currency":null,"disabled_reason":null}}"#;

    let metrics = ailimits::providers::claude::parse_oauth_usage(body).expect("should parse");

    // Session, Weekly, Sonnet; null windows (opus) are skipped.
    assert_eq!(metrics.len(), 3);
    assert_eq!(metrics[0].label, "Session");
    assert_eq!(metrics[0].used, 83);
    assert_eq!(metrics[0].limit, Some(100));
    assert!(metrics[0].reset_at.is_some());
    assert_eq!(metrics[1].label, "Weekly");
    assert_eq!(metrics[1].used, 6);
    assert_eq!(metrics[2].label, "Sonnet");
}

#[test]
fn oauth_usage_tolerates_unknown_schema() {
    // The schema is undocumented: unknown fields → an empty list, not an error.
    let metrics = ailimits::providers::claude::parse_oauth_usage(r#"{"new_field": 1}"#)
        .expect("should not fail");
    assert!(metrics.is_empty());
}

#[test]
fn a_failing_claude_usage_token_keeps_the_row() {
    use ailimits::providers::claude::ChainOutcome;

    // Only a stored usage token, and it gave no data (rejected, 429, offline,
    // an unknown schema): an honest error that app.rs greys, never a hidden
    // row that drops the last real data.
    let only_token = ChainOutcome {
        usage_token: true,
        ..ChainOutcome::default()
    };
    assert!(
        matches!(only_token.status(), ProviderStatus::NetworkError(ref m) if m == "token expired"),
        "got {:?}",
        only_token.status()
    );
    let bounced = ChainOutcome {
        rate_limited: true,
        ..only_token
    };
    assert!(matches!(
        bounced.status(),
        ProviderStatus::NetworkError(ref m) if m == "rate-limited, retrying"
    ));

    // The credentials file alone, as it always was.
    let file = ChainOutcome {
        credentials_file: true,
        ..ChainOutcome::default()
    };
    assert!(matches!(file.status(), ProviderStatus::NetworkError(_)));

    // No source at all: the row stays hidden.
    assert!(matches!(
        ChainOutcome::default().status(),
        ProviderStatus::NotConfigured
    ));
}

#[test]
fn an_offline_claude_is_not_an_expired_token() {
    use ailimits::providers::claude::ChainOutcome;
    use ailimits::ui::renderer::hover_reason;
    use chrono::Duration;
    use std::collections::HashMap;

    // A valid Claude Code token, but no request got an answer.
    let offline = ChainOutcome {
        credentials_file: true,
        offline: true,
        ..ChainOutcome::default()
    };
    let status = offline.status();
    let ProviderStatus::NetworkError(msg) = &status else {
        panic!("offline must stay an honest error, got {status:?}");
    };
    assert_eq!(msg, "offline, retrying");

    // The hover adds no "run Claude Code": it would not help.
    let stale = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![metric(10, Some(100))],
        updated_at: Utc::now() - Duration::hours(1),
        received_at: None,
    };
    let errors = HashMap::from([(ProviderId::Claude, msg.clone())]);
    assert_eq!(
        hover_reason(&stale, &errors).as_deref(),
        Some("offline, retrying")
    );

    // A 429 in the same cycle reached the server: the rate limit is the news.
    let bounced = ChainOutcome {
        rate_limited: true,
        ..offline
    };
    assert!(matches!(
        bounced.status(),
        ProviderStatus::NetworkError(ref m) if m == "rate-limited, retrying"
    ));

    // A 200 the parser does not know is no expired token either.
    let unknown = ChainOutcome {
        credentials_file: true,
        unrecognized: true,
        ..ChainOutcome::default()
    };
    assert!(matches!(
        unknown.status(),
        ProviderStatus::NetworkError(ref m) if m == "unrecognized usage schema"
    ));
}

#[test]
fn a_failing_codex_usage_token_or_unreadable_auth_json_keeps_the_row() {
    use ailimits::providers::codex::status_without_cli_token;

    // No auth.json behind a rejected usage token: the rejection owns the row.
    let rejected = Some(ProviderStatus::AuthError(
        "usage token rejected".to_string(),
    ));
    assert!(matches!(
        status_without_cli_token(rejected.clone(), false),
        ProviderStatus::AuthError(ref m) if m == "usage token rejected"
    ));
    // The same for a request that never got an answer.
    let offline = Some(ProviderStatus::NetworkError(
        "error sending request".to_string(),
    ));
    assert!(matches!(
        status_without_cli_token(offline, false),
        ProviderStatus::NetworkError(ref m) if m == "error sending request"
    ));

    // An auth.json that cannot be read is an honest error, never "not set up".
    for usage_token_failure in [None, rejected] {
        assert!(matches!(
            status_without_cli_token(usage_token_failure, true),
            ProviderStatus::NetworkError(ref m) if m == "auth.json unreadable"
        ));
    }

    // No source at all: the row stays hidden.
    assert!(matches!(
        status_without_cli_token(None, false),
        ProviderStatus::NotConfigured
    ));
}

#[test]
fn wham_usage_response_parses_to_metrics() {
    use ailimits::providers::codex::parse_wham_usage;

    // A real wham/usage response, verified 2026-06-10.
    let body = r#"{"user_id": "user-x", "plan_type": "plus",
        "rate_limit": {
            "allowed": true, "limit_reached": false,
            "primary_window": {"used_percent": 1, "limit_window_seconds": 18000, "reset_after_seconds": 18000, "reset_at": 1781121633},
            "secondary_window": {"used_percent": 28, "limit_window_seconds": 604800, "reset_after_seconds": 450201, "reset_at": 1781553834}},
        "code_review_rate_limit": null, "additional_rate_limits": null,
        "credits": {"has_credits": false, "balance": "0"}}"#;

    let metrics = parse_wham_usage(body).expect("should parse");
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].label, "Session");
    assert_eq!(metrics[0].used, 1);
    assert_eq!(metrics[0].limit, Some(100));
    assert!(metrics[0].reset_at.is_some());
    assert_eq!(metrics[1].label, "Weekly");
    assert_eq!(metrics[1].used, 28);

    // An unknown schema → empty, not an error.
    assert!(parse_wham_usage(r#"{"foo": 1}"#).unwrap().is_empty());
}

#[test]
fn copilot_internal_quotas_parse_to_metrics() {
    use ailimits::providers::copilot::parse_copilot_quotas;

    // A fragment of a real copilot_internal/user response, verified 2026-06-10.
    let body = r#"{"login": "napxlexn", "copilot_plan": "individual",
        "quota_reset_date": "2026-07-01",
        "quota_snapshots": {
            "chat": {"entitlement": 200, "remaining": -1, "unlimited": false, "percent_remaining": 0.0},
            "completions": {"entitlement": 2000, "remaining": 2000, "unlimited": false, "percent_remaining": 100.0},
            "premium_interactions": {"entitlement": 50, "remaining": 10, "unlimited": false, "percent_remaining": 20.0}}}"#;

    let metrics = parse_copilot_quotas(body).expect("should parse");
    assert_eq!(metrics.len(), 3);
    // The first one is premium_interactions.
    assert_eq!(metrics[0].label, "Premium");
    assert_eq!(metrics[0].used, 40);
    assert_eq!(metrics[0].limit, Some(50));
    assert!(metrics[0].reset_at.is_some());
    // remaining -1 → used = 201; the UI clamps it.
    assert_eq!(metrics[1].label, "Chat");
    assert_eq!(metrics[1].used, 201);
    assert_eq!(metrics[2].used, 0);

    // Unlimited quotas are skipped.
    let unlimited =
        r#"{"quota_snapshots": {"chat": {"entitlement": 0, "remaining": 0, "unlimited": true}}}"#;
    assert!(parse_copilot_quotas(unlimited).unwrap().is_empty());
}

#[test]
fn copilot_answers_map_to_honest_statuses() {
    use ailimits::providers::copilot::quota_outcome;

    let body = r#"{"quota_reset_date": "2026-07-01", "quota_snapshots": {
        "premium_interactions": {"entitlement": 50, "remaining": 10, "unlimited": false}}}"#;
    let (status, metrics) = quota_outcome(200, body);
    assert!(matches!(status, ProviderStatus::Ok));
    assert_eq!(metrics[0].used, 40);

    // A 200 with a body we cannot read — an HTML page, another shape — is an
    // honest row, never a dropped update.
    for body in ["<html>oops</html>", r#"{"foo": 1}"#, ""] {
        let (status, metrics) = quota_outcome(200, body);
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if m == "unrecognized copilot_internal schema"),
            "{body}: {status:?}"
        );
        assert!(metrics.is_empty());
    }

    for code in [401, 403, 404] {
        let (status, metrics) = quota_outcome(code, body);
        assert!(
            matches!(&status, ProviderStatus::AuthError(m) if m == "token has no Copilot access")
        );
        assert!(metrics.is_empty());
    }
    for code in [302, 429, 500] {
        let (status, metrics) = quota_outcome(code, body);
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if *m == format!("HTTP {code}")),
            "{code}: {status:?}"
        );
        assert!(metrics.is_empty(), "a non-200 body is never parsed");
    }
}

#[test]
fn copilot_zero_entitlement_quota_does_not_become_primary_metric() {
    use ailimits::providers::copilot::parse_copilot_quotas;

    let body = r#"{"login": "napxlexn", "copilot_plan": "individual",
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "premium_interactions": {"entitlement": 0, "remaining": 0, "unlimited": false},
            "chat": {"entitlement": 200, "remaining": 115, "unlimited": false},
            "completions": {"entitlement": 2000, "remaining": 2000, "unlimited": false}}}"#;

    let metrics = parse_copilot_quotas(body).expect("should parse");

    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].label, "Chat");
    assert_eq!(metrics[0].used, 85);
    assert_eq!(metrics[0].limit, Some(200));
    assert_eq!(metrics[1].label, "Completions");
    assert_eq!(metrics[1].used, 0);
}

#[test]
fn antigravity_keyring_token_parses_to_access_token() {
    use ailimits::providers::antigravity::{parse_antigravity_keyring_token, AntigravityLogin};

    let body = r#"{"token":{"access_token":"tok-live","token_type":"Bearer","refresh_token":"refresh","expiry":"2099-07-09T16:05:44.2342031+02:00"},"auth_method":"consumer"}"#;

    match parse_antigravity_keyring_token(body).expect("should parse") {
        AntigravityLogin::Token(token) => assert_eq!(token, "tok-live"),
        other => panic!("expected a token, got {other:?}"),
    }
}

#[test]
fn antigravity_keyring_token_reports_an_expired_token_as_expired() {
    use ailimits::providers::antigravity::{parse_antigravity_keyring_token, AntigravityLogin};

    // The token is never sent, but a login exists: the row must keep its
    // last data (greyed, "token expired"), not vanish as "not set up".
    let body = r#"{"token":{"access_token":"tok-expired","token_type":"Bearer","refresh_token":"refresh","expiry":"2000-07-09T16:05:44.2342031+02:00"},"auth_method":"consumer"}"#;
    assert!(matches!(
        parse_antigravity_keyring_token(body).expect("should parse"),
        AntigravityLogin::Expired
    ));

    // No readable expiry: not sent either.
    let undated = r#"{"token":{"access_token":"tok-undated"}}"#;
    assert!(matches!(
        parse_antigravity_keyring_token(undated).unwrap(),
        AntigravityLogin::Expired
    ));

    // Nothing that looks like a session.
    for body in [
        r#"{"auth_method":"consumer"}"#,
        r#"{"token":{"access_token":""}}"#,
    ] {
        assert!(
            matches!(
                parse_antigravity_keyring_token(body).unwrap(),
                AntigravityLogin::Missing
            ),
            "{body} holds no session"
        );
    }
}

#[test]
fn antigravity_legacy_gemini_creds_classify_like_the_keyring() {
    use ailimits::providers::antigravity::{parse_gemini_oauth_creds, AntigravityLogin};
    use chrono::TimeZone;

    let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
    let live = r#"{"access_token":"tok-live","expiry_date":4102444800000}"#;
    assert!(matches!(
        parse_gemini_oauth_creds(live, now).unwrap(),
        AntigravityLogin::Token(ref t) if t == "tok-live"
    ));
    let expired = r#"{"access_token":"tok-old","expiry_date":1759104000000}"#;
    assert!(matches!(
        parse_gemini_oauth_creds(expired, now).unwrap(),
        AntigravityLogin::Expired
    ));
    assert!(matches!(
        parse_gemini_oauth_creds(r#"{"refresh_token":"r"}"#, now).unwrap(),
        AntigravityLogin::Missing
    ));
    assert!(parse_gemini_oauth_creds("{\"half", now).is_err());
}

#[test]
fn antigravity_logins_combine_to_the_most_telling_state() {
    use ailimits::providers::antigravity::{combine_logins, AntigravityLogin};
    use AntigravityLogin::{Expired, Missing, Token};

    let unreadable = || Err(anyhow::anyhow!("unreadable"));
    let token = || Ok(Token("tok".to_string()));

    // A usable token from either source wins.
    assert!(matches!(combine_logins(token(), Ok(Expired)), Ok(Token(_))));
    assert!(matches!(combine_logins(Ok(Expired), token()), Ok(Token(_))));
    assert!(matches!(
        combine_logins(unreadable(), token()),
        Ok(Token(_))
    ));
    // An expired login is not a missing one, whichever source holds it.
    assert!(matches!(
        combine_logins(Ok(Expired), Ok(Missing)),
        Ok(Expired)
    ));
    assert!(matches!(
        combine_logins(Ok(Missing), Ok(Expired)),
        Ok(Expired)
    ));
    assert!(matches!(
        combine_logins(unreadable(), Ok(Expired)),
        Ok(Expired)
    ));
    // An unreadable source alone is an error, never "not set up".
    assert!(combine_logins(unreadable(), Ok(Missing)).is_err());
    assert!(combine_logins(Ok(Missing), unreadable()).is_err());
    // Neither source: the row stays hidden.
    assert!(matches!(
        combine_logins(Ok(Missing), Ok(Missing)),
        Ok(Missing)
    ));
}

#[test]
fn antigravity_login_debug_print_never_shows_the_token() {
    use ailimits::providers::antigravity::AntigravityLogin;

    let printed = format!(
        "{:?}",
        AntigravityLogin::Token("tok-secret-value".to_string())
    );
    assert!(!printed.contains("tok-secret-value"), "{printed}");
}

#[test]
fn antigravity_models_quota_dedupes_shared_pool_and_skips_foreign_models() {
    use ailimits::providers::antigravity::parse_available_models_quota;

    // Real schema captured live 2026-07-09: every Gemini model shares one
    // pool (same fraction + reset); Antigravity also fronts Claude/GPT-OSS
    // pools, which are not Gemini's story.
    let body = r#"{"models": {
        "gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro", "quotaInfo": {"remainingFraction": 0.6300952, "resetTime": "2026-07-16T05:34:22Z"}},
        "gemini-3.1-pro-high": {"displayName": "Gemini 3.1 Pro (High)", "quotaInfo": {"remainingFraction": 0.6300952, "resetTime": "2026-07-16T05:34:22Z"}},
        "gemini-3.5-flash-low": {"displayName": "Gemini 3.5 Flash (Medium)", "quotaInfo": {"remainingFraction": 0.6300952, "resetTime": "2026-07-16T05:34:22Z"}},
        "claude-opus-4-6-thinking": {"displayName": "Claude Opus 4.6 (Thinking)", "quotaInfo": {"remainingFraction": 1, "resetTime": "2026-07-16T16:40:06Z"}},
        "tab_jump_flash_lite_preview": {"quotaInfo": {"remainingFraction": 1}},
        "chat_20706": {}
    }}"#;
    let metrics = parse_available_models_quota(body).expect("should parse");
    // One shared Gemini pool, represented by its newest Pro member.
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].label, "3.1 Pro-High");
    assert_eq!(metrics[0].used, 37); // (1 - 0.6300952) * 100, rounded
    assert!(metrics[0].reset_at.is_some());

    // Unknown shape → empty (the provider reports it as Unusable and moves
    // on; there is no further fallback source).
    assert!(parse_available_models_quota(r#"{"foo": 1}"#)
        .unwrap()
        .is_empty());
}

#[test]
fn hover_reason_explains_stale_rows_only() {
    use ailimits::providers::{
        Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
    };
    use ailimits::ui::renderer::hover_reason;
    use chrono::{Duration, Utc};
    use std::collections::HashMap;

    let stale = |id: ProviderId| ProviderData {
        id,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 48,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() + Duration::hours(2)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now() - Duration::hours(10),
        received_at: None, // wall-age staleness (10 h)
    };

    let mut errors = HashMap::new();
    errors.insert(ProviderId::Claude, "token expired".to_string());
    errors.insert(
        ProviderId::Codex,
        "token expired — run Codex CLI once".to_string(),
    );

    // Stored cause + appended per-provider action.
    assert_eq!(
        hover_reason(&stale(ProviderId::Claude), &errors).as_deref(),
        Some("token expired — run Claude Code")
    );
    // A message that already carries its action is shown verbatim.
    assert_eq!(
        hover_reason(&stale(ProviderId::Codex), &errors).as_deref(),
        Some("token expired — run Codex CLI once")
    );
    // No stored error (e.g. cache-loaded after a restart) → generic cause.
    assert_eq!(
        hover_reason(&stale(ProviderId::Antigravity), &errors).as_deref(),
        Some("no fresh data — run Antigravity")
    );

    // Fresh data → no reason; hover keeps its weekly-metric meaning.
    let fresh = ProviderData {
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
        ..stale(ProviderId::Claude)
    };
    assert_eq!(hover_reason(&fresh, &errors), None);
}

#[test]
fn antigravity_load_code_assist_project_parses() {
    use ailimits::providers::antigravity::parse_load_code_assist_project;

    // Shape verified live 2026-07-09.
    let body = r#"{"currentTier": {"id": "free-tier"}, "cloudaicompanionProject": "sacred-airline-12rdt", "gcpManaged": false}"#;
    assert_eq!(
        parse_load_code_assist_project(body).as_deref(),
        Some("sacred-airline-12rdt")
    );
    assert_eq!(
        parse_load_code_assist_project(r#"{"gcpManaged": false}"#),
        None
    );
    assert_eq!(
        parse_load_code_assist_project(r#"{"cloudaicompanionProject": ""}"#),
        None
    );
}

#[test]
fn stale_data_with_passed_reset_extrapolates_to_zero() {
    use ailimits::providers::{
        Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
    };
    use chrono::{Duration, Utc};

    // Data from 10 minutes ago; the session reset 5 minutes ago,
    // the weekly reset is in the future.
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![
            Metric {
                label: "Session".to_string(),
                used: 100,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(Utc::now() - Duration::minutes(5)),
                window: MetricWindow::Session,
            },
            Metric {
                label: "Weekly".to_string(),
                used: 28,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(Utc::now() + Duration::days(3)),
                window: MetricWindow::Session,
            },
        ],
        updated_at: Utc::now() - Duration::minutes(10),
        // No monotonic anchor → staleness falls back to the 10-min wall age.
        received_at: None,
    };

    let aged = data.aged_for_display();
    // The session rolled over — an estimate; the weekly metric is intact.
    assert!(matches!(aged.status, ProviderStatus::Estimated));
    assert_eq!(aged.metrics[0].used, 0);
    assert!(aged.metrics[0].reset_at.is_none());
    assert_eq!(aged.metrics[1].used, 28);
    assert!(aged.metrics[1].reset_at.is_some());
}

#[test]
fn fresh_data_is_not_extrapolated() {
    use ailimits::providers::{
        Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
    };
    use chrono::{Duration, Utc};

    // Fresh data (30s old) is left intact even with a reset in the past:
    // the next fetch will refine it.
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 80,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() - Duration::seconds(10)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now() - Duration::seconds(30),
        received_at: Some(std::time::Instant::now()),
    };

    let aged = data.aged_for_display();
    assert!(matches!(aged.status, ProviderStatus::Ok));
    assert_eq!(aged.metrics[0].used, 80);
}

#[test]
fn primary_percentage_uses_first_metric() {
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![metric(50, Some(100)), metric(10, Some(100))],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(data.primary_percentage(), Some(50.0));
}

// Helper: a percent metric with an explicit window.
fn window_metric(label: &str, used: u64, window: MetricWindow) -> Metric {
    Metric {
        label: label.to_string(),
        used,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: None,
        window,
    }
}

#[test]
fn an_exhausted_long_window_becomes_the_headline_for_every_surface() {
    // Claude shape: the weekly cap is spent, so no new session can start —
    // the tray, the taskbar panel and the toast must all read 100%, not 20%.
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![
            window_metric("Session", 20, MetricWindow::Session),
            window_metric("Weekly", 100, MetricWindow::Long),
        ],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(
        data.headline_metric().map(|m| m.label.as_str()),
        Some("Weekly")
    );
    assert_eq!(data.primary_percentage(), Some(100.0));
}

#[test]
fn an_exhausted_opus_pool_counts_even_though_its_label_never_says_week() {
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![
            window_metric("Session", 10, MetricWindow::Session),
            window_metric("Weekly", 50, MetricWindow::Long),
            window_metric("Opus", 100, MetricWindow::Long),
        ],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(
        data.headline_metric().map(|m| m.label.as_str()),
        Some("Opus")
    );
}

#[test]
fn a_long_window_with_headroom_leaves_the_session_in_charge() {
    let data = ProviderData {
        id: ProviderId::Codex,
        status: ProviderStatus::Ok,
        metrics: vec![
            window_metric("Session", 40, MetricWindow::Session),
            window_metric("Weekly", 60, MetricWindow::Long),
        ],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(
        data.headline_metric().map(|m| m.label.as_str()),
        Some("Session")
    );
    assert_eq!(data.primary_percentage(), Some(40.0));
}

#[test]
fn headline_reset_quotes_the_spent_long_windows_reset_not_the_soonest() {
    use chrono::Duration;
    // Claude shape: the Session window resets in an hour but the spent Weekly
    // window (the headline, since it's exhausted) resets in five days. The
    // notification must quote the Weekly reset — quoting the Session reset
    // would tell a blocked user they are unblocked in an hour when they are
    // not.
    let now = Utc::now();
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![
            Metric {
                label: "Session".to_string(),
                used: 20,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(now + Duration::hours(1)),
                window: MetricWindow::Session,
            },
            Metric {
                label: "Weekly".to_string(),
                used: 100,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(now + Duration::days(5)),
                window: MetricWindow::Long,
            },
        ],
        updated_at: now,
        received_at: Some(std::time::Instant::now()),
    };
    let long_reset = data.metrics[1].reset_at.unwrap();
    let session_reset = data.metrics[0].reset_at.unwrap();
    assert_eq!(data.headline_reset(), Some(long_reset));
    assert_ne!(data.headline_reset(), Some(session_reset));
    // next_reset() itself would have picked the sooner Session reset — this
    // is exactly the reset headline_reset() must NOT quote here.
    assert_eq!(data.next_reset(), Some(session_reset));
}

#[test]
fn headline_reset_falls_back_to_the_nearest_reset_when_no_spent_long_window() {
    use chrono::Duration;
    // No spent long window, so the headline is the Session metric itself —
    // headline_reset() must agree with next_reset() (the nearest future
    // reset), same as before this fix.
    let now = Utc::now();
    let data = ProviderData {
        id: ProviderId::Codex,
        status: ProviderStatus::Ok,
        metrics: vec![
            Metric {
                label: "Session".to_string(),
                used: 40,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(now + Duration::hours(1)),
                window: MetricWindow::Session,
            },
            Metric {
                label: "Weekly".to_string(),
                used: 60,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(now + Duration::days(5)),
                window: MetricWindow::Long,
            },
        ],
        updated_at: now,
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(data.headline_reset(), data.next_reset());
    assert_eq!(data.headline_reset(), data.metrics[0].reset_at);
}

#[test]
fn live_data_survives_a_wall_clock_jump() {
    use chrono::Duration;
    // Models an NTP / VM-resume forward jump: data RECEIVED live just now (fresh
    // monotonic stamp) whose wall-clock updated_at suddenly reads an hour old.
    // It must NOT grey and must NOT fabricate an estimate — the staleness gate
    // is monotonic for live data.
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 50,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() + Duration::hours(2)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now() - Duration::hours(1),
        received_at: Some(std::time::Instant::now()),
    };
    assert!(
        data.stale_age_secs().is_none(),
        "fresh live data must not read as stale after a clock jump"
    );
    assert!(matches!(data.aged_for_display().status, ProviderStatus::Ok));
}

#[test]
fn data_without_monotonic_anchor_uses_wall_age() {
    use chrono::Duration;
    // A disk-cache entry (received_at None) is staled by its own wall-clock
    // age: a fresh entry is live, an old one is stale.
    let mk = |age_secs: i64| ProviderData {
        id: ProviderId::Codex,
        status: ProviderStatus::Ok,
        metrics: vec![metric(10, Some(100))],
        updated_at: Utc::now() - Duration::seconds(age_secs),
        received_at: None,
    };
    assert!(mk(10).stale_age_secs().is_none());
    assert!(mk(600).stale_age_secs().is_some());
}

#[test]
fn healthy_data_on_a_long_update_interval_is_not_stale() {
    use ailimits::providers::{stale_after_secs, STALE_AFTER_SECS};
    use chrono::Duration;

    let aged = |minutes: i64| ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![metric(10, Some(100))],
        updated_at: Utc::now() - Duration::minutes(minutes),
        received_at: None,
    };

    // Ten minutes after the last fetch on the 15-minute menu interval: the
    // next refresh is still due, the value has not stopped updating.
    assert!(aged(10)
        .stale_age_secs_after(stale_after_secs(900))
        .is_none());
    // The same age on the 1-minute interval has missed several refreshes.
    assert!(aged(10)
        .stale_age_secs_after(stale_after_secs(60))
        .is_some());
    // A missed 15-minute refresh is stale too.
    assert!(aged(17)
        .stale_age_secs_after(stale_after_secs(900))
        .is_some());

    // Short intervals keep the five-minute floor; nothing overflows.
    assert_eq!(stale_after_secs(0), STALE_AFTER_SECS);
    assert_eq!(stale_after_secs(60), STALE_AFTER_SECS);
    assert!(stale_after_secs(1800) > 1800);
    assert_eq!(stale_after_secs(u64::MAX), i64::MAX);
}

// The idle-pause rule, on injected monotonic times (no sleeping): `t(s)` is
// `s` seconds after a fixed origin.
fn pause_clock() -> impl Fn(u64) -> std::time::Instant {
    let origin = std::time::Instant::now();
    move |secs| origin + std::time::Duration::from_secs(secs)
}

#[test]
fn the_idle_pause_does_not_age_live_data() {
    use ailimits::providers::PollingPause;
    let t = pause_clock();
    let mut pause = PollingPause::default();
    // Fetched at 0; the user leaves and polling pauses a minute later.
    pause.mark(true, t(60));
    // Hours away: never stale while paused.
    assert!(!pause.is_stale(t(0), t(4 * 3600), 300));
    // Back: the seconds before the resume fetch lands are not stale either.
    pause.mark(false, t(4 * 3600));
    assert!(!pause.is_stale(t(0), t(4 * 3600 + 10), 300));
    // Only active time counts: 60 s before the pause + 240 s after it is
    // exactly the limit, one more second is over it.
    assert!(!pause.is_stale(t(0), t(4 * 3600 + 240), 300));
    assert!(pause.is_stale(t(0), t(4 * 3600 + 241), 300));
}

#[test]
fn data_already_stale_before_the_pause_stays_stale_after_it() {
    use ailimits::providers::PollingPause;
    let t = pause_clock();
    let mut pause = PollingPause::default();
    // A failing provider: its last good data is from 0 and it was grey by
    // the time the user left at 600.
    assert!(pause.is_stale(t(0), t(600), 300));
    pause.mark(true, t(600));
    pause.mark(false, t(3600));
    // The resume fetch fails again: the retained value must not turn live.
    assert!(pause.is_stale(t(0), t(3601), 300));
}

#[test]
fn without_a_pause_live_data_ages_as_before() {
    use ailimits::providers::PollingPause;
    let t = pause_clock();
    let pause = PollingPause::default();
    // A fetch that keeps failing while the user is active still greys.
    assert!(!pause.is_stale(t(0), t(300), 300));
    assert!(pause.is_stale(t(0), t(301), 300));
    // Data fetched after the last pause ages by its full elapsed time.
    let mut pause = PollingPause::default();
    pause.mark(true, t(0));
    pause.mark(false, t(1000));
    assert!(pause.is_stale(t(1100), t(1401), 300));
    assert!(!pause.is_stale(t(1100), t(1400), 300));
}

#[test]
fn data_received_during_a_pause_ages_from_the_resume() {
    use ailimits::providers::PollingPause;
    let t = pause_clock();
    let mut pause = PollingPause::default();
    // An out-of-band fetch (a menu action) can land while the scheduler
    // still counts the user as away.
    pause.mark(true, t(0));
    pause.mark(false, t(1000));
    assert!(!pause.is_stale(t(500), t(1300), 300));
    assert!(pause.is_stale(t(500), t(1301), 300));
}

#[test]
fn pause_marks_keep_the_first_start_and_the_last_pause() {
    use ailimits::providers::PollingPause;
    let t = pause_clock();
    let mut pause = PollingPause::default();
    // Resuming without a pause changes nothing.
    pause.mark(false, t(5));
    assert_eq!(pause, PollingPause::default());
    // A repeated pause keeps its first start.
    pause.mark(true, t(10));
    pause.mark(true, t(30));
    assert_eq!(pause.paused_since, Some(t(10)));
    pause.mark(false, t(100));
    assert_eq!(pause.paused_since, None);
    assert_eq!(pause.last, Some((t(10), t(100))));
    // The next pause replaces the remembered one only when it ends.
    pause.mark(true, t(500));
    assert_eq!(pause.last, Some((t(10), t(100))));
    pause.mark(false, t(900));
    assert_eq!(pause.last, Some((t(500), t(900))));
}

#[test]
fn marginally_past_reset_within_grace_is_not_extrapolated() {
    use chrono::Duration;
    // Stale data whose reset is only ~30s past — inside the grace window, so it
    // could be a wall-clock skew rather than a real rollover. It must NOT become
    // an ≈0% estimate; the value is preserved (the renderer greys it instead).
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 95,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() - Duration::seconds(30)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now() - Duration::minutes(10),
        received_at: None,
    };
    let aged = data.aged_for_display();
    assert!(
        matches!(aged.status, ProviderStatus::Ok),
        "a reset within the grace window must not become an estimate"
    );
    assert_eq!(
        aged.metrics[0].used, 95,
        "the value must be preserved, not zeroed"
    );
}

#[test]
fn a_reset_passed_during_the_idle_pause_still_reads_as_zero() {
    use chrono::Duration;
    // Live data received just now (so it is not stale - e.g. the idle pause
    // kept it from ageing) whose session window provably reset 10 minutes
    // ago: the row must show the ≈0% estimate, not the old 95% as live.
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 95,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() - Duration::minutes(10)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert!(data.stale_age_secs().is_none(), "fresh data is not stale");
    let aged = data.aged_for_display();
    assert!(matches!(aged.status, ProviderStatus::Estimated));
    assert_eq!(aged.metrics[0].used, 0);
    assert_eq!(aged.metrics[0].reset_at, None);
}

#[test]
fn next_reset_skips_past_timestamps() {
    use chrono::{Duration, Utc};

    let future = Utc::now() + Duration::hours(3);
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![
            // Already rolled over — must not count as the "next" reset.
            Metric {
                label: "Session".to_string(),
                used: 80,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(Utc::now() - Duration::minutes(5)),
                window: MetricWindow::Session,
            },
            Metric {
                label: "Weekly".to_string(),
                used: 30,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: Some(future),
                window: MetricWindow::Session,
            },
        ],
        updated_at: Utc::now(),
        received_at: None,
    };
    assert_eq!(data.next_reset(), Some(future));

    let all_past = ProviderData {
        metrics: vec![Metric {
            label: "Session".to_string(),
            used: 80,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() - Duration::minutes(5)),
            window: MetricWindow::Session,
        }],
        ..data
    };
    assert_eq!(all_past.next_reset(), None);
}

#[test]
fn claude_marks_every_seven_day_window_as_long() {
    use ailimits::providers::claude::parse_oauth_usage;
    use ailimits::providers::MetricWindow;
    let body = r#"{"five_hour": {"utilization": 20.0, "resets_at": "2026-08-02T10:00:00Z"},
        "seven_day": {"utilization": 100.0, "resets_at": "2026-08-07T10:00:00Z"},
        "seven_day_opus": {"utilization": 100.0, "resets_at": "2026-08-07T10:00:00Z"},
        "seven_day_sonnet": {"utilization": 40.0, "resets_at": "2026-08-07T10:00:00Z"}}"#;
    let metrics = parse_oauth_usage(body).expect("should parse");
    assert_eq!(metrics.len(), 4);
    assert_eq!(metrics[0].label, "Session");
    assert_eq!(metrics[0].window, MetricWindow::Session);
    // Opus and Sonnet are seven-day pools too; their labels never say "week",
    // which is exactly what the old label-sniffing rule missed.
    for m in &metrics[1..] {
        assert_eq!(
            m.window,
            MetricWindow::Long,
            "{} must be a long window",
            m.label
        );
    }
}

#[test]
fn codex_marks_the_secondary_window_as_long() {
    use ailimits::providers::codex::parse_wham_usage;
    use ailimits::providers::MetricWindow;
    let body = r#"{"rate_limit": {
        "primary_window": {"used_percent": 30, "reset_at": 1786000000},
        "secondary_window": {"used_percent": 100, "reset_at": 1786600000}}}"#;
    let metrics = parse_wham_usage(body).expect("should parse");
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].window, MetricWindow::Session);
    assert_eq!(metrics[1].window, MetricWindow::Long);
}

#[test]
fn antigravity_quota_summary_parses_shared_pools() {
    use ailimits::providers::antigravity::parse_quota_summary;
    use ailimits::providers::MetricWindow;
    // Verified live 2026-08-01 (field names and group names are real).
    let body = r#"{"groups": [
        {"displayName": "Gemini Models", "buckets": [
            {"bucketId": "gemini", "displayName": "Weekly Limit", "window": "WEEKLY",
             "resetTime": "2026-08-07T18:30:57Z", "remainingFraction": 0}]},
        {"displayName": "Claude and GPT models", "buckets": [
            {"bucketId": "claude_gpt", "displayName": "Weekly Limit", "window": "WEEKLY",
             "resetTime": "2026-08-07T19:41:22Z", "remainingFraction": 0.873262}]}]}"#;
    let metrics = parse_quota_summary(body).expect("should parse");
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].label, "Gemini");
    assert_eq!(metrics[0].used, 100, "a spent pool must read 100% used");
    assert_eq!(metrics[0].window, MetricWindow::Long);
    assert_eq!(metrics[1].label, "Claude/GPT");
    assert_eq!(metrics[1].used, 13);
}

#[test]
fn antigravity_quota_summary_refuses_the_project_less_default_view() {
    use ailimits::providers::antigravity::parse_quota_summary;
    // Without a project id Google answers one synthetic "All Models" group
    // with everything full. Rendering it would claim a full quota that may be
    // entirely spent, so the parser must yield nothing.
    let body = r#"{"groups": [{"displayName": "All Models", "buckets": [
        {"remainingFraction": 1}, {"remainingFraction": 1}]}]}"#;
    assert!(parse_quota_summary(body).unwrap().is_empty());
}

#[test]
fn antigravity_quota_summary_treats_a_missing_fraction_with_a_reset_as_exhausted() {
    use ailimits::providers::antigravity::parse_quota_summary;
    let body = r#"{"groups": [{"displayName": "Gemini Models", "buckets": [
        {"resetTime": "2026-08-07T18:30:57Z"}]}]}"#;
    let metrics = parse_quota_summary(body).expect("should parse");
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].used, 100);
}

#[test]
fn antigravity_quota_summary_ignores_an_unknown_shape() {
    use ailimits::providers::antigravity::parse_quota_summary;
    assert!(parse_quota_summary(r#"{"foo": 1}"#).unwrap().is_empty());
}

// The billing body verified live 2026-11-10, after some Grok Build use.
const GROK_BILLING_FULL: &str = r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-11-09T23:01:28.407+00:00","end":"2026-11-16T23:01:28.407+00:00"},
 "creditUsagePercent":1.0,
 "productUsage":[{"product":"GrokBuild","usagePercent":1.0}],
 "isUnifiedBillingUser":true,"prepaidBalance":{"val":0},"topUpMethod":"TOP_UP_METHOD_SAVED_PAYMENT_METHOD",
 "billingPeriodStart":"2026-11-09T23:01:28.407+00:00","billingPeriodEnd":"2026-11-16T23:01:28.407+00:00"}}"#;

#[test]
fn grok_billing_full_body_parses_to_the_weekly_limit_and_its_products() {
    use ailimits::providers::grok::parse_billing_credits;
    use chrono::{TimeZone, Timelike};

    let metrics = parse_billing_credits(GROK_BILLING_FULL).expect("should parse");
    assert_eq!(metrics.len(), 2);

    // The whole subscription limit leads: it is what every surface shows.
    assert_eq!(metrics[0].label, "Weekly");
    assert_eq!(metrics[0].used, 1);
    assert_eq!(metrics[0].limit, Some(100));
    assert_eq!(metrics[0].window, MetricWindow::Long);
    let reset = metrics[0].reset_at.expect("currentPeriod.end is the reset");
    assert_eq!(
        reset.with_nanosecond(0),
        Utc.with_ymd_and_hms(2026, 11, 16, 23, 1, 28).single()
    );

    // The per-product share rides along, compactly labelled.
    assert_eq!(metrics[1].label, "Build");
    assert_eq!(metrics[1].used, 1);
    assert_eq!(metrics[1].window, MetricWindow::Long);
    assert_eq!(metrics[1].reset_at, Some(reset));

    let data = ProviderData {
        id: ProviderId::Grok,
        status: ProviderStatus::Ok,
        metrics,
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(
        data.headline_metric().map(|m| m.label.as_str()),
        Some("Weekly")
    );
}

#[test]
fn grok_billing_without_the_percent_fields_reads_zero_beside_a_valid_period() {
    use ailimits::providers::grok::{billing_outcome, parse_billing_credits};

    // Before any use: protobuf JSON leaves the zero percent and the empty
    // product list out entirely (verified live 2026-11-10).
    let body = r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-11-09T23:01:28.407+00:00","end":"2026-11-16T23:01:28.407+00:00"},
        "isUnifiedBillingUser":true,"prepaidBalance":{"val":0},
        "billingPeriodStart":"2026-11-09T23:01:28.407+00:00","billingPeriodEnd":"2026-11-16T23:01:28.407+00:00"}}"#;
    let metrics = parse_billing_credits(body).expect("should parse");
    assert_eq!(metrics.len(), 1, "no products without a product list");
    assert_eq!(metrics[0].label, "Weekly");
    assert_eq!(metrics[0].used, 0);
    assert!(metrics[0].reset_at.is_some());

    let (status, metrics) = billing_outcome(200, body);
    assert!(matches!(status, ProviderStatus::Ok));
    assert_eq!(metrics[0].used, 0);
}

#[test]
fn grok_billing_reset_falls_back_to_the_billing_period_end() {
    use ailimits::providers::grok::parse_billing_credits;

    let body = r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-11-09T23:01:28Z"},
        "creditUsagePercent":42.4,"billingPeriodEnd":"2026-11-16T23:01:28Z"}}"#;
    let metrics = parse_billing_credits(body).expect("should parse");
    assert_eq!(metrics[0].used, 42);
    assert_eq!(
        metrics[0].reset_at.map(|t| t.to_rfc3339()).as_deref(),
        Some("2026-11-16T23:01:28+00:00")
    );
}

#[test]
fn grok_billing_names_an_unknown_period_for_what_it_is() {
    use ailimits::providers::grok::parse_billing_credits;

    let body = r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_SOMETHING_NEW","end":"2026-11-16T23:01:28Z"},
        "creditUsagePercent":250,"productUsage":[{"product":"Chat"},{"product":"GrokBuild","usagePercent":"bad"}]}}"#;
    let metrics = parse_billing_credits(body).expect("should parse");
    assert_eq!(metrics[0].label, "Limit");
    assert_eq!(metrics[0].used, 100, "clamped to the scale");
    // A product without a percent is an omitted zero; one with garbage is
    // dropped rather than guessed.
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[1].label, "Chat");
    assert_eq!(metrics[1].used, 0);
}

#[test]
fn grok_billing_malformed_or_unknown_bodies_degrade_to_an_honest_error() {
    use ailimits::providers::grok::{billing_outcome, parse_billing_credits};

    // Not JSON at all (an HTML error page behind a 200).
    assert!(parse_billing_credits("<html>oops</html>").is_err());
    // The plain /v1/billing shape (monthly API spend) and other strangers:
    // no period and no percent, so no number may be shown.
    for body in [
        r#"{"monthlyLimit":{"val":0},"used":{"val":0},"onDemandCap":{"val":0}}"#,
        r#"{"config":{"monthlyLimit":{"val":0},"used":{"val":0}}}"#,
        r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY"}}}"#,
        r#"{"config":[]}"#,
        r#"[]"#,
    ] {
        assert!(
            parse_billing_credits(body).unwrap().is_empty(),
            "{body} must not yield metrics"
        );
    }

    for body in ["<html>oops</html>", r#"{"foo": 1}"#] {
        let (status, metrics) = billing_outcome(200, body);
        assert!(metrics.is_empty());
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if m == "unrecognized billing schema"),
            "got {status:?}"
        );
    }
}

#[test]
fn grok_billing_non_200_degrades_to_an_honest_error() {
    use ailimits::providers::grok::billing_outcome;

    for (code, want) in [
        (401, "token rejected"),
        (403, "token rejected"),
        (429, "HTTP 429"),
        (500, "HTTP 500"),
        (302, "HTTP 302"),
    ] {
        // Even a body that would parse is ignored off a non-200.
        let (status, metrics) = billing_outcome(code, GROK_BILLING_FULL);
        assert!(metrics.is_empty(), "HTTP {code} must not yield metrics");
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if m == want),
            "HTTP {code}: got {status:?}"
        );
    }
}

#[test]
fn grok_auth_picks_the_live_session_and_never_an_expired_one() {
    use ailimits::providers::grok::{parse_grok_auth, GrokLogin};
    use chrono::TimeZone;

    let now = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
    // The verified shape: sessions keyed "<issuer>::<client-id>".
    let body = r#"{
        "https://auth.x.ai::client-a": {"key": "tok-old", "refresh_token": "r1", "auth_mode": "oidc",
            "expires_at": "2026-11-09T05:43:16.908847600Z", "oidc_issuer": "https://auth.x.ai"},
        "https://auth.x.ai::client-b": {"key": "tok-live", "refresh_token": "r2", "auth_mode": "oidc",
            "expires_at": "2026-11-10T05:43:16.908847600Z", "oidc_issuer": "https://auth.x.ai"},
        "https://idp.example::client-c": {"key": "tok-undated", "auth_mode": "oidc"}
    }"#;
    match parse_grok_auth(body, now).expect("should parse") {
        GrokLogin::Token { key, expires_at } => {
            assert_eq!(key, "tok-live", "a stated, later expiry wins");
            assert!(expires_at.is_some_and(|t| t > now));
        }
        other => panic!("expected a token, got {other:?}"),
    }

    // Every session lapsed: a login exists, so this is not "no login".
    let expired =
        r#"{"https://auth.x.ai::a": {"key": "tok-old", "expires_at": "2026-11-09T05:43:16Z"}}"#;
    assert!(matches!(
        parse_grok_auth(expired, now).unwrap(),
        GrokLogin::Expired
    ));

    // A session with no stated expiry is left to the server to judge.
    let undated = r#"{"https://auth.x.ai::a": {"key": "tok-undated"}}"#;
    assert!(matches!(
        parse_grok_auth(undated, now).unwrap(),
        GrokLogin::Token {
            expires_at: None,
            ..
        }
    ));

    // Signed out, or nothing that looks like a session.
    for body in ["{}", r#"{"https://auth.x.ai::a": {"key": ""}}"#, "[]"] {
        assert!(
            matches!(parse_grok_auth(body, now).unwrap(), GrokLogin::Missing),
            "{body} holds no session"
        );
    }

    // Not JSON — an error the provider reports as an unreadable file.
    assert!(parse_grok_auth("{\"half", now).is_err());
}

#[test]
fn grok_auth_debug_print_never_shows_the_token() {
    use ailimits::providers::grok::parse_grok_auth;

    let body = r#"{"https://auth.x.ai::a": {"key": "tok-secret-value", "expires_at": "2099-01-01T00:00:00Z"}}"#;
    let login = parse_grok_auth(body, Utc::now()).unwrap();
    let printed = format!("{login:?}");
    assert!(!printed.contains("tok-secret-value"), "{printed}");
    assert!(printed.contains("Token"));
}

#[test]
fn grok_stale_rows_point_at_the_grok_cli() {
    use ailimits::ui::renderer::hover_reason;
    use chrono::Duration;
    use std::collections::HashMap;

    let stale = ProviderData {
        id: ProviderId::Grok,
        status: ProviderStatus::Ok,
        metrics: vec![window_metric("Weekly", 30, MetricWindow::Long)],
        updated_at: Utc::now() - Duration::hours(7),
        received_at: None,
    };
    let mut errors = HashMap::new();
    errors.insert(ProviderId::Grok, "token expired".to_string());
    assert_eq!(
        hover_reason(&stale, &errors).as_deref(),
        Some("token expired — run Grok CLI")
    );
}

#[test]
fn antigravity_models_quota_treats_a_missing_fraction_as_exhausted() {
    use ailimits::providers::antigravity::parse_available_models_quota;
    // Verified live 2026-08-01: every gemini model reported quotaInfo with a
    // resetTime and no remainingFraction while the weekly pool was spent.
    let body = r#"{"models": {
        "gemini-3.6-flash-high": {"displayName": "Gemini 3.6 Flash",
            "quotaInfo": {"resetTime": "2026-08-07T18:30:57Z"}},
        "gemini-2.5-pro": {"displayName": "Gemini 2.5 Pro",
            "quotaInfo": {"resetTime": "2026-08-07T18:30:57Z"}}}}"#;
    let metrics = parse_available_models_quota(body).expect("should parse");
    assert_eq!(metrics.len(), 1, "one shared pool, deduped");
    assert_eq!(metrics[0].used, 100);
}

// ─── Cursor ───────────────────────────────────────────────────────

/// The usage-summary body verified live on a free account, 2026-11-10.
const CURSOR_FREE_BODY: &str = r#"{"billingCycleStart":"2026-10-28T20:49:51.046Z","billingCycleEnd":"2026-11-28T20:49:51.046Z","membershipType":"free","limitType":"user","isUnlimited":false,"autoModelSelectedDisplayMessage":"You've used 0% of your included total usage","namedModelSelectedDisplayMessage":"You've used 0% of your included API usage","individualUsage":{"plan":{"enabled":true,"used":0,"limit":0,"remaining":0,"breakdown":{"included":0,"bonus":0,"total":0},"autoPercentUsed":0,"apiPercentUsed":0,"totalPercentUsed":0},"onDemand":{"enabled":false,"used":0,"limit":null,"remaining":null}},"teamUsage":{}}"#;

#[test]
fn cursor_usage_summary_parses_the_verified_free_body() {
    use ailimits::providers::cursor::parse_usage_summary;
    use chrono::{TimeZone, Timelike};

    let metrics = parse_usage_summary(CURSOR_FREE_BODY).expect("should parse");
    assert_eq!(metrics.len(), 1, "one monthly window");
    let m = &metrics[0];
    assert_eq!(m.label, "Free", "membershipType labels the window");
    assert_eq!(m.used, 0);
    assert_eq!(m.limit, Some(100));
    assert!(matches!(m.unit, MetricUnit::Percent));
    assert_eq!(m.window, MetricWindow::Session);
    let reset = m.reset_at.expect("billingCycleEnd is the reset");
    assert_eq!(
        reset.with_nanosecond(0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 28, 20, 49, 51).unwrap()
    );
}

#[test]
fn cursor_usage_summary_parses_a_paid_body_with_usage() {
    use ailimits::providers::cursor::parse_usage_summary;
    // The same shape filled in the way a paid plan fills it: amounts in cents,
    // percentages 0–100, a fractional total.
    let body = r#"{"billingCycleStart":"2026-11-02T09:00:00.000Z","billingCycleEnd":"2026-12-02T09:00:00.000Z",
        "membershipType":"pro","limitType":"user","isUnlimited":false,
        "individualUsage":{"plan":{"enabled":true,"used":1450,"limit":2000,"remaining":550,
            "breakdown":{"included":2000,"bonus":0,"total":2000},
            "autoPercentUsed":31.5,"apiPercentUsed":88.2,"totalPercentUsed":72.5},
          "onDemand":{"enabled":true,"used":0,"limit":5000,"remaining":5000}},
        "teamUsage":{}}"#;
    let metrics = parse_usage_summary(body).expect("should parse");
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].label, "Pro");
    // 72.5 rounds half away from zero.
    assert_eq!(metrics[0].used, 73);
    assert_eq!(metrics[0].percentage(), Some(73.0));
    assert!(metrics[0].reset_at.is_some());
}

#[test]
fn cursor_usage_summary_falls_back_to_used_over_limit() {
    use ailimits::providers::cursor::parse_usage_summary;
    let body = r#"{"membershipType":"pro","billingCycleEnd":"2026-12-02T09:00:00Z",
        "individualUsage":{"plan":{"enabled":true,"used":500,"limit":2000}}}"#;
    let metrics = parse_usage_summary(body).expect("should parse");
    assert_eq!(metrics[0].used, 25);
}

#[test]
fn cursor_usage_summary_clamps_overage_to_the_limit() {
    use ailimits::providers::cursor::parse_usage_summary;
    let body = r#"{"membershipType":"pro","individualUsage":{"plan":{"enabled":true,"totalPercentUsed":134.0}}}"#;
    let metrics = parse_usage_summary(body).expect("should parse");
    assert_eq!(metrics[0].used, 100);
    assert_eq!(metrics[0].label, "Pro");
    assert!(
        metrics[0].reset_at.is_none(),
        "no billingCycleEnd, no reset"
    );
}

#[test]
fn cursor_unlimited_plan_is_an_unlimited_row_not_zero_percent() {
    use ailimits::providers::cursor::parse_usage_summary;

    let body = r#"{"billingCycleEnd":"2026-11-28T20:49:51.046Z","membershipType":"enterprise",
        "isUnlimited":true,
        "individualUsage":{"plan":{"enabled":true,"totalPercentUsed":0}}}"#;
    let metrics = parse_usage_summary(body).expect("should parse");
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].label, "Unlimited");
    assert_eq!(metrics[0].limit, None);
    // No percentage: every surface shows a status, never a 0% bar.
    assert_eq!(metrics[0].percentage(), None);
    // No reset: the hint line must not quote the row against a countdown.
    assert!(metrics[0].reset_at.is_none());

    let data = ProviderData {
        id: ProviderId::Cursor,
        status: ProviderStatus::Ok,
        metrics,
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    };
    assert_eq!(data.primary_percentage(), None);
}

#[test]
fn cursor_usage_summary_refuses_malformed_and_unknown_bodies() {
    use ailimits::providers::cursor::parse_usage_summary;
    // Not JSON at all (an HTML error page answered with 200).
    assert!(parse_usage_summary("<!DOCTYPE html><html></html>").is_err());
    // JSON of another shape.
    assert!(parse_usage_summary(r#"{"foo": 1}"#).unwrap().is_empty());
    assert!(parse_usage_summary("[]").unwrap().is_empty());
    // The plan object without any usable number.
    assert!(
        parse_usage_summary(r#"{"individualUsage":{"plan":{"enabled":true,"limit":0}}}"#)
            .unwrap()
            .is_empty()
    );
    // A disabled individual plan says nothing about this account.
    assert!(parse_usage_summary(
        r#"{"individualUsage":{"plan":{"enabled":false,"totalPercentUsed":0}}}"#
    )
    .unwrap()
    .is_empty());
}

#[test]
fn cursor_answers_map_to_honest_statuses() {
    use ailimits::providers::cursor::interpret_usage_summary;

    let (status, metrics) = interpret_usage_summary(200, CURSOR_FREE_BODY);
    assert!(matches!(status, ProviderStatus::Ok));
    assert_eq!(metrics.len(), 1);

    // 200 with a body we cannot read: an error, never a guessed number.
    for body in ["not json", r#"{"foo": 1}"#] {
        let (status, metrics) = interpret_usage_summary(200, body);
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if m == "unrecognized usage schema"),
            "{body}: {status:?}"
        );
        assert!(metrics.is_empty());
    }

    // The session is no longer accepted.
    for code in [401, 403] {
        let (status, metrics) = interpret_usage_summary(code, "");
        assert!(matches!(&status, ProviderStatus::NetworkError(m) if m == "token rejected"));
        assert!(metrics.is_empty());
    }

    // Anything else, a redirect included, is the bare status.
    for code in [302, 429, 500, 503] {
        let (status, metrics) = interpret_usage_summary(code, CURSOR_FREE_BODY);
        assert!(
            matches!(&status, ProviderStatus::NetworkError(m) if *m == format!("HTTP {code}")),
            "{code}: {status:?}"
        );
        assert!(metrics.is_empty(), "a non-200 body is never parsed");
    }
}

#[test]
fn cursor_session_claims_come_from_the_token_payload() {
    use ailimits::providers::cursor::parse_session_claims;
    use chrono::TimeZone;

    // Fake JWT-shaped tokens. Payloads, in order:
    // {"sub":"github|user_01TESTFIXTURE","exp":4102444800,"iss":…,"aud":…}
    let valid = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dXNlcl8wMVRFU1RGSVhUVVJFIiwiZXhwIjo0MTAyNDQ0ODAwLCJpc3MiOiJodHRwczovL2F1dGhlbnRpY2F0aW9uLmN1cnNvci5zaCIsImF1ZCI6Imh0dHBzOi8vY3Vyc29yLmNvbSJ9.c2ln";
    // {"sub":"github|user_01TESTFIXTURE","exp":1759104000} (2025-09-29)
    let expired = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJnaXRodWJ8dXNlcl8wMVRFU1RGSVhUVVJFIiwiZXhwIjoxNzU5MTA0MDAwfQ.c2ln";
    // {"sub":"user_01PLAIN","exp":4102444800}
    let no_provider = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyXzAxUExBSU4iLCJleHAiOjQxMDI0NDQ4MDB9.c2ln";
    // {"exp":4102444800}
    let no_sub = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjQxMDI0NDQ4MDB9.c2ln";
    // {"sub":"auth0|user_01NOEXP"}
    let no_exp = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhdXRoMHx1c2VyXzAxTk9FWFAifQ.c2ln";

    let claims = parse_session_claims(valid).expect("valid claims");
    assert_eq!(claims.subject, "github|user_01TESTFIXTURE");
    assert_eq!(
        claims.expires_at,
        Some(Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap())
    );
    assert!(!claims.is_expired(Utc::now()));

    let claims = parse_session_claims(expired).expect("expired claims still parse");
    assert!(
        claims.is_expired(Utc::now()),
        "an expired token is never sent"
    );

    assert_eq!(
        parse_session_claims(no_provider).unwrap().subject,
        "user_01PLAIN"
    );
    assert!(parse_session_claims(no_sub).is_none());

    let claims = parse_session_claims(no_exp).expect("no exp is fine");
    assert_eq!(claims.subject, "auth0|user_01NOEXP");
    assert!(
        !claims.is_expired(Utc::now()),
        "the server judges a token without exp"
    );

    for junk in ["", "not-a-jwt", "a.b!c.d", "eyJ.e30.x"] {
        assert!(parse_session_claims(junk).is_none(), "{junk}");
    }
}

#[test]
fn cursor_hover_reasons_name_the_fix_or_none_for_a_busy_database() {
    use ailimits::ui::renderer::hover_reason;
    use chrono::Duration;
    use std::collections::HashMap;

    let stale = |id: ProviderId| ProviderData {
        id,
        status: ProviderStatus::Ok,
        metrics: vec![Metric {
            label: "Pro".to_string(),
            used: 40,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: Some(Utc::now() + Duration::days(10)),
            window: MetricWindow::Session,
        }],
        updated_at: Utc::now() - Duration::hours(3),
        received_at: None,
    };

    let mut errors = HashMap::new();
    errors.insert(ProviderId::Cursor, "token expired".to_string());
    assert_eq!(
        hover_reason(&stale(ProviderId::Cursor), &errors).as_deref(),
        Some("token expired — open Cursor")
    );

    // A transient busy database retries by itself: no action appended.
    errors.insert(
        ProviderId::Cursor,
        "state database busy, retrying".to_string(),
    );
    assert_eq!(
        hover_reason(&stale(ProviderId::Cursor), &errors).as_deref(),
        Some("state database busy, retrying")
    );
    // Claude's rate-limit message keeps its behaviour.
    errors.insert(ProviderId::Claude, "rate-limited, retrying".to_string());
    assert_eq!(
        hover_reason(&stale(ProviderId::Claude), &errors).as_deref(),
        Some("rate-limited, retrying")
    );
}

#[test]
fn cursor_config_id_round_trips() {
    assert_eq!(
        ProviderId::from_config_id("cursor"),
        Some(ProviderId::Cursor)
    );
    assert_eq!(ProviderId::Cursor.display_name(), "Cursor");
    let json = serde_json::to_string(&ProviderId::Cursor).unwrap();
    assert_eq!(json, "\"cursor\"");
    let back: ProviderId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ProviderId::Cursor);
}
