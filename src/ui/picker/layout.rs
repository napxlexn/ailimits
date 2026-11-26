// ui/picker/layout.rs — the picker laid out for one state: the box and its
// header, the scrolling list (rows, model parts, the source, the names), the
// update notice, and an open question (its text, which scrolls, and its
// buttons, which stay in the box).

use super::*;

impl Picker {
    // ─── Layout ──────────────────────────────────────────────────

    pub(super) fn layout_key(&self) -> LayoutKey {
        LayoutKey {
            input: self.input_gen,
            scroll: self.scroll.to_bits(),
            max_h: self.max_box_h.to_bits(),
            ask: self
                .ask
                .as_ref()
                .map(|a| (a.question.clone(), a.picked.clone(), a.scroll.to_bits())),
            found_answered: self.found_answered,
            reviewed: self.reviewed,
            edit: self
                .edit
                .as_ref()
                .map(|e| (e.key.clone(), e.text.clone(), e.full)),
            field: self
                .focus
                .clone()
                .filter(|t| matches!(t, Target::Field { .. })),
            names: (self.names_open, self.open_names.clone()),
            sign_in: self.open_sign_in.clone(),
            tab: self.tab,
        }
    }

    /// Debug builds only, for the E2E harness: the viewport, then every
    /// control's layer, target and rect (window coordinates, logical
    /// pixels, scrolled), one per line, tab-separated.
    #[cfg(debug_assertions)]
    pub(super) fn debug_layout(&self) -> String {
        let f = self.frame();
        let v = f.viewport;
        let mut out = format!("Viewport\t-\t{}\t{}\t{}\t{}\n", v.x, v.y, v.w, v.h);
        for (layer, el) in &f.els {
            let (t, r) = match el {
                El::Tick { target, cell, .. } => (target.clone(), *cell),
                El::Radio { target, rect, .. }
                | El::Pick { target, rect, .. }
                | El::Field { target, rect, .. }
                | El::Button { target, rect, .. }
                | El::Toggle { target, rect, .. }
                | El::Tab { target, rect, .. }
                | El::Tile { target, rect, .. }
                | El::Link { target, rect, .. } => (target.clone(), *rect),
                El::Close { rect, .. } => (Target::Close, *rect),
                _ => continue,
            };
            out.push_str(&format!(
                "{layer:?}\t{t:?}\t{}\t{}\t{}\t{}\n",
                r.x, r.y, r.w, r.h
            ));
        }
        out
    }

    /// The whole window laid out for the current state; the last layout
    /// when nothing it depends on changed.
    pub(super) fn frame(&self) -> Rc<Frame> {
        let key = self.layout_key();
        if let Some((k, f)) = &*self.layout.borrow() {
            if *k == key {
                return f.clone();
            }
        }
        let f = Rc::new(self.build_frame());
        *self.layout.borrow_mut() = Some((key, f.clone()));
        f
    }

    /// Lay out the whole window.
    pub(super) fn build_frame(&self) -> Frame {
        let view = self.view();
        let tabs = matches!(view, View::Rows { .. });
        // The header, with the tabs under the title.
        let head_h = HEADER_H + if tabs { TABS_H } else { 0.0 };
        let mut flow = Flow::new();
        match view {
            View::Rows { welcome } => self.flow_tab(&mut flow, self.tab, welcome),
            View::Notice => self.flow_notice(&mut flow),
        }
        let content_h = flow.y.ceil();
        // One height for both tabs, the taller one's: the window does not
        // jump when the other tab is chosen.
        let tall_h = match view {
            View::Rows { welcome } => {
                let mut other = Flow::new();
                self.flow_tab(&mut other, self.tab.other(), welcome);
                content_h.max(other.y.ceil())
            }
            View::Notice => content_h,
        };
        let footer_h = if view == View::Notice { FOOTER_H } else { 0.0 };
        let ask = self.ask.as_ref().map(|a| self.ask_layout(a));

        let natural = head_h + tall_h + footer_h;
        let mut box_h = natural.min(self.max_box_h);
        if let Some(q) = &ask {
            box_h = box_h.max((HEADER_H + q.height() + ASK_INSET * 2.0).min(self.max_box_h));
        }
        let box_h = box_h.ceil();
        let boxr = rect(MARGIN, MARGIN, BOX_W, box_h);
        let viewport = rect(
            boxr.x + 1.0,
            boxr.y + head_h,
            BOX_W - 2.0,
            box_h - head_h - footer_h - 1.0,
        );
        let max_scroll = (content_h - viewport.h).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll).round();

        // The box and its header.
        let mut els: Vec<(Layer, El)> = vec![
            (
                Layer::Chrome,
                El::Shape(Shape::Shadow {
                    rect: boxr,
                    radius: 8.0,
                }),
            ),
            (
                Layer::Chrome,
                El::Shape(Shape::Fill {
                    rect: boxr,
                    radius: 8.0,
                    ink: Ink::Surface,
                    alpha: Level::Full,
                }),
            ),
            (
                Layer::Chrome,
                El::Shape(text_shape(
                    boxr.x + PAD_X,
                    boxr.y + 8.0 + ((30.0 - lh(SIZE_TITLE)) / 2.0).round(),
                    self.title(),
                    SIZE_TITLE,
                    Ink::Text,
                    true,
                )),
            ),
            (
                Layer::Chrome,
                El::Close {
                    rect: rect(boxr.x + BOX_W - 6.0 - 34.0, boxr.y + 8.0, 34.0, 30.0),
                    nav: (0, 9),
                },
            ),
        ];

        // The tabs: a segmented control under the title.
        if tabs {
            let track = rect(
                boxr.x + PAD_X,
                boxr.y + HEADER_H,
                BOX_W - PAD_X * 2.0,
                TABS_H - 8.0,
            );
            els.push((
                Layer::Chrome,
                El::Shape(Shape::Fill {
                    rect: track,
                    radius: 6.0,
                    ink: Ink::Hover,
                    alpha: Level::Full,
                }),
            ));
            let w = ((track.w - 6.0) / 2.0).floor();
            for (i, (tab, label)) in [(Tab::Rows, TAB_ROWS), (Tab::Shows, TAB_SHOWS)]
                .into_iter()
                .enumerate()
            {
                els.push((
                    Layer::Chrome,
                    El::Tab {
                        target: Target::Tab(tab),
                        rect: rect(
                            track.x + 3.0 + w * i as f32,
                            track.y + 3.0,
                            w,
                            track.h - 6.0,
                        ),
                        label: label.to_string(),
                        on: self.tab == tab,
                        nav: (1, i as u32),
                    },
                ));
            }
        }

