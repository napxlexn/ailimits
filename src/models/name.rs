// models/name.rs — the generic name rule for a model id, with no table of
// names, so a new model needs no app update. A port of `modelWords`,
// `joinWords` and `nameParts` in design/workshop/countdown/rows.js; the
// fixture tests/fixtures/model-names.tsv (156 public ids) holds it to the
// workshop's output word for word.
//
// The rule: drop date stamps and "latest", drop the provider's own name
// unless a version follows it, join version digits ("4-5" -> "4.5"),
// capitalise; an acronym takes its version with a hyphen ("GPT-5.4").

use super::NameParts;

/// The name for `id`, or for the user's `rename` when it has any text, split
/// into the model and the model with its version. `provider` is the
/// provider's display name ("Claude"), dropped from the front of an id.
pub(super) fn name_parts(id: &str, provider: &str, rename: Option<&str>) -> NameParts {
    let typed = rename
        .map(str::trim)
        .filter(|s| !s.is_empty())
        // A tool that stores a display label rather than an id (Antigravity:
        // "GPT-OSS 120B (Medium)") has its words already: taken as they are,
        // like a rename, never split at its hyphens. An id has no spaces.
        .or_else(|| Some(id.trim()).filter(|s| s.contains(char::is_whitespace)));
    let words: Vec<String> = match typed {
        // A rename keeps its words as typed: no capitals, no dropped parts.
        Some(own) => own
            .split(|c: char| c.is_whitespace() || c == '\u{feff}')
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect(),
        None => model_words(id, provider),
    };
    split_parts(&words)
}

/// The words of a model id: "claude-opus-4-5-20251101" -> ["Opus", "4.5"].
fn model_words(raw: &str, provider: &str) -> Vec<String> {
    let t: Vec<&str> = raw
        .split(['-', '_', ' '])
        .filter(|s| !s.is_empty())
        .collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let x = t[i];
        // 20251101
        if is_date8(x) {
            i += 1;
            continue;
        }
        // 2024-05-13
        if is_year(x) && i + 2 < t.len() && is_two_digits(t[i + 1]) && is_two_digits(t[i + 2]) {
            i += 3;
            continue;
        }
        // 04-2026
        if is_two_digits(x) && i + 1 < t.len() && is_year(t[i + 1]) {
            i += 2;
            continue;
        }
        // 4.20-0309, ...-agent-0309
        if is_month_day(x)
            && out
                .last()
                .is_some_and(|last| starts_with_digit(last) || i == t.len() - 1)
        {
            i += 1;
            continue;
        }
        out.push(x);
        i += 1;
    }

    let mut w: Vec<&str> = out
        .iter()
        .copied()
        .filter(|x| !x.eq_ignore_ascii_case("latest"))
        .collect();
    if w.is_empty() {
        w = out;
    }
    if w.len() > 1 && w[0].to_lowercase() == provider.to_lowercase() && !starts_with_digit(w[1]) {
        w.remove(0);
    }

    // Version digits join: "4", "5" -> "4.5".
    let mut res: Vec<String> = Vec::new();
    for x in w {
        let joins = (1..=2).contains(&x.len())
            && all_digits(x)
            && res.last().is_some_and(|last| is_version(last));
        match res.last_mut() {
            Some(last) if joins => {
                last.push('.');
                last.push_str(x);
            }
            _ => res.push(x.to_string()),
        }
    }
    res.into_iter().map(|x| capitalise(&x)).collect()
}

/// Digits and letter-digit codes stay as they are ("4.5", "o3"); a word
/// without vowels is an acronym ("GPT"); any other word gets a capital.
fn capitalise(x: &str) -> String {
    if starts_with_digit(x) || is_letter_digits(x) {
        return x.to_string();
    }
    if !x.chars().any(|c| "aeiouyAEIOUY".contains(c)) {
        return x.to_uppercase();
    }
    let mut chars = x.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Words joined with spaces; an acronym of 2-4 capitals takes a version that
/// follows it with a hyphen: "GPT", "5.4" -> "GPT-5.4".
fn join_words<S: AsRef<str>>(words: &[S]) -> String {
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let w = w.as_ref();
        match out.last_mut() {
            Some(last) if is_acronym(last) && starts_with_digit(w) => {
                last.push('-');
                last.push_str(w);
            }
            _ => out.push(w.to_string()),
        }
    }
    out.join(" ")
}

