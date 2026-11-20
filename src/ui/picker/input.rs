// ui/picker/input.rs — what the pointer, the keyboard and typing do: hits
// become actions, questions are asked and answered, the focus moves, a
// name is edited.

use super::*;

impl Picker {
    // ─── Pointer ─────────────────────────────────────────────────

    pub(super) fn pointer_down(&mut self, x: f32, y: f32) -> Vec<PickerAction> {
        self.pointer = Some((x, y));
        self.focus_visible = false;
        self.dirty = true;
        let f = self.frame();
        match self.hit(&f, x, y) {
            // A click beside the box closes it; it answers nothing.
            Hit::Outside => self.dismiss(),
            Hit::Bar { ask } => {
                self.pressed = None;
                self.press_bar(&f, ask, y);
                Vec::new()
            }
            Hit::Nothing => {
                self.pressed = None;
                // A press on nothing ends an edit, as a click beside a field does.
                if matches!(self.focus, Some(Target::Field { .. })) {
                    self.set_focus(None);
                }
                Vec::new()
            }
            Hit::Target(t @ Target::Field { .. }) => {
                self.pressed = None;
                self.set_focus(Some(t.clone()));
                self.place_caret(&f, &t, x);
                Vec::new()
            }
            Hit::Target(t) => {
                self.pressed = Some(t);
                Vec::new()
            }
        }
    }

    pub(super) fn pointer_up(&mut self, x: f32, y: f32) -> Vec<PickerAction> {
        self.pointer = Some((x, y));
        if self.drag.take().is_some() {
            self.dirty = true;
            return Vec::new();
        }
        let Some(t) = self.pressed.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let f = self.frame();
        if self.hit(&f, x, y) != Hit::Target(t.clone()) {
            return Vec::new();
        }
        if !matches!(t, Target::Close | Target::NoticeOk) {
            self.set_focus(Some(t.clone()));
        }
        self.activate(&t)
    }

    /// A press on a scroll bar: on its thumb it starts a drag, elsewhere on
    /// the strip it pages toward the press.
    pub(super) fn press_bar(&mut self, f: &Frame, ask: bool, y: f32) {
        let (bar, scroll, page) = match (ask, &f.ask) {
            (true, Some(a)) => (a.bar, a.scroll, a.viewport.h - lh(SIZE_TEXT)),
            _ => (f.bar, f.scroll, f.viewport.h - ROW_H),
        };
        let Some(b) = bar else {
            return;
        };
        if y >= b.thumb.y && y < bottom(&b.thumb) {
            self.drag = Some(Drag {
                ask,
                from_y: y,
                from_scroll: scroll,
            });
        } else {
            let page = page.max(lh(SIZE_TEXT));
            self.scroll_by(if y < b.thumb.y { -page } else { page });
        }
    }

    /// A thumb dragged to `y`.
    pub(super) fn drag_to(&mut self, d: Drag, y: f32) {
        let f = self.frame();
        if d.ask {
            if let (Some(a), Some(ask)) = (f.ask, self.ask.as_mut()) {
                if let Some(b) = a.bar {
                    ask.scroll = b.scrolled(&a.viewport, a.content_h, d.from_scroll, y - d.from_y);
                }
            }
        } else if let Some(b) = f.bar {
            self.scroll = b.scrolled(&f.viewport, f.content_h, d.from_scroll, y - d.from_y);
        }
        self.dirty = true;
    }

    /// Scroll what the wheel and the page keys move: an open question's
    /// text, else the list. Positive goes down.
    pub(super) fn scroll_by(&mut self, dy: f32) {
        match self.ask.as_mut() {
            Some(a) => a.scroll += dy,
            None => self.scroll += dy,
        }
        self.settle();
        self.dirty = true;
    }