        // The list, moved from content to window coordinates.
        let (dx, dy) = (boxr.x, viewport.y - scroll);
        for el in &flow.els {
            els.push((Layer::Body, el.moved(dx, dy)));
        }
        // The column heads: in place, or pinned to the top once the list has
        // scrolled past them.
        let sticky = flow.sticky.as_ref().and_then(|(cy, ch, shapes)| {
            let at = (cy + dy).max(viewport.y);
            let r = rect(viewport.x, at, viewport.w, *ch);
            (r.y < bottom(&viewport)).then(|| Sticky {
                rect: r,
                shapes: shapes.iter().map(|s| s.moved(dx, at)).collect(),
                pinned: at > cy + dy,
            })
        });

        // The notice's buttons.
        if view == View::Notice {
            let y = bottom(&boxr) - 14.0 - BUTTON_H;
            let (review, review_w) = ("Review…", self.button_w("Review…"));
            let ok_w = self.button_w("OK");
            let rx = boxr.x + BOX_W - PAD_X - review_w;
            let ox = rx - 8.0 - ok_w;
            els.push((
                Layer::Overlay,
                El::Button {
                    target: Target::NoticeOk,
                    rect: rect(ox, y, ok_w, BUTTON_H),
                    label: "OK".into(),
                    primary: true,
                    enabled: true,
                    nav: (u32::MAX - 1, 0),
                },
            ));
            els.push((
                Layer::Overlay,
                El::Button {
                    target: Target::NoticeReview,
                    rect: rect(rx, y, review_w, BUTTON_H),
                    label: review.into(),
                    primary: false,
                    enabled: true,
                    nav: (u32::MAX - 1, 1),
                },
            ));
        }

        // The scroll bar: a thin thumb, only when the list scrolls.
        let bar = Bar::of(&viewport, content_h, scroll, boxr.x + BOX_W - 6.0);
        if let Some(b) = &bar {
            els.push((Layer::Overlay, El::Shape(thumb_shape(b))));
        }

        // The read question, over the list's bottom. Its buttons stay in the
        // box; its text scrolls above them when the box is too low for it.
        let mut ask_frame = None;
        if let (Some(q), Some(a)) = (ask, self.ask.as_ref()) {
            let qh = q.height().min(box_h - HEADER_H - ASK_INSET);
            let qr = rect(
                boxr.x + ASK_INSET,
                bottom(&boxr) - ASK_INSET - qh,
                BOX_W - ASK_INSET * 2.0,
                qh,
            );
            for s in [
                Shape::Shadow {
                    rect: qr,
                    radius: 8.0,
                },
                Shape::Fill {
                    rect: qr,
                    radius: 8.0,
                    ink: Ink::Surface,
                    alpha: Level::Full,
                },
                Shape::Stroke {
                    rect: qr,
                    radius: 8.0,
                    width: 1.0,
                    ink: Ink::Border,
                    alpha: Level::Full,
                },
            ] {
                els.push((Layer::Overlay, El::Shape(s)));
            }
            let by = bottom(&qr) - ASK_BOTTOM - BUTTON_H;
            let tv = rect(qr.x + 1.0, qr.y + 1.0, qr.w - 2.0, by - 4.0 - (qr.y + 1.0));
            let mut af = AskFrame {
                rect: qr,
                viewport: tv,
                // From the viewport's top, a pixel below the box's.
                content_h: (q.text_h - 1.0).ceil(),
                scroll: 0.0,
                bar: None,
            };
            af.scroll = a.scroll.clamp(0.0, af.max_scroll()).round();
            af.bar = Bar::of(&tv, af.content_h, af.scroll, qr.x + qr.w - 6.0);
            for el in q.text {
                els.push((Layer::AskBody, el.moved(qr.x, qr.y - af.scroll)));
            }
            if let Some(b) = &af.bar {
                els.push((Layer::Top, El::Shape(thumb_shape(b))));
                // More below: a hairline over the buttons marks the cut.
                if af.scroll < af.max_scroll() {
                    els.push((
                        Layer::Top,
                        El::Shape(Shape::Fill {
                            rect: rect(qr.x + 10.0, bottom(&tv), qr.w - 20.0, 1.0),
                            radius: 0.0,
                            ink: Ink::Separator,
                            alpha: Level::Full,
                        }),
                    ));
                }
            }
            for el in q.buttons {
                els.push((Layer::Top, el.moved(qr.x, by)));
            }
            ask_frame = Some(af);
        }

        // The outline, last, over whatever reaches the edge.
        els.push((
            Layer::Top,
            El::Shape(Shape::Stroke {
                rect: boxr,
                radius: 8.0,
                width: 1.0,
                ink: Ink::Border,
                alpha: Level::Full,
            }),
        ));