/// Words before the first version word are the model, all of them the model
/// with its version: Opus | 5.5, GPT | 6.1 Sol.
fn split_parts(words: &[String]) -> NameParts {
    let with_version = join_words(words);
    let Some(k) = words
        .iter()
        .position(|x| starts_with_digit(x) || hyphen_version(x).is_some())
    else {
        return NameParts {
            model: with_version.clone(),
            with_version,
        };
    };
    let mut head: Vec<&str> = words[..k].iter().map(String::as_str).collect();
    if let Some(acronym) = hyphen_version(&words[k]) {
        head.push(acronym);
        return NameParts {
            model: join_words(&head),
            with_version,
        };
    }
    let model = join_words(&head);
    NameParts {
        model: if model.is_empty() {
            with_version.clone()
        } else {
            model
        },
        with_version,
    }
}

/// "GPT-5.4" -> Some("GPT"): one to four letters, a hyphen, then a version.
/// (rows.js: /^([A-Za-z]{1,4})-(\d.*)$/, whose `.` stops at line breaks.)
fn hyphen_version(x: &str) -> Option<&str> {
    let (head, rest) = x.split_once('-')?;
    let ok = (1..=4).contains(&head.len())
        && head.bytes().all(|b| b.is_ascii_alphabetic())
        && starts_with_digit(rest)
        && !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
    ok.then_some(head)
}

fn starts_with_digit(x: &str) -> bool {
    x.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

fn all_digits(x: &str) -> bool {
    !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit())
}

/// /^\d+(\.\d+)*$/
fn is_version(x: &str) -> bool {
    x.split('.').all(all_digits)
}

/// /^\d{2}$/
fn is_two_digits(x: &str) -> bool {
    x.len() == 2 && all_digits(x)
}

/// /^(19|20)\d{2}$/
fn is_year(x: &str) -> bool {
    x.len() == 4 && all_digits(x) && (x.starts_with("19") || x.starts_with("20"))
}

/// /^(19|20)\d{6}$/
fn is_date8(x: &str) -> bool {
    x.len() == 8 && all_digits(x) && (x.starts_with("19") || x.starts_with("20"))
}

/// /^[01]\d[0-3]\d$/
fn is_month_day(x: &str) -> bool {
    let b = x.as_bytes();
    b.len() == 4 && all_digits(x) && b[0] <= b'1' && b[2] <= b'3'
}

/// /^[a-z]\d+$/
fn is_letter_digits(x: &str) -> bool {
    let b = x.as_bytes();
    b.len() >= 2 && b[0].is_ascii_lowercase() && b[1..].iter().all(u8::is_ascii_digit)
}

