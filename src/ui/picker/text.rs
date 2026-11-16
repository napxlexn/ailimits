// ui/picker/text.rs — measuring and wrapping the picker's text in the
// app's font: widths (cached), pen advances, the room a model name has, and
// greedy wrapping with long words (paths) broken after a separator.

use super::*;

impl Picker {
    // ─── Measuring ───────────────────────────────────────────────

    pub(super) fn w(&self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (text.to_string(), size.to_bits());
        if let Some(w) = self.widths.borrow().get(&key) {
            return *w;
        }
        let w = (self.measure)(text, size);
        self.widths.borrow_mut().insert(key, w);
        w
    }

    /// The pen advance after `text`: where the next character would start
    /// (a trailing space counts, unlike the ink width).
    pub(super) fn adv(&self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        (self.w(&format!("{text}|"), size) - self.w("|", size)).max(0.0)
    }

    /// The faux bold the renderer draws is about a pixel wider.
    pub(super) fn adv_styled(&self, text: &str, size: f32, bold: bool) -> f32 {
        self.adv(text, size) + if bold { 1.0 } else { 0.0 }
    }

    /// Whether a model name fits every place the widget draws one.
    pub(super) fn name_fits(&self, name: &str) -> bool {
        crate::rows::name_fits(name, |t, s| self.w(t, s))
    }

    /// How many more characters of ordinary text still fit after `text`.
    pub(super) fn chars_left(&self, text: &str) -> usize {
        let sample: Vec<char> = ROOM_SAMPLE.chars().collect();
        let mut k = 0;
        while k < sample.len() {
            let more: String = sample[..=k].iter().collect();
            if !self.name_fits(&format!("{text}{more}")) {
                break;
            }
            k += 1;
        }
        let fit: String = sample[..k].iter().collect();
        fit.trim_end().chars().count()
    }

    /// Wrap styled text to `width`: words break at spaces, a word too long
    /// for a line (a path) after a separator or, failing that, anywhere.
    pub(super) fn wrap(&self, spans: &[Span], width: f32) -> Vec<Vec<Run>> {
        struct Tok {
            text: String,
            style: usize,
            space: bool,
        }
        let mut toks: Vec<Tok> = Vec::new();
        let mut space = false;
        for (si, sp) in spans.iter().enumerate() {
            for (i, part) in sp.text.split(' ').enumerate() {
                if i > 0 {
                    space = true;
                }
                if part.is_empty() {
                    continue;
                }
                let first_space = space && !toks.is_empty();
                space = false;
                let s = &spans[si];
                for (k, piece) in self
                    .break_word(part, s.size, s.bold, width)
                    .into_iter()
                    .enumerate()
                {
                    toks.push(Tok {
                        text: piece,
                        style: si,
                        space: k == 0 && first_space,
                    });
                }
            }
        }
        // Greedy fill, then merge neighbours of one style into runs.
        let mut lines: Vec<Vec<(f32, &Tok)>> = vec![Vec::new()];
        let mut x = 0.0;
        for t in &toks {
            let s = &spans[t.style];
            let gap = if t.space && x > 0.0 {
                self.adv(" ", s.size)
            } else {
                0.0
            };
            let w = self.adv_styled(&t.text, s.size, s.bold);
            if x > 0.0 && x + gap + w > width {
                lines.push(Vec::new());
                x = 0.0;
                lines.last_mut().unwrap().push((0.0, t));
                x += w;
            } else {
                lines.last_mut().unwrap().push((x + gap, t));
                x += gap + w;
            }
        }
        lines
            .into_iter()
            .filter(|l| !l.is_empty())
            .map(|l| {
                // Neighbours from one span share a run, drawn in one go.
                let mut runs: Vec<Run> = Vec::new();
                for (tx, t) in l {
                    let s = &spans[t.style];
                    match runs.last_mut() {
                        Some(r) if r.style == t.style => {
                            if t.space {
                                r.text.push(' ');
                            }
                            r.text.push_str(&t.text);
                        }
                        _ => runs.push(Run {
                            x: tx,
                            text: t.text.clone(),
                            size: s.size,
                            ink: s.ink,
                            bold: s.bold,
                            style: t.style,
                        }),
                    }
                }
                runs
            })
            .collect()
    }

    /// Split a word that is wider than `width` into pieces that fit.
    pub(super) fn break_word(&self, word: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
        if self.adv_styled(word, size, bold) <= width {
            return vec![word.to_string()];
        }
        let mut out = Vec::new();
        let mut rest = word;
        while !rest.is_empty() {
            if self.adv_styled(rest, size, bold) <= width {
                out.push(rest.to_string());
                break;
            }
            let mut end = 0;
            let mut soft = 0;
            for (i, c) in rest.char_indices() {
                let next = i + c.len_utf8();
                if self.adv_styled(&rest[..next], size, bold) > width {
                    break;
                }
                end = next;
                if matches!(c, '\\' | '/' | '.' | '_' | '-' | ',') {
                    soft = next;
                }
            }
            let cut = if soft > 0 {
                soft
            } else {
                end.max(first_char_len(rest))
            };
            out.push(rest[..cut].to_string());
            rest = &rest[cut..];
        }
        out
    }
}

pub(super) fn first_char_len(s: &str) -> usize {
    s.chars().next().map_or(0, char::len_utf8)
}