        Frame {
            width: BOX_W + MARGIN * 2.0,
            height: box_h + MARGIN * 2.0,
            boxr,
            viewport,
            content_h,
            scroll,
            els,
            sticky,
            bar,
            ask: ask_frame,
        }
    }

    pub(super) fn button_w(&self, label: &str) -> f32 {
        (self.adv(label, SIZE_TEXT) + 24.0)
            .max(BUTTON_MIN_W)
            .round()
    }

    /// The Rows tab: which rows show in the widget, in the tooltip and on
    /// the taskbar, each provider's sign-in at the end of its rows.
    pub(super) fn flow_rows(&self, f: &mut Flow, welcome: bool) {
        let input = &self.input;
        if welcome {
            f.y += 4.0;
            let mut spans = vec![
                span("Select the tools you use.", SIZE_TEXT, Ink::Text, true),
                span(
                    " AI Limits reads nothing of a tool before you allow it.",
                    SIZE_TEXT,
                    Ink::Text,
                    false,
                ),
            ];
            if !input.found.is_empty() {
                spans.push(span(
                    " Detected tools are marked.",
                    SIZE_TEXT,
                    Ink::Text,
                    false,
                ));
            }
            let lines = self.wrap(&spans, BOX_W - 16.0 - 20.0);
            let h = lines.len() as f32 * lh(SIZE_TEXT) + 16.0;
            f.shape(Shape::Fill {
                rect: rect(8.0, f.y, BOX_W - 16.0, h),
                radius: 6.0,
                ink: Ink::Hover,
                alpha: Level::Full,
            });
            f.lines(&lines, 18.0, f.y + 8.0, lh(SIZE_TEXT));
            f.y += h + 8.0;
        }

        self.flow_heads(f, ["Widget", "Tooltip", TASKBAR]);

        // Rows, grouped by provider.
        let mut groups: Vec<Vec<&RowChoice>> = Vec::new();
        for row in &input.rows {
            match groups.last_mut() {
                Some(g) if canonical(g[0].provider) == canonical(row.provider) => g.push(row),
                _ => groups.push(vec![row]),
            }
        }
        for (gi, g) in groups.iter().enumerate() {
            // A hairline between providers.
            if gi > 0 {
                f.y += 4.0;
                f.shape(Shape::Fill {
                    rect: rect(PAD_X, f.y, BOX_W - PAD_X * 2.0, 1.0),
                    radius: 0.0,
                    ink: Ink::Separator,
                    alpha: Level::Full,
                });
                f.y += 5.0;
            }
            // A provider with one limit for everything has no extra rows.
            let one_limit = g.len() == 1 && !g[0].id.contains('.');
            for row in g {
                self.flow_row(f, row, welcome, one_limit);
            }
            // Where its sign-in comes from, at the end of its group.
            if !welcome {
                self.flow_sign_in(f, g[0].provider);
            }
        }

        f.y += 10.0;
    }

    /// The column heads, pinned to the top while the list scrolls.
    pub(super) fn flow_heads(&self, f: &mut Flow, labels: [&str; 3]) {
        let heads: Vec<Shape> = [COL_WIDGET, COL_TOOLTIP, COL_TASKBAR]
            .into_iter()
            .zip(labels)
            .map(|(cx, label)| {
                let w = self.adv(label, SIZE_NOTE);
                text_shape(
                    (cx - w / 2.0).round(),
                    f.y + ((COLS_H - lh(SIZE_NOTE)) / 2.0).round(),
                    label,
                    SIZE_NOTE,
                    Ink::Muted,
                    false,
                )
            })
            .collect();
        // Drawn once, by the frame, in place or pinned.
        f.sticky = Some((
            f.y,
            COLS_H,
            heads.into_iter().map(|s| s.moved(0.0, -f.y)).collect(),
        ));
        f.y += COLS_H;
    }

    /// A section's title: small capitals over the rows under it.
    pub(super) fn flow_section(&self, f: &mut Flow, title: &str) {
        f.y += 8.0;
        f.shape(text_shape(PAD_X, f.y, title, SIZE_NOTE, Ink::Dim, true));
        f.y += lh(SIZE_NOTE) + 2.0;
    }

    /// A row of the What shows tab: its label and note on the left, its
    /// description on hover. Its top and height, for the ticks beside it.
    pub(super) fn flow_labelled(
        &self,
        f: &mut Flow,
        label: &str,
        note: &str,
        hint: &str,
    ) -> (f32, f32) {
        let notes = self.wrap(
            &[span(note, SIZE_NOTE, Ink::Muted, false)],
            LABEL_RIGHT - PAD_X,
        );
        let block = lh(SIZE_TEXT) + notes.len() as f32 * lh(SIZE_NOTE);
        let h = (block + 8.0).max(ROW_H);
        let y = f.y;
        f.els.push(El::RowBg(rect(6.0, y, BOX_W - 12.0, h)));
        let top = y + ((h - block) / 2.0).round();
        f.shape(text_shape(PAD_X, top, label, SIZE_TEXT, Ink::Text, false));
        f.lines(&notes, PAD_X, top + lh(SIZE_TEXT), lh(SIZE_NOTE));
        f.els.push(El::Hint {
            rect: rect(PAD_X, y, LABEL_RIGHT - PAD_X, h),
            text: hint.to_string(),
        });
        (y, h)
    }

    /// A dash in a column that has nothing to choose there.
    pub(super) fn flow_dash(&self, f: &mut Flow, cx: f32, y: f32, h: f32) {
        let w = self.adv(DASH, SIZE_NOTE);
        f.shape(text_shape(
            (cx - w / 2.0).round(),
            y + ((h - lh(SIZE_NOTE)) / 2.0).round(),
            DASH,
            SIZE_NOTE,
            Ink::Dim,
            false,
        ));
    }

    /// The What shows tab: what each row shows in the widget, the tooltip
    /// and on the panel; the model label; the tray icon's look; the model
    /// names and the settings file.
    pub(super) fn flow_shows(&self, f: &mut Flow, welcome: bool) {
        self.flow_heads(f, ["Widget", "Tooltip", PANEL]);
        self.flow_section(f, SECT_NUMBERS);
        for (part, label, note, hint) in TIP_PARTS {
            let (y, h) = self.flow_labelled(f, label, note, hint);
            for (col, place, cx) in [
                (0, NumbersOn::Widget, COL_WIDGET),
                (1, NumbersOn::Tooltip, COL_TOOLTIP),
                (2, NumbersOn::Panel, COL_TASKBAR),
            ] {
                // The tooltip has no bar to show or hide.
                if part == TipPart::Bar && place == NumbersOn::Tooltip {
                    self.flow_dash(f, cx, y, h);
                    continue;
                }
                let cell = rect(cx - COL_W / 2.0, y, COL_W, h);
                let enabled = self.tip_enabled(part, place);
                f.els.push(El::Tick {
                    target: Target::Tip(part, place),
                    cell,
                    on: self.tip_on(part, place),
                    enabled,
                    nav: (f.nav, col),
                });
                // The last one on there says why it stays.
                if !enabled {
                    f.els.push(El::Hint {
                        rect: cell,
                        text: lock_hint(place).to_string(),
                    });
                }
            }
            f.nav += 1;
            f.y += h;
        }
        self.flow_section(f, SECT_MODEL);
        for (part, label, note) in PARTS {
            let (y, h) = self.flow_labelled(f, label, note, part_hint(part));
            for (col, (surface, cx)) in SURFACES.iter().enumerate() {
                f.els.push(El::Tick {
                    target: Target::Part {
                        part,
                        surface: *surface,
                    },
                    cell: rect(cx - COL_W / 2.0, y, COL_W, h),
                    on: self.part_on(*surface, part),
                    enabled: self.part_enabled(*surface, part),
                    nav: (f.nav, col as u32),
                });
            }
            // The panel has no room for a model.
            self.flow_dash(f, COL_TASKBAR, y, h);
            f.nav += 1;
            f.y += h;
        }
        let read_note = self.wrap(
            &[span(READ_NOTE, SIZE_NOTE, Ink::Muted, false)],
            BOX_W - SUB_X - PAD_X,
        );
        f.y += f.lines(&read_note, SUB_X, f.y, lh(SIZE_NOTE)) + 4.0;
        // Allowed once: Forget permission takes it back.
        if self.input.reads.tool_settings || self.input.reads.last_session {
            const PAD: f32 = 4.0;
            let w = self.adv(FORGET, SIZE_NOTE).ceil();
            let r = rect(SUB_X - PAD, f.y, w + PAD * 2.0, lh(SIZE_NOTE) + 8.0);
            f.els.push(El::Link {
                target: Target::ForgetModelReads,
                rect: r,
                label: FORGET.to_string(),
                pad: PAD,
                nav: (f.nav, 0),
            });
            f.els.push(El::Hint {
                rect: r,
                text: FORGET_MODELS_HINT.to_string(),
            });
            f.nav += 1;
            f.y += r.h;
        }

        // Where the model comes from.
        let enabled = self.any_part_on();
        f.y += 6.0;
        f.shape(Shape::Text {
            x: PAD_X,
            y: f.y,
            text: MODEL_SHOWN.to_string(),
            size: SIZE_NOTE,
            ink: Ink::Dim,
            bold: false,
            italic: false,
            alpha: if enabled {
                Level::Full
            } else {
                Level::Disabled
            },
        });
        f.y += lh(SIZE_NOTE) + 4.0;
        let mut x = PAD_X;
        for (col, (source, label, hint)) in [
            (ModelSource::Configured, SOURCE_SET, SOURCE_SET_HINT),
            (ModelSource::LastUsed, SOURCE_LAST, SOURCE_LAST_HINT),
        ]
        .into_iter()
        .enumerate()
        {
            let w = RADIO + 8.0 + self.adv(label, SIZE_TEXT);
            f.els.push(El::Hint {
                rect: rect(x, f.y, w.ceil(), RADIO_H),
                text: hint.to_string(),
            });
            f.els.push(El::Radio {
                target: Target::Source(source),
                rect: rect(x, f.y, w.ceil(), RADIO_H),
                label: label.to_string(),
                on: self.source() == source,
                enabled,
                nav: (f.nav, col as u32),
            });
            x += w.ceil() + 16.0;
        }
        f.nav += 1;
        f.y += RADIO_H;

        self.flow_tray(f);
        // Model names: closed until opened, then one heading per tool.
        self.flow_sep(f);
        f.y += 4.0;
        let models = self.renamable();
        let count = if models.is_empty() {
            String::new()
        } else {
            let mut keys: Vec<&str> = models
                .iter()
                .map(|&mi| self.input.models[mi].key.as_str())
                .collect();
            keys.sort_unstable();
            keys.dedup();
            count_of(keys.len(), "model")
        };
        let h = ROW_H - 6.0;
        f.els.push(El::Toggle {
            target: Target::Names,
            rect: rect(6.0, f.y, BOX_W - 12.0, h),
            label: NAMES.to_string(),
            count,
            open: self.names_open,
            bold: true,
            arrow: false,
            nav: (f.nav, 0),
        });
        f.els.push(El::Hint {
            rect: rect(6.0, f.y, BOX_W - 12.0, h),
            text: NAMES_HINT.to_string(),
        });
        f.nav += 1;
        f.y += h;
        if self.names_open {
            let note = if models.is_empty() {
                NAMES_OFF
            } else {
                NAMES_ON
            };
            let note = self.wrap(
                &[span(note, SIZE_NOTE, Ink::Muted, false)],
                BOX_W - SUB_X - PAD_X,
            );
            f.y += f.lines(&note, SUB_X, f.y, lh(SIZE_NOTE)) + 4.0;
            if !models.is_empty() && !self.model_on() {
                let note = self.wrap(
                    &[span(NAMES_WHERE, SIZE_NOTE, Ink::Accent, false)],
                    BOX_W - SUB_X - PAD_X,
                );
                f.y += f.lines(&note, SUB_X, f.y, lh(SIZE_NOTE)) + 4.0;
            }
            let mut tools: Vec<&'static str> = Vec::new();
            for &mi in &models {
                let p = self.input.models[mi].provider;
                if !tools.contains(&p) {
                    tools.push(p);
                }
            }
            for p in tools {
                let mine: Vec<usize> = models
                    .iter()
                    .copied()
                    .filter(|&mi| self.input.models[mi].provider == p)
                    .collect();
                let open = self.open_names.iter().any(|o| o == p);
                f.els.push(El::Toggle {
                    target: Target::NamesOf(p.to_string()),
                    rect: rect(SUB_X - 8.0, f.y, BOX_W - SUB_X + 2.0, h),
                    label: self.provider_name(p),
                    count: mine.len().to_string(),
                    open,
                    bold: false,
                    arrow: false,
                    nav: (f.nav, 0),
                });
                f.nav += 1;
                f.y += h;
                if open {
                    for mi in mine {
                        self.flow_name(f, mi);
                    }
                }
            }
        }
        // The settings file, for what has no place here. Not in the
        // welcome, which is about what to read.
        if !welcome {
            self.flow_settings_file(f);
        }
        f.y += 10.0;
    }

    /// The tray icon's look: a tile for each, the chosen one outlined.
    pub(super) fn flow_tray(&self, f: &mut Flow) {
        f.y += 4.0;
        self.flow_section(f, SECT_TRAY);
        f.y += 4.0;
        let gap = 6.0;
        let n = TRAY_STYLES.len() as f32;
        let w = ((BOX_W - PAD_X * 2.0 - gap * (n - 1.0)) / n).floor();
        let mut x = PAD_X;
        for (col, (style, label, hint)) in TRAY_STYLES.into_iter().enumerate() {
            let r = rect(x, f.y, w, TILE_H);
            f.els.push(El::Tile {
                target: Target::TrayStyle(style),
                rect: r,
                label: label.to_string(),
                style,
                on: self.input.tray_style == style,
                nav: (f.nav, col as u32),
            });
            f.els.push(El::Hint {
                rect: r,
                text: hint.to_string(),
            });
            x += w + gap;
        }
        f.nav += 1;
        f.y += TILE_H + 6.0;
        let note = self.wrap(
            &[span(TRAY_NOTE, SIZE_NOTE, Ink::Muted, false)],
            BOX_W - PAD_X * 2.0,
        );
        f.y += f.lines(&note, PAD_X, f.y, lh(SIZE_NOTE)) + 2.0;
    }

    /// The list of a tab.
    pub(super) fn flow_tab(&self, f: &mut Flow, tab: Tab, welcome: bool) {
        match tab {
            Tab::Rows => self.flow_rows(f, welcome),
            Tab::Shows => self.flow_shows(f, welcome),
        }
    }

    /// "Edit config.toml…": a small link at the list's end, its description
    /// on hover.
    pub(super) fn flow_settings_file(&self, f: &mut Flow) {
        self.flow_sep(f);
        const PAD: f32 = 4.0;
        let w = self.adv(SETTINGS_FILE, SIZE_NOTE).ceil();
        let r = rect(PAD_X - PAD, f.y, w + PAD * 2.0, lh(SIZE_NOTE) + 8.0);
        f.els.push(El::Link {
            target: Target::SettingsFile,
            rect: r,
            label: SETTINGS_FILE.to_string(),
            pad: PAD,
            nav: (f.nav, 0),
        });
        f.els.push(El::Hint {
            rect: r,
            text: SETTINGS_FILE_HINT.to_string(),
        });
        f.nav += 1;
        f.y += r.h;
    }

    /// One row: its name (and note) with a Widget and a Tooltip tick. A
    /// provider's own row has a dot for how it runs (not in the welcome,
    /// where nothing runs yet); an extra row says what it is in a word or
    /// two beside its name. `one_limit`: the provider has no extra rows,
    /// which its note says while it works.
    pub(super) fn flow_row(&self, f: &mut Flow, row: &RowChoice, welcome: bool, one_limit: bool) {
        let own = !row.id.contains('.');
        let x = if own { PAD_X } else { SUB_X };
        let width = LABEL_RIGHT - x;
        let name = if own {
            row.name
        } else {
            row.name.split_once(" · ").map_or(row.name, |(_, b)| b)
        };
        let dot = (own && !welcome).then(|| self.dot_of(row));
        let name_x = if dot.is_some() { x + DOT + DOT_GAP } else { x };
        let mut notes: Vec<String> = Vec::new();
        if one_limit && row.provider_ok {
            notes.push(ONE_LIMIT.to_string());
        } else if !row.note.is_empty() {
            notes.push(row.note.to_string());
        }
        // Allowed but switched off: the menu, not a missing consent.
        if own && self.allowed(row.provider) && !self.enabled(row.provider) {
            notes.push(OFF_IN_MENU.to_string());
        }
        // Missing only once the provider answered: not while it loads, so
        // the rows do not grow and shrink under the cursor after Allow.
        if !own && !row.has_data && row.provider_ok && self.allowed(row.provider) {
            notes.push(NOT_REPORTED.to_string());
        }
        let mut note_lines = Vec::new();
        for n in &notes {
            note_lines.extend(self.wrap(&[span(n.as_str(), SIZE_NOTE, Ink::Muted, false)], width));
        }
        let block = lh(SIZE_TEXT) + note_lines.len() as f32 * lh(SIZE_NOTE);
        let h = (block + 8.0).max(ROW_H);
        let y = f.y;
        f.els.push(El::RowBg(rect(6.0, y, BOX_W - 12.0, h)));
        let top = y + ((h - block) / 2.0).round();
        if !own {
            f.shape(text_shape(
                PAD_X + 1.0,
                top,
                "+",
                SIZE_TEXT,
                Ink::Dim,
                false,
            ));
        }
        if let Some(ink) = dot {
            let r = rect(x, top + DOT_TOP, DOT, DOT);
            f.shape(if ink == Ink::Dim {
                Shape::Stroke {
                    rect: r,
                    radius: DOT / 2.0,
                    width: 1.0,
                    ink,
                    alpha: Level::Full,
                }
            } else {
                Shape::Fill {
                    rect: r,
                    radius: DOT / 2.0,
                    ink,
                    alpha: Level::Full,
                }
            });
        }
        f.shape(text_shape(name_x, top, name, SIZE_TEXT, Ink::Text, own));
        // An extra row's word or two, beside its name when there is room.
        if !row.short.is_empty() {
            let sx = (name_x + self.adv(name, SIZE_TEXT) + 6.0).round();
            if sx + self.adv(row.short, SIZE_NOTE) <= LABEL_RIGHT {
                f.shape(text_shape(
                    sx,
                    top + (lh(SIZE_TEXT) - lh(SIZE_NOTE)),
                    row.short,
                    SIZE_NOTE,
                    Ink::Dim,
                    false,
                ));
            }
        }
        if !row.hint.is_empty() {
            f.els.push(El::Hint {
                rect: rect(x, y, LABEL_RIGHT - x, h),
                text: row.hint.to_string(),
            });
        }
        if own && welcome && self.found(row.provider) {
            let bx = (name_x + self.adv_styled(name, SIZE_TEXT, true) + 8.0).round();
            let bw = (self.adv_styled(FOUND, SIZE_BADGE, true) + 14.0).round();
            f.shape(Shape::Fill {
                rect: rect(bx, top, bw, lh(SIZE_TEXT)),
                radius: 8.0,
                ink: Ink::Hover,
                alpha: Level::Full,
            });
            f.shape(text_shape(
                bx + 7.0,
                top + ((lh(SIZE_TEXT) - lh(SIZE_BADGE)) / 2.0).round(),
                FOUND,
                SIZE_BADGE,
                Ink::Accent,
                true,
            ));
            let sure = !self.found_weakly(row.provider);
            f.els.push(El::Hint {
                rect: rect(bx, top, bw, lh(SIZE_TEXT)),
                text: crate::consent::found_hint(canonical(row.provider), sure).to_string(),
            });
        }
        f.lines(&note_lines, x, top + lh(SIZE_TEXT), lh(SIZE_NOTE));
        for (col, (surface, cx)) in SURFACES.iter().enumerate() {
            f.els.push(El::Tick {
                target: Target::Row {
                    row: row.id.to_string(),
                    surface: *surface,
                },
                cell: rect(cx - COL_W / 2.0, y, COL_W, h),
                on: self.row_on(*surface, row),
                enabled: true,
                nav: (f.nav, col as u32),
            });
        }
        // The taskbar panel and the tray icon show providers, not extra
        // rows: a provider's own row has the third tick.
        if own {
            let cell = rect(COL_TASKBAR - COL_W / 2.0, y, COL_W, h);
            let on = self.taskbar_on(row.provider);
            let enabled = self.taskbar_enabled(row.provider);
            f.els.push(El::Tick {
                target: Target::Taskbar(canonical(row.provider).to_string()),
                cell,
                on,
                enabled,
                nav: (f.nav, SURFACES.len() as u32),
            });
            let hint = match (enabled, on) {
                (true, _) => TASKBAR_HINT,
                (false, true) => TASKBAR_LAST,
                (false, false) => TASKBAR_FULL,
            };
            f.els.push(El::Hint {
                rect: cell,
                text: hint.to_string(),
            });
        }
        f.nav += 1;
        f.y += h;
    }

    /// A provider's sign-in under its own row: one line saying where it
    /// comes from and how it went, which opens to choose the source, to
    /// paste or remove a key, or to sign in to Cursor.
    pub(super) fn flow_sign_in(&self, f: &mut Flow, provider: &str) {
        let p = canonical(provider);
        let Some(s) = self.input.sign_in.iter().find(|s| s.provider == p) else {
            return;
        };
        let open = self.open_sign_in.iter().any(|o| o == p);
        // A chip as wide as what it says: "Sign-in Claude Code · connected ›".
        let chip = rect(
            SUB_X,
            f.y + 2.0,
            self.chip_w(SIGN_IN, &s.summary).min(BOX_W - PAD_X - SUB_X),
            CHIP_H,
        );
        f.els.push(El::Toggle {
            target: Target::SignIn(p.to_string()),
            rect: chip,
            label: SIGN_IN.to_string(),
            count: s.summary.clone(),
            open,
            bold: false,
            arrow: true,
            nav: (f.nav, 0),
        });
        f.els.push(El::Hint {
            rect: chip,
            text: SIGN_IN_HINT.to_string(),
        });
        f.nav += 1;
        f.y = bottom(&chip) + 6.0;
        if !open {
            return;
        }
        let x = SUB_X + 14.0;
        let width = BOX_W - PAD_X - x;
        for (col, src) in s.sources.iter().enumerate() {
            let w = (RADIO + 8.0 + self.adv(src.label, SIZE_TEXT)).ceil();
            f.els.push(El::Radio {
                target: Target::SignInSource(p.to_string(), src.id),
                rect: rect(x, f.y, w, RADIO_H),
                label: src.label.to_string(),
                on: src.on,
                enabled: true,
                nav: (f.nav, col as u32),
            });
            f.nav += 1;
            f.y += RADIO_H;
        }
        if !s.sources.is_empty() {
            f.y += 2.0;
        }
        let detail = self.wrap(&[span(&s.detail, SIZE_NOTE, Ink::Muted, false)], width);
        f.y += f.lines(&detail, x, f.y, lh(SIZE_NOTE)) + 6.0;
        // The actions, in one row.
        let mut buttons: Vec<(Target, String, bool, bool)> = Vec::new();
        if let Some(label) = s.paste {
            buttons.push((Target::Paste(p.to_string()), label.to_string(), false, true));
            buttons.push((
                Target::Remove(p.to_string()),
                REMOVE.to_string(),
                false,
                s.stored,
            ));
        }
        match s.cursor {
            Some(CursorState::SignedOut) => {
                buttons.push((Target::CursorSignIn, CURSOR_SIGN_IN.to_string(), true, true));
            }
            Some(CursorState::Waiting) => {
                buttons.push((Target::CursorCancel, CURSOR_CANCEL.to_string(), false, true));
            }
            Some(CursorState::SignedIn) => {
                buttons.push((
                    Target::CursorSignOut,
                    CURSOR_SIGN_OUT.to_string(),
                    false,
                    true,
                ));
            }
            None => {}
        }
        if s.forget {
            buttons.push((
                Target::Forget(p.to_string()),
                FORGET.to_string(),
                false,
                true,
            ));
        }
        if !buttons.is_empty() {
            let mut bx = x;
            for (col, (target, label, primary, enabled)) in buttons.into_iter().enumerate() {
                let w = self.button_w(&label);
                let r = rect(bx, f.y, w, BUTTON_H);
                if matches!(target, Target::Forget(_)) {
                    f.els.push(El::Hint {
                        rect: r,
                        text: FORGET_HINT.to_string(),
                    });
                }
                f.els.push(El::Button {
                    target,
                    rect: r,
                    label,
                    primary,
                    enabled,
                    nav: (f.nav, col as u32),
                });
                bx += w + 8.0;
            }
            f.nav += 1;
            f.y += BUTTON_H + 6.0;
        }
        let note = match (s.cursor, &s.note) {
            (_, Some(n)) => Some(n.as_str()),
            (Some(CursorState::Waiting), None) => Some(CURSOR_WAITING),
            _ => None,
        };
        if let Some(n) = note {
            let lines = self.wrap(&[span(n, SIZE_NOTE, Ink::Accent, false)], width);
            f.y += f.lines(&lines, x, f.y, lh(SIZE_NOTE)) + 4.0;
        }
        f.y += 6.0;
    }

    pub(super) fn flow_sep(&self, f: &mut Flow) {
        f.y += 8.0;
        f.shape(Shape::Fill {
            rect: rect(10.0, f.y, BOX_W - 20.0, 1.0),
            radius: 0.0,
            ink: Ink::Separator,
            alpha: Level::Full,
        });
        f.y += 9.0;
    }

    /// One model's name field: which id, whether this tool uses it now and
    /// which other tools share the name, the field, the room left.
    pub(super) fn flow_name(&self, f: &mut Flow, mi: usize) {
        let m = &self.input.models[mi];
        let x = SUB_X + 14.0;
        let width = BOX_W - PAD_X - x;
        f.y += 4.0;
        let mut spans = vec![span(
            shorten(&m.model_id, ID_CHARS),
            SIZE_NOTE,
            Ink::Muted,
            false,
        )];
        if m.current {
            spans.push(span(format!("  {IN_USE}"), SIZE_NOTE, Ink::Accent, true));
        }
        if !m.shared_with.is_empty() {
            spans.push(span(
                format!("  shared with {}", m.shared_with.join(", ")),
                SIZE_NOTE,
                Ink::Muted,
                false,
            ));
        }
        let label = self.wrap(&spans, width);
        f.y += f.lines(&label, x, f.y, lh(SIZE_NOTE)) + 3.0;
        let target = Target::Field {
            key: m.key.clone(),
            provider: m.provider.to_string(),
        };
        f.els.push(El::Field {
            target: target.clone(),
            rect: rect(x, f.y, width, FIELD_H),
            model: mi,
            nav: (f.nav, 0),
        });
        f.nav += 1;
        f.y += FIELD_H + 4.0;
        // How much room is left: for the field being typed in only.
        if !self.focused(&target) {
            f.y += 2.0;
            return;
        }
        let text = self.field_text(m);
        let full = matches!(&self.edit, Some(e) if e.key == m.key && e.full);
        let hint = room_text(self.chars_left(text.trim()), text.trim().is_empty());
        let (ink, bold) = if full {
            (Ink::Accent, true)
        } else {
            (Ink::Muted, false)
        };
        let hint = self.wrap(&[span(hint, SIZE_NOTE, ink, bold)], width);
        f.y += f.lines(&hint, x, f.y, lh(SIZE_NOTE)) + 6.0;
    }

    /// The update notice: every file read and every host the tokens go to.
    pub(super) fn flow_notice(&self, f: &mut Flow) {
        let width = BOX_W - PAD_X * 2.0;
        f.y += 2.0;
        self.flow_para(
            f,
            &[span(
                "From now on, AI Limits asks before it reads any file. The tools you already use keep working; nothing changes for them:",
                SIZE_TEXT,
                Ink::Text,
                false,
            )],
            width,
        );
        for p in self.allowed_in_order() {
            let Some(info) = crate::consent::read_info(&p) else {
                continue;
            };
            let mut spans = vec![span(format!("{}:", info.tool), SIZE_TEXT, Ink::Text, true)];
            spans.push(span(
                format!(" {}", info.sign_in.join("; ")),
                SIZE_NOTE,
                Ink::Text,
                false,
            ));
            spans.push(span(
                format!("; its {} goes only to ", info.secret()),
                SIZE_TEXT,
                Ink::Text,
                false,
            ));
            spans.push(span(info.hosts.join(" and "), SIZE_NOTE, Ink::Text, false));
            if self.input.reads.tool_settings {
                if let Some(s) = info.settings {
                    spans.push(span(". The model: ", SIZE_TEXT, Ink::Text, false));
                    spans.push(span(s, SIZE_NOTE, Ink::Text, false));
                }
            }
            if self.input.reads.last_session {
                if let Some(s) = info.session {
                    spans.push(span(". Last session: ", SIZE_TEXT, Ink::Text, false));
                    spans.push(span(s, SIZE_NOTE, Ink::Text, false));
                }
            }
            if let Some(risk) = info.risk {
                spans.push(span(format!(". {risk}"), SIZE_TEXT, Ink::Text, false));
            }
            self.flow_item(f, &spans, width);
        }
        f.y += 4.0;
        self.flow_para(
            f,
            &[span(
                "A new tool or option asks once before its first read. Clearing a tool's rows pauses it; Forget permission under its Sign-in stops reading its file.",
                SIZE_TEXT,
                Ink::Text,
                false,
            )],
            width,
        );
    }

    pub(super) fn flow_para(&self, f: &mut Flow, spans: &[Span], width: f32) {
        let lines = self.wrap(spans, width);
        f.y += f.lines(&lines, PAD_X, f.y, lh(SIZE_TEXT)) + 8.0;
    }

    pub(super) fn flow_item(&self, f: &mut Flow, spans: &[Span], width: f32) {
        let lines = self.wrap(spans, width - 18.0);
        f.shape(text_shape(
            PAD_X + 4.0,
            f.y,
            "•",
            SIZE_TEXT,
            Ink::Text,
            false,
        ));
        f.y += f.lines(&lines, PAD_X + 18.0, f.y, lh(SIZE_TEXT)) + 4.0;
    }

    /// The providers that may be read, in the order the picker lists them.
    pub(super) fn allowed_in_order(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let listed = self
            .input
            .rows
            .iter()
            .map(|r| r.provider)
            .chain(self.input.reads.sign_in.iter().map(String::as_str));
        for p in listed {
            let p = canonical(p).to_string();
            if self.allowed(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// The question's title, its lead-in, the files, and what follows.
    pub(super) fn ask_text(&self, question: &Question) -> (String, String, Vec<String>, String) {
        // The files a settings or session question covers: every tool's
        // that has one, so a tool allowed later is never read unnamed.
        let files = |pick: fn(&crate::consent::ReadInfo) -> Option<&'static str>| {
            crate::consent::READ_INFO
                .iter()
                .filter_map(pick)
                .map(String::from)
                .collect::<Vec<String>>()
        };
        match question {
            Question::Read(ReadKind::SignIn(p)) => {
                let info = crate::consent::read_info(p);
                let tool = info.map_or(p.as_str(), |i| i.tool);
                let name = self.provider_name(p);
                let mut list: Vec<String> = info
                    .map(|i| i.sign_in.iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                // Reads already allowed for every tool apply to this one too.
                if let Some(i) = info {
                    list.extend(self.allowed_model_files(i).into_iter().map(String::from));
                }
                let hosts = info.map(|i| i.hosts.join(" and ")).unwrap_or_default();
                let goes = if hosts.is_empty() {
                    String::new()
                } else {
                    format!(" and goes only to {hosts}, to ask for your usage")
                };
                if info.is_some_and(|i| !i.tool_sign_in) {
                    // The app signs in by itself (Cursor): nothing of the
                    // tool's is read for it, and the session is the app's.
                    return (
                        format!("Show {name}?"),
                        format!(
                            "To show your {name} limits, AI Limits signs in by itself and reads no file of {tool}'s for it:"
                        ),
                        list,
                        format!(
                            "The session stays on this PC{goes}. AI Limits renews it before it runs out; the editor's own sign-in is never touched. Asked once: clearing its rows pauses it, Sign out ends it."
                        ),
                    );
                }
                (
                    format!("Allow access to the {tool} sign-in?"),
                    format!("To show your {name} limits, AI Limits reads:"),
                    list,
                    format!(
                        "The token stays on this PC{goes}. It is never changed or refreshed. Asked once: clearing its rows pauses it, Forget permission under its Sign-in takes it back."
                    ),
                )
            }
            Question::Found => (
                FOUND_TITLE.to_string(),
                FOUND_LEAD.to_string(),
                Vec::new(),
                FOUND_OUTRO.to_string(),
            ),
            Question::Read(ReadKind::ToolSettings) => (
                "Allow reading tool settings files?".to_string(),
                "To show the model, its version and effort, AI Limits reads these values from each tool's own settings file:".to_string(),
                files(|i| i.settings),
                "Only those values are kept, in memory, and nothing is sent anywhere. Every running tool's file is read, also one signed in with a pasted key or token. Asked once: clearing Model, Version and Reasoning effort pauses it, Forget permission under them takes it back.".to_string(),
            ),
            Question::Read(ReadKind::LastSession) => (
                "Allow reading the latest session file?".to_string(),
                "For Last session, AI Limits reads the newest session file back from its end until it finds the model, at most the last 1 MB:".to_string(),
                files(|i| i.session),
                "Only the model and its effort are kept, in memory. Your conversations are not stored or sent anywhere. Switch back to Tool settings to stop, or Forget permission under Model, Version and Reasoning effort.".to_string(),
            ),
        }
    }

    /// The question laid out in its box's own coordinates: its text, which
    /// may scroll, and its buttons, which stay.
    pub(super) fn ask_layout(&self, ask: &Ask) -> AskLayout {
        let (title, lead, files, outro) = self.ask_text(&ask.question);
        let inner = BOX_W - ASK_INSET * 2.0 - ASK_PAD * 2.0;
        let mut f = Flow::new();
        f.nav = 0;
        f.y = ASK_TOP;
        let t = self.wrap(&[span(title, SIZE_TITLE, Ink::Text, true)], inner);
        f.y += f.lines(&t, ASK_PAD, f.y, lh(SIZE_TITLE)) + 6.0;
        let l = self.wrap(&[span(lead, SIZE_TEXT, Ink::Text, false)], inner);
        f.y += f.lines(&l, ASK_PAD, f.y, lh(SIZE_TEXT)) + 4.0;
        for file in files {
            self.flow_bullet(&mut f, &file, inner);
        }
        // What reading this tool's sign-in may cost, before it is allowed.
        let risk = match &ask.question {
            Question::Read(ReadKind::SignIn(p)) => {
                crate::consent::read_info(p).and_then(|i| i.risk)
            }
            _ => None,
        };
        if let Some(risk) = risk {
            f.y += 4.0;
            let r = self.wrap(&[span(risk, SIZE_TEXT, Ink::Text, false)], inner);
            f.y += f.lines(&r, ASK_PAD, f.y, lh(SIZE_TEXT)) + 2.0;
        }
        if ask.question == Question::Found {
            for id in &ask.tools {
                self.flow_pick(&mut f, ask, id, inner);
            }
        }
        let others = match &ask.question {
            Question::Read(ReadKind::SignIn(p)) => self.others_found(p),
            _ => Vec::new(),
        };
        if !others.is_empty() {
            f.y += 4.0;
            let l = self.wrap(&[span(ALLOW_ALL_LEAD, SIZE_TEXT, Ink::Text, false)], inner);
            f.y += f.lines(&l, ASK_PAD, f.y, lh(SIZE_TEXT)) + 4.0;
            for info in others.iter().filter_map(|p| crate::consent::read_info(p)) {
                let entry = format!("{}: {}", info.tool, self.found_entry(info));
                self.flow_bullet(&mut f, &entry, inner);
            }
        }
        f.y += 4.0;
        let o = self.wrap(&[span(outro, SIZE_TEXT, Ink::Text, false)], inner);
        f.y += f.lines(&o, ASK_PAD, f.y, lh(SIZE_TEXT));
        let text_h = f.y;

        let row = f.nav;
        let mut buttons = Vec::new();
        let (aw, cw) = (self.button_w("Allow"), self.button_w("Cancel"));
        let right = BOX_W - ASK_INSET * 2.0 - ASK_PAD;
        let allow_x = right - cw - 8.0 - aw;
        if !others.is_empty() {
            let w = self.button_w(ALLOW_ALL);
            buttons.push(El::Button {
                target: Target::AllowAll,
                rect: rect(allow_x - 8.0 - w, 0.0, w, BUTTON_H),
                label: ALLOW_ALL.into(),
                primary: false,
                enabled: true,
                nav: (row, 0),
            });
        }
        buttons.push(El::Button {
            target: Target::Allow,
            rect: rect(allow_x, 0.0, aw, BUTTON_H),
            label: "Allow".into(),
            primary: true,
            enabled: self.can_allow(ask),
            nav: (row, 1),
        });
        buttons.push(El::Button {
            target: Target::Cancel,
            rect: rect(right - cw, 0.0, cw, BUTTON_H),
            label: "Cancel".into(),
            primary: false,
            enabled: true,
            nav: (row, 2),
        });
        AskLayout {
            text_h,
            text: f.els,
            buttons,
        }
    }

    /// Whether Allow can answer: the welcome's question needs a tool ticked.
    pub(super) fn can_allow(&self, ask: &Ask) -> bool {
        ask.question != Question::Found || !ask.picked.is_empty()
    }

    /// One item of a question's list, behind a bullet.
    pub(super) fn flow_bullet(&self, f: &mut Flow, text: &str, inner: f32) {
        let lines = self.wrap(&[span(text, SIZE_NOTE, Ink::Text, false)], inner - 18.0);
        f.shape(text_shape(
            ASK_PAD + 4.0,
            f.y,
            "•",
            SIZE_NOTE,
            Ink::Text,
            false,
        ));
        f.y += f.lines(&lines, ASK_PAD + 18.0, f.y, lh(SIZE_NOTE)) + 2.0;
    }

    /// A tool in the welcome's question: its tick, its name, what is read
    /// and where its token goes, and why it is unticked when only a related
    /// tool was found. The whole entry toggles the tick.
    pub(super) fn flow_pick(&self, f: &mut Flow, ask: &Ask, id: &str, inner: f32) {
        let Some(info) = crate::consent::read_info(id) else {
            return;
        };
        let x = ASK_PAD + PICK_BOX + 8.0;
        let width = inner - PICK_BOX - 8.0;
        let top = f.y + 2.0;
        let name = self.wrap(&[span(info.tool, SIZE_TEXT, Ink::Text, true)], width);
        let detail = self.wrap(
            &[span(self.found_entry(info), SIZE_NOTE, Ink::Text, false)],
            width,
        );
        let weak = if self.found_weakly(id) {
            self.wrap(
                &[span(
                    crate::consent::weak_note(info.id),
                    SIZE_NOTE,
                    Ink::Muted,
                    false,
                )],
                width,
            )
        } else {
            Vec::new()
        };
        let h =
            name.len() as f32 * lh(SIZE_TEXT) + (detail.len() + weak.len()) as f32 * lh(SIZE_NOTE);
        // The entry first, so its hover wash lies under its text.
        f.els.push(El::Pick {
            target: Target::Pick(id.to_string()),
            rect: rect(ASK_PAD - 6.0, top - 3.0, inner + 12.0, h + 6.0),
            check: rect(
                ASK_PAD,
                top + ((lh(SIZE_TEXT) - PICK_BOX) / 2.0).round(),
                PICK_BOX,
                PICK_BOX,
            ),
            on: ask.picked.iter().any(|p| p == id),
            nav: (f.nav, 0),
        });
        f.nav += 1;
        let mut y = top;
        y += f.lines(&name, x, y, lh(SIZE_TEXT));
        y += f.lines(&detail, x, y, lh(SIZE_NOTE));
        y += f.lines(&weak, x, y, lh(SIZE_NOTE));
        f.y = y + 6.0;
    }
}