    /// Put the caret at the character boundary nearest to `x`.
    pub(super) fn place_caret(&mut self, f: &Frame, field: &Target, x: f32) {
        let Some(r) = f.els.iter().find_map(|(_, el)| match el {
            El::Field { target, rect, .. } if target == field => Some(*rect),
            _ => None,
        }) else {
            return;
        };
        let Some(text) = self.edit.as_ref().map(|e| e.text.clone()) else {
            return;
        };
        let local = x - (r.x + 8.0);
        let mut best = (f32::MAX, text.len());
        let bounds = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()));
        for i in bounds {
            let d = (self.adv(&text[..i], SIZE_TEXT) - local).abs();
            if d < best.0 {
                best = (d, i);
            }
        }
        if let Some(e) = self.edit.as_mut() {
            e.caret = best.1;
        }
    }

    // ─── Acting ──────────────────────────────────────────────────

    pub(super) fn activate(&mut self, t: &Target) -> Vec<PickerAction> {
        self.dirty = true;
        match t {
            Target::Close => self.close(),
            Target::Row { row, surface } => {
                let Some(choice) = self.input.rows.iter().find(|r| r.id == row).cloned() else {
                    return Vec::new();
                };
                let on = self.row_on(*surface, &choice);
                let act = PickerAction::SetRow {
                    surface: *surface,
                    row: row.clone(),
                    on: !on,
                };
                if !on && !self.allowed(choice.provider) {
                    let kind = ReadKind::SignIn(canonical(choice.provider).to_string());
                    self.open_question(kind, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Taskbar(provider) => {
                if !self.taskbar_enabled(provider) {
                    return Vec::new();
                }
                let on = self.taskbar_on(provider);
                let act = PickerAction::SetTaskbar {
                    provider: provider.clone(),
                    on: !on,
                };
                if !on && !self.allowed(provider) {
                    let kind = ReadKind::SignIn(canonical(provider).to_string());
                    self.open_question(kind, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Part { part, surface } => {
                if !self.part_enabled(*surface, *part) {
                    return Vec::new();
                }
                if self.part_on(*surface, *part) {
                    let mut out = vec![PickerAction::SetPart {
                        surface: *surface,
                        part: *part,
                        on: false,
                    }];
                    // A version without its model means nothing.
                    if *part == ModelPart::Model
                        && self
                            .input
                            .model
                            .parts(*surface)
                            .contains(&ModelPart::Version)
                    {
                        out.push(PickerAction::SetPart {
                            surface: *surface,
                            part: ModelPart::Version,
                            on: false,
                        });
                    }
                    return out;
                }
                let act = PickerAction::SetPart {
                    surface: *surface,
                    part: *part,
                    on: true,
                };
                if !self.input.reads.tool_settings {
                    self.open_question(ReadKind::ToolSettings, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Source(source) => {
                if !self.any_part_on() || self.source() == *source {
                    return Vec::new();
                }
                let act = PickerAction::SetSource(*source);
                if *source == ModelSource::LastUsed && !self.input.reads.last_session {
                    self.open_question(ReadKind::LastSession, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Field { .. } => {
                self.set_focus(Some(t.clone()));
                Vec::new()
            }
            Target::Pick(id) => {
                if let Some(a) = self.ask.as_mut() {
                    let on = a.picked.contains(id);
                    let picked: Vec<String> = a
                        .tools
                        .iter()
                        .filter(|p| if *p == id { !on } else { a.picked.contains(p) })
                        .cloned()
                        .collect();
                    a.picked = picked;
                }
                Vec::new()
            }
            Target::Allow => {
                if !self.ask.as_ref().is_some_and(|a| self.can_allow(a)) {
                    return Vec::new();
                }
                let Some(ask) = self.ask.take() else {
                    return Vec::new();
                };
                self.set_focus(ask.from.clone());
                let (mut out, alone) = match ask.question {
                    Question::Found => {
                        self.found_answered = true;
                        let mut out: Vec<PickerAction> = ask
                            .picked
                            .iter()
                            .map(|p| PickerAction::Allow(ReadKind::SignIn(p.clone())))
                            .collect();
                        for p in &ask.picked {
                            out.extend(Self::own_rows_on(p));
                        }
                        (out, false)
                    }
                    Question::Read(kind) => {
                        // The Providers menu's question: its Allow goes alone
                        // (the app then shows the provider's rows).
                        let alone = ask.then.is_empty();
                        let mut out = vec![PickerAction::Allow(kind)];
                        out.extend(ask.then);
                        (out, alone)
                    }
                };
                out.extend(self.answered(alone));
                out
            }
            Target::AllowAll => {
                let Some(Question::Read(ReadKind::SignIn(p))) =
                    self.ask.as_ref().map(|a| a.question.clone())
                else {
                    return Vec::new();
                };
                let others = self.others_found(&p);
                let Some(ask) = self.ask.take() else {
                    return Vec::new();
                };
                self.set_focus(ask.from.clone());
                let mut out: Vec<PickerAction> = std::iter::once(&p)
                    .chain(&others)
                    .map(|id| PickerAction::Allow(ReadKind::SignIn(id.clone())))
                    .collect();
                // The tick that asked, or (asked from the menu) its own rows.
                if ask.then.is_empty() {
                    out.extend(Self::own_rows_on(&p));
                } else {
                    out.extend(ask.then);
                }
                for o in &others {
                    out.extend(Self::own_rows_on(o));
                }
                out.extend(self.answered(false));
                out
            }
            Target::Names => {
                self.names_open = !self.names_open;
                Vec::new()
            }
            Target::NamesOf(p) => {
                if let Some(i) = self.open_names.iter().position(|o| o == p) {
                    self.open_names.remove(i);
                } else {
                    self.open_names.push(p.clone());
                }
                Vec::new()
            }
            Target::Cancel => {
                let Some(ask) = self.ask.take() else {
                    return Vec::new();
                };
                if ask.question == Question::Found {
                    self.found_answered = true;
                }
                self.set_focus(ask.from);
                self.answered(false)
            }
            Target::NoticeOk => {
                self.notice_done = true;
                vec![PickerAction::NoticeDone, PickerAction::Close]
            }
            Target::NoticeReview => {
                self.notice_done = true;
                self.reviewed = true;
                self.scroll = 0.0;
                self.focus = None;
                vec![PickerAction::NoticeDone]
            }
        }
    }

    /// A question the user's own click or the Providers menu opened: Allow
    /// has the focus, as the mock's question does.
    pub(super) fn open_question(&mut self, kind: ReadKind, then: Vec<PickerAction>, from: Target) {
        self.ask = Some(Ask {
            question: Question::Read(kind),
            then,
            from: Some(from),
            tools: Vec::new(),
            picked: Vec::new(),
            scroll: 0.0,
        });
        self.set_focus(Some(Target::Allow));
        self.dirty = true;
    }

    /// Any question answered in the welcome answers the welcome: it is not
    /// owed again. Sent at once, or with the window's close when the answer
    /// goes alone (`alone`: the Providers menu's question).
    pub(super) fn answered(&mut self, alone: bool) -> Vec<PickerAction> {
        if self.opened_as != PickerMode::Welcome || self.notice_done {
            return Vec::new();
        }
        if alone {
            self.welcome_answered = true;
            return Vec::new();
        }
        self.notice_done = true;
        vec![PickerAction::NoticeDone]
    }

    /// Esc, the cross, Alt+F4: closed on purpose, so the welcome and the
    /// notice count as answered.
    pub(super) fn close(&mut self) -> Vec<PickerAction> {
        let mut out = Vec::new();
        if matches!(
            self.opened_as,
            PickerMode::Welcome | PickerMode::UpdateNotice
        ) && !self.notice_done
        {
            self.notice_done = true;
            out.push(PickerAction::NoticeDone);
        }
        self.ask = None;
        out.push(PickerAction::Close);
        out
    }

    /// A click outside or the focus gone to another window: the window
    /// closes, but the welcome and the notice are owed still, unless a
    /// question in the welcome was answered.
    pub(super) fn dismiss(&mut self) -> Vec<PickerAction> {
        let mut out = Vec::new();
        if self.welcome_answered && !self.notice_done {
            self.notice_done = true;
            out.push(PickerAction::NoticeDone);
        }
        self.ask = None;
        out.push(PickerAction::Close);
        out
    }

    /// The focus went to another window. Typing a name, that may be a shell
    /// panel (Win+V, Win+.) that types into the field: the window stays,
    /// and the edit goes on when the focus comes back.
    pub(super) fn focus_lost(&mut self) -> Vec<PickerAction> {
        if self.editing() {
            return Vec::new();
        }
        self.dismiss()
    }

    // ─── Focus ───────────────────────────────────────────────────

    /// Move the focus. Leaving a name field ends its edit; entering one
    /// starts it, caret at the end.
    pub(super) fn set_focus(&mut self, t: Option<Target>) {
        if self.focus == t {
            return;
        }
        self.focus = t;
        self.dirty = true;
        match &self.focus {
            Some(Target::Field { key, .. }) => {
                if self.edit.as_ref().is_none_or(|e| &e.key != key) {
                    let current = self
                        .input
                        .models
                        .iter()
                        .find(|m| &m.key == key)
                        .and_then(|m| m.renamed.clone())
                        .unwrap_or_default();
                    let sent = Some(current.trim().to_string()).filter(|s| !s.is_empty());
                    self.edit = Some(Edit {
                        key: key.clone(),
                        caret: current.len(),
                        original: sent.clone(),
                        text: current,
                        full: false,
                        sent,
                    });
                }
            }
            _ => self.edit = None,
        }
    }

    /// The focusable targets in tab order with their keyboard positions and
    /// rectangles (window coordinates).
    pub(super) fn focusables(&self, f: &Frame) -> Vec<(Target, Nav, Rect, Layer)> {
        let mut out: Vec<(Target, Nav, Rect, Layer)> = Vec::new();
        for (layer, el) in &f.els {
            if let Some((t, r, nav)) = el.active() {
                if self.ask.is_some()
                    && !matches!(
                        t,
                        Target::Allow | Target::AllowAll | Target::Cancel | Target::Pick(_)
                    )
                {
                    continue;
                }
                out.push((t.clone(), nav, r, *layer));
            }
        }
        out
    }

    /// Where the keyboard focus goes first: the default button (an open
    /// question's Allow, else its Cancel; the notice's OK), else the first.
    pub(super) fn first_focus(&self, items: &[(Target, Nav, Rect, Layer)]) -> usize {
        let wanted = match self.view() {
            _ if self.ask.is_some() => [Target::Allow, Target::Cancel],
            View::Notice => [Target::NoticeOk, Target::NoticeOk],
            View::Rows { .. } => return 0,
        };
        wanted
            .iter()
            .find_map(|w| items.iter().position(|(t, ..)| t == w))
            .unwrap_or(0)
    }

    /// Forget a focus whose target is gone or disabled.
    pub(super) fn drop_lost_focus(&mut self) {
        if let Some(t) = &self.focus {
            let f = self.frame();
            if !self.focusables(&f).iter().any(|(ft, ..)| ft == t) {
                self.set_focus(None);
            }
        }
    }

    /// Scroll the focused target into view: in the list, or in an open
    /// question's text.
    pub(super) fn reveal_focus(&mut self) {
        let Some(t) = self.focus.clone() else {
            return;
        };
        let f = self.frame();
        let Some((_, _, r, layer)) = self.focusables(&f).into_iter().find(|(ft, ..)| *ft == t)
        else {
            return;
        };
        match layer {
            Layer::Body => {
                // A field is revealed with its label above and its hint below.
                let (top, end) = if matches!(t, Target::Field { .. }) {
                    (r.y - lh(SIZE_TEXT) - 6.0, bottom(&r) + 4.0 + lh(SIZE_NOTE))
                } else {
                    (r.y, bottom(&r))
                };
                // Scrolled to the top edge, a row sits under the pinned heads.
                let list_top = f.viewport.y + COLS_H;
                if top < list_top {
                    self.scroll = f.scroll - (list_top - top);
                } else if end > bottom(&f.viewport) {
                    self.scroll = f.scroll + (end - bottom(&f.viewport)) + 4.0;
                }
            }
            Layer::AskBody => {
                if let (Some(a), Some(ask)) = (f.ask, self.ask.as_mut()) {
                    if r.y < a.viewport.y {
                        ask.scroll = a.scroll - (a.viewport.y - r.y) - 4.0;
                    } else if bottom(&r) > bottom(&a.viewport) {
                        ask.scroll = a.scroll + (bottom(&r) - bottom(&a.viewport)) + 4.0;
                    }
                }
            }
            _ => {}
        }
        self.settle();
    }

    /// Hold the scrolls to their content after any change.
    pub(super) fn settle(&mut self) {
        let f = self.frame();
        self.scroll = self.scroll.clamp(0.0, f.max_scroll());
        if let (Some(a), Some(ask)) = (f.ask, self.ask.as_mut()) {
            ask.scroll = ask.scroll.clamp(0.0, a.max_scroll());
        }
    }

    /// How far a page key scrolls: what shows, less a line to keep.
    pub(super) fn page_height(&self) -> f32 {
        let f = self.frame();
        let h = match &f.ask {
            Some(a) => a.viewport.h - lh(SIZE_TEXT),
            None => f.viewport.h - ROW_H,
        };
        h.max(lh(SIZE_TEXT))
    }

    // ─── Keyboard ────────────────────────────────────────────────

    pub(super) fn key(&mut self, k: PickerKey) -> Vec<PickerAction> {
        self.dirty = true;
        if k == PickerKey::Escape {
            if self.ask.is_some() {
                return self.activate(&Target::Cancel);
            }
            // In a name field Esc first takes the edit back.
            if let Some(out) = self.revert_edit() {
                return out;
            }
            return self.close();
        }
        // The page keys scroll what shows: the question's text, else the
        // list. The focus stays.
        if matches!(k, PickerKey::PageUp | PickerKey::PageDown) {
            let page = self.page_height();
            self.scroll_by(if k == PickerKey::PageUp { -page } else { page });
            return Vec::new();
        }
        // In a name field the editing keys edit.
        if self.editing() {
            match k {
                PickerKey::Backspace => return self.erase(false),
                PickerKey::Delete => return self.erase(true),
                PickerKey::Left | PickerKey::Right | PickerKey::Home | PickerKey::End => {
                    if let Some(e) = self.edit.as_mut() {
                        e.caret = match k {
                            PickerKey::Left => prev_boundary(&e.text, e.caret),
                            PickerKey::Right => next_boundary(&e.text, e.caret),
                            PickerKey::Home => 0,
                            _ => e.text.len(),
                        };
                    }
                    return Vec::new();
                }
                PickerKey::Space => return self.type_text(" "),
                PickerKey::Enter => return Vec::new(),
                _ => {}
            }
        }
        let f = self.frame();
        let items = self.focusables(&f);
        if items.is_empty() {
            return Vec::new();
        }
        let at = self
            .focus
            .as_ref()
            .and_then(|t| items.iter().position(|(ft, ..)| ft == t));
        let next = match k {
            PickerKey::Tab => {
                Some(at.map_or_else(|| self.first_focus(&items), |i| (i + 1) % items.len()))
            }
            PickerKey::BackTab => {
                Some(at.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len()))
            }
            PickerKey::Home => Some(0),
            PickerKey::End => Some(items.len() - 1),
            PickerKey::Up | PickerKey::Down | PickerKey::Left | PickerKey::Right => match at {
                Some(i) => {
                    let j = grid_move(&items, i, k);
                    // Nothing further that way in a question: its text
                    // scrolls instead, a line at a time.
                    if j == i && self.ask.is_some() && matches!(k, PickerKey::Up | PickerKey::Down)
                    {
                        let line = lh(SIZE_TEXT) * 3.0;
                        self.scroll_by(if k == PickerKey::Up { -line } else { line });
                        return Vec::new();
                    }
                    Some(j)
                }
                None => Some(self.first_focus(&items)),
            },
            PickerKey::Space | PickerKey::Enter => {
                self.focus_visible = true;
                return match &self.focus {
                    Some(t) if items.iter().any(|(ft, ..)| ft == t) => {
                        let t = t.clone();
                        self.activate(&t)
                    }
                    _ => Vec::new(),
                };
            }
            _ => None,
        };
        if let Some(i) = next {
            self.focus_visible = true;
            self.set_focus(Some(items[i].0.clone()));
            self.reveal_focus();
        }
        Vec::new()
    }

    // ─── Editing a name ──────────────────────────────────────────

    pub(super) fn type_text(&mut self, s: &str) -> Vec<PickerAction> {
        if !matches!(self.focus, Some(Target::Field { .. })) {
            return Vec::new();
        }
        let Some(mut e) = self.edit.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let mut full = false;
        for c in s.chars() {
            let c = if c.is_whitespace() { ' ' } else { c };
            if c.is_control() {
                continue;
            }
            let (before, after) = e.text.split_at(e.caret);
            if c == ' ' {
                // No leading, double or useless trailing space: a space must
                // leave room for something after it.
                if before.is_empty() || before.ends_with(' ') || after.starts_with(' ') {
                    continue;
                }
                if !self.name_fits(&format!("{before} e{after}")) {
                    full = true;
                    break;
                }
            }
            let candidate = format!("{before}{c}{after}");
            if !self.name_fits(&candidate) {
                full = true;
                break;
            }
            e.caret += c.len_utf8();
            e.text = candidate;
        }
        e.full = full;
        self.edit = Some(e);
        self.renamed()
    }

    /// A paste: one line, single spaces, cut to what fits.
    pub(super) fn paste(&mut self, s: &str) -> Vec<PickerAction> {
        if !matches!(self.focus, Some(Target::Field { .. })) {
            return Vec::new();
        }
        let Some(mut e) = self.edit.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let clean = s
            .chars()
            .filter(|c| !c.is_control() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let (before, after) = e.text.split_at(e.caret);
        let (before, after) = (before.to_string(), after.to_string());
        // `clean` has no space at either end.
        let inserted = if before.is_empty() && after.is_empty() {
            crate::rows::fit_name(&clean, |t, sz| self.w(t, sz))
        } else {
            let mut end = 0;
            for (i, c) in clean.char_indices() {
                let next = i + c.len_utf8();
                if !self.name_fits(&format!("{before}{}{after}", &clean[..next])) {
                    break;
                }
                end = next;
            }
            let piece = &clean[..end];
            if after.is_empty() || after.starts_with(' ') {
                piece.trim_end().to_string()
            } else {
                piece.to_string()
            }
        };
        e.full = inserted.chars().count() < clean.chars().count();
        e.text = format!("{before}{inserted}{after}");
        e.caret = before.len() + inserted.len();
        self.edit = Some(e);
        self.renamed()
    }

    pub(super) fn erase(&mut self, forward: bool) -> Vec<PickerAction> {
        let Some(e) = self.edit.as_mut() else {
            return Vec::new();
        };
        let (from, to) = if forward {
            (e.caret, next_boundary(&e.text, e.caret))
        } else {
            (prev_boundary(&e.text, e.caret), e.caret)
        };
        if from == to {
            return Vec::new();
        }
        e.text.replace_range(from..to, "");
        e.caret = from;
        e.full = false;
        self.renamed()
    }

    /// Esc in a name field: back to the name it had when the edit began.
    /// None when there is nothing to take back; Esc then closes.
    pub(super) fn revert_edit(&mut self) -> Option<Vec<PickerAction>> {
        if !self.editing() {
            return None;
        }
        let e = self.edit.as_mut()?;
        let original = e.original.clone().unwrap_or_default();
        if e.text == original {
            return None;
        }
        e.caret = original.len();
        e.text = original;
        e.full = false;
        Some(self.renamed())
    }

    /// The rename an edit makes, when the trimmed name changed.
    pub(super) fn renamed(&mut self) -> Vec<PickerAction> {
        let Some(e) = self.edit.as_mut() else {
            return Vec::new();
        };
        let name = Some(e.text.trim().to_string()).filter(|s| !s.is_empty());
        if name == e.sent {
            return Vec::new();
        }
        e.sent = name.clone();
        vec![PickerAction::Rename {
            key: e.key.clone(),
            name,
        }]
    }
}

/// Arrow keys over the grid of rows and columns: up and down to the nearest
/// column of the next row that has one, left and right within a row.
pub(super) fn grid_move(items: &[(Target, Nav, Rect, Layer)], at: usize, k: PickerKey) -> usize {
    let (row, col) = items[at].1;
    let pick = |cands: Vec<usize>| -> Option<usize> {
        cands
            .into_iter()
            .min_by_key(|&i| (items[i].1 .1 as i64 - col as i64).unsigned_abs())
    };
    let found = match k {
        PickerKey::Up | PickerKey::Down => {
            let target_row = items
                .iter()
                .map(|(_, n, ..)| n.0)
                .filter(|&r| if k == PickerKey::Up { r < row } else { r > row })
                .reduce(|a, b| {
                    if k == PickerKey::Up {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                });
            target_row
                .and_then(|tr| pick((0..items.len()).filter(|&i| items[i].1 .0 == tr).collect()))
        }
        _ => {
            let same: Vec<usize> = (0..items.len()).filter(|&i| items[i].1 .0 == row).collect();
            let want = |c: u32| {
                if k == PickerKey::Left {
                    c < col
                } else {
                    c > col
                }
            };
            same.into_iter()
                .filter(|&i| want(items[i].1 .1))
                .min_by_key(|&i| (items[i].1 .1 as i64 - col as i64).unsigned_abs())
        }
    };
    found.unwrap_or(at)
}

pub(super) fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].char_indices().last().map_or(0, |(j, _)| j)
}

pub(super) fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}
