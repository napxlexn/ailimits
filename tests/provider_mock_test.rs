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
    use ailimits::providers::antigravity::parse_antigravity_keyring_token;

    let body = r#"{"token":{"access_token":"tok-live","token_type":"Bearer","refresh_token":"refresh","expiry":"2099-07-09T16:05:44.2342031+02:00"},"auth_method":"consumer"}"#;

    assert_eq!(
        parse_antigravity_keyring_token(body).expect("should parse"),
        Some("tok-live".to_string())
    );
}

#[test]
fn antigravity_keyring_token_skips_expired_token() {
    use ailimits::providers::antigravity::parse_antigravity_keyring_token;

    let body = r#"{"token":{"access_token":"tok-expired","token_type":"Bearer","refresh_token":"refresh","expiry":"2000-07-09T16:05:44.2342031+02:00"},"auth_method":"consumer"}"#;

    assert_eq!(
        parse_antigravity_keyring_token(body).expect("should parse"),
        None
    );
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

    // Unknown shape → empty (the provider falls back to the bucket view).
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
    // A statusline snapshot / disk-cache entry (received_at None) is staled by
    // its own wall-clock age: a fresh snapshot is live, an old one is stale.
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