/// /^[A-Z]{2,4}$/
fn is_acronym(x: &str) -> bool {
    (2..=4).contains(&x.len()) && x.bytes().all(|b| b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(id: &str, provider: &str) -> (String, String) {
        let n = name_parts(id, provider, None);
        (n.model, n.with_version)
    }

    /// Every id of the fixture reads exactly as the workshop's rule reads it;
    /// the provider column is the provider's display name in the app.
    #[test]
    fn the_rule_matches_the_workshop_on_every_fixture_id() {
        let fixture = include_str!("../../tests/fixtures/model-names.tsv");
        let mut rows = 0;
        let mut misses = Vec::new();
        for line in fixture.lines() {
            let line = line.trim_end_matches('\r');
            if line.starts_with('#') || line.is_empty() || line.starts_with("provider\t") {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 5, "bad fixture line: {line}");
            let (provider, id, rule) = (cols[0], cols[1], cols[3]);
            // The provider column is one of the app's display names.
            let pid = crate::providers::ProviderId::from_config_id(&provider.to_lowercase())
                .unwrap_or_else(|| panic!("unknown provider {provider}"));
            assert_eq!(pid.display_name(), provider);
            let got = crate::models::name_parts(id, &pid, None).with_version;
            if got != rule {
                misses.push(format!("{provider} {id}: want {rule:?}, got {got:?}"));
            }
            rows += 1;
        }
        assert_eq!(rows, 156, "the fixture has 156 ids");
        assert!(
            misses.is_empty(),
            "{} misses:\n{}",
            misses.len(),
            misses.join("\n")
        );
    }

    #[test]
    fn the_model_is_the_words_before_the_version() {
        assert_eq!(
            parts("claude-opus-5-5", "Claude"),
            ("Opus".into(), "Opus 5.5".into())
        );
        assert_eq!(
            parts("claude-opus-4-5-20251101", "Claude"),
            ("Opus".into(), "Opus 4.5".into())
        );
        assert_eq!(
            parts("gpt-6.1-sol", "Codex"),
            ("GPT".into(), "GPT-6.1 Sol".into())
        );
        assert_eq!(
            parts("gpt-5.4-mini", "Codex"),
            ("GPT".into(), "GPT-5.4 Mini".into())
        );
        assert_eq!(
            parts("grok-4.5", "Grok"),
            ("Grok".into(), "Grok 4.5".into())
        );
        assert_eq!(
            parts("gemini-3.1-pro-preview", "Antigravity"),
            ("Gemini".into(), "Gemini 3.1 Pro Preview".into())
        );
        // Antigravity stores a display label; it reads as itself.
        assert_eq!(
            parts("Claude Sonnet 4.6 (Thinking)", "Antigravity"),
            (
                "Claude Sonnet".into(),
                "Claude Sonnet 4.6 (Thinking)".into()
            )
        );
        // Hyphens inside a label stay where the tool put them.
        assert_eq!(
            parts("GPT-OSS 120B (Medium)", "Antigravity"),
            ("GPT-OSS".into(), "GPT-OSS 120B (Medium)".into())
        );
        assert_eq!(
            parts("Gemini 3.1 Pro (High)", "Antigravity"),
            ("Gemini".into(), "Gemini 3.1 Pro (High)".into())
        );
    }

    #[test]
    fn a_name_without_a_version_is_the_same_in_both_lengths() {
        assert_eq!(parts("auto", "Copilot"), ("Auto".into(), "Auto".into()));
        assert_eq!(parts("opus", "Claude"), ("Opus".into(), "Opus".into()));
        // The provider's own name stays when nothing but it is left.
        assert_eq!(
            parts("claude", "Claude"),
            ("Claude".into(), "Claude".into())
        );
        // ... and when a version follows it.
        assert_eq!(parts("grok-4", "Grok"), ("Grok".into(), "Grok 4".into()));
    }

    #[test]
    fn a_version_first_name_keeps_every_word_as_the_model() {
        // No words before the version: the model is the whole name.
        assert_eq!(parts("4o", "Codex"), ("4o".into(), "4o".into()));
        assert_eq!(
            parts("o3-mini", "Codex"),
            ("o3 Mini".into(), "o3 Mini".into())
        );
    }

    #[test]
    fn dates_and_latest_are_dropped() {
        assert_eq!(
            parts("claude-sonnet-4-5-20250929", "Claude").1,
            "Sonnet 4.5"
        );
        assert_eq!(
            parts("gpt-4o-2024-05-13", "Codex"),
            ("GPT".into(), "GPT-4o".into())
        );
        assert_eq!(
            parts("gemini-2.5-flash-preview-04-2026", "Antigravity").1,
            "Gemini 2.5 Flash Preview"
        );
        assert_eq!(parts("grok-4-fast-latest", "Grok").1, "Grok 4 Fast");
        // "latest" alone is kept rather than leaving nothing.
        assert_eq!(parts("latest", "Codex").1, "Latest");
    }

    #[test]
    fn odd_ids_never_panic() {
        for id in [
            "",
            "-",
            "--__  ",
            "20251101",
            "é",
            "ß",
            "-4",
            "a-",
            "GPT-",
            "x-1\n2",
            "日本語-1",
        ] {
            let n = name_parts(id, "Claude", None);
            assert!(n.with_version.len() >= n.model.len() || n.model == n.with_version);
        }
        assert_eq!(parts("", "Claude"), (String::new(), String::new()));
        assert_eq!(parts("ß", "Claude"), ("SS".into(), "SS".into()));
    }

    #[test]
    fn a_rename_goes_through_the_same_split() {
        let r = |s: &str| {
            let n = name_parts("claude-opus-5-5", "Claude", Some(s));
            (n.model, n.with_version)
        };
        assert_eq!(r("My Opus"), ("My Opus".into(), "My Opus".into()));
        assert_eq!(r("Big 5.5 fast"), ("Big".into(), "Big 5.5 fast".into()));
        // An acronym and its version join with a hyphen, and split back.
        assert_eq!(r("GPT 7"), ("GPT".into(), "GPT-7".into()));
        assert_eq!(r("GPT-7 Sol"), ("GPT".into(), "GPT-7 Sol".into()));
        // A rename keeps its case and its spacing collapses.
        assert_eq!(
            r("  opus   five  "),
            ("opus five".into(), "opus five".into())
        );
        // A rename that starts with its version is all model.
        assert_eq!(r("5.5 Opus"), ("5.5 Opus".into(), "5.5 Opus".into()));
        // Nothing typed restores the automatic name.
        assert_eq!(r(""), ("Opus".into(), "Opus 5.5".into()));
        assert_eq!(r("   "), ("Opus".into(), "Opus 5.5".into()));
    }
}
