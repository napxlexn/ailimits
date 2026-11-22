// ui/picker/scene.rs — a layout turned into shapes for the renderer, with
// hover, focus and presses; what a point is over; the descriptions shown on
// hover and on keyboard focus.

use super::*;

impl Picker {
    /// The picker as shapes, for `Renderer::draw_picker`.
    pub fn scene(&self) -> PickerScene {
        self.scene_with(true)
    }

    /// `cull`: leave out list rows scrolled out of sight.
    pub(super) fn scene_with(&self, cull: bool) -> PickerScene {
        let f = self.frame();
        let (hover, hover_row) = match self.pointer {
            Some((x, y)) => (self.hit(&f, x, y), self.hover_row(&f, x, y)),
            None => (Hit::Nothing, None),
        };
        let hovered = match &hover {
            Hit::Target(t) => Some(t),
            _ => None,
        };
        // The description the pointer rests on; without one, that of the
        // control the keyboard moved the focus to.
        let hint = self
            .pointer
            .and_then(|(x, y)| self.hint_at(&f, x, y))
            .or_else(|| self.focus_hint(&f))
            .and_then(|i| match &f.els[i].1 {
                El::Hint { rect, text } => Some(self.hint_shapes(&f, rect, text)),
                _ => None,
            });
        let mut chrome = Vec::new();
        let mut body = Vec::new();
        let mut overlay = Vec::new();
        let mut ask_body = Vec::new();
        let mut top = Vec::new();
        let (vx, vy) = (f.viewport.x, f.viewport.y);
        let ask_viewport = f.ask.map(|a| a.viewport);
        // Off-screen rows are not drawn.
        let seen = |el: &El, v: &Rect| {
            !cull || el_rect(el).is_none_or(|r| bottom(&r) > v.y && r.y < bottom(v))
        };
        for (i, (layer, el)) in f.els.iter().enumerate() {
            let mut shapes = Vec::new();
            self.el_shapes(el, hovered, hover_row == Some(i), &mut shapes);
            match layer {
                Layer::Chrome => chrome.extend(shapes),
                Layer::Overlay => overlay.extend(shapes),
                Layer::Top => top.extend(shapes),
                Layer::Body => {
                    if seen(el, &f.viewport) {
                        body.extend(shapes.iter().map(|s| s.moved(-vx, -vy)));
                    }
                }
                Layer::AskBody => {
                    if let Some(v) = ask_viewport.filter(|v| seen(el, v)) {
                        ask_body.extend(shapes.iter().map(|s| s.moved(-v.x, -v.y)));
                    }
                }
            }
        }
        if let Some(s) = &f.sticky {
            body.push(
                Shape::Fill {
                    rect: s.rect,
                    radius: 0.0,
                    ink: Ink::Surface,
                    alpha: Level::Full,
                }
                .moved(-vx, -vy),
            );
            body.extend(s.shapes.iter().map(|s| s.moved(-vx, -vy)));
            // Pinned, a hairline shows the rows pass under the heads.
            if s.pinned {
                body.push(
                    Shape::Fill {
                        rect: rect(f.boxr.x + 10.0, bottom(&s.rect) - 1.0, BOX_W - 20.0, 1.0),
                        radius: 0.0,
                        ink: Ink::Separator,
                        alpha: Level::Full,
                    }
                    .moved(-vx, -vy),
                );
            }
        }
        // On top of everything: the description.
        top.extend(hint.into_iter().flatten());
        PickerScene {
            width: f.width,
            height: f.height,
            chrome,
            viewport: f.viewport,
            body,
            overlay,
            ask_viewport,
            ask_body,
            top,
        }
    }

    // ─── Shapes with state ───────────────────────────────────────

    pub(super) fn focused(&self, t: &Target) -> bool {
        self.focus.as_ref() == Some(t)
    }

    pub(super) fn ring(&self, t: &Target) -> bool {
        self.focus_visible && self.focused(t)
    }

    pub(super) fn el_shapes(
        &self,
        el: &El,
        hovered: Option<&Target>,
        row_hover: bool,
        out: &mut Vec<Shape>,
    ) {
        let pressed = |t: &Target| self.pressed.as_ref() == Some(t) && hovered == Some(t);
        match el {
            El::Shape(s) => out.push(s.clone()),
            El::RowBg(r) => {
                if row_hover {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: Level::Full,
                    });
                }
            }
            El::Tick {
                target,
                cell,
                on,
                enabled,
                ..
            } => {
                let (cx, cy) = (cell.x + cell.w / 2.0, (cell.y + cell.h / 2.0).round());
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(
                            cx - TICK_RING / 2.0,
                            cy - TICK_RING / 2.0,
                            TICK_RING,
                            TICK_RING,
                        ),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                out.push(Shape::Check {
                    rect: rect(cx - TICK / 2.0, cy - TICK / 2.0, TICK, TICK),
                    on: *on,
                    alpha: if !*enabled {
                        Level::DisabledMark
                    } else if pressed(target) {
                        Level::MarkPressed
                    } else {
                        Level::Full
                    },
                });
            }
            El::Pick {
                target,
                rect: r,
                check,
                on,
                ..
            } => {
                if hovered == Some(target) {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 6.0,
                        ink: Ink::Hover,
                        alpha: Level::Full,
                    });
                }
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: *r,
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                out.push(Shape::Check {
                    rect: *check,
                    on: *on,
                    alpha: if pressed(target) {
                        Level::MarkPressed
                    } else {
                        Level::Full
                    },
                });
            }
            El::Radio {
                target,
                rect: r,
                label,
                on,
                enabled,
                ..
            } => {
                let label_level = if *enabled {
                    Level::Full
                } else {
                    Level::Disabled
                };
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(r.x - 4.0, r.y, r.w + 8.0, r.h),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                out.push(Shape::Radio {
                    rect: rect(r.x, r.y + ((r.h - RADIO) / 2.0).round(), RADIO, RADIO),
                    on: *on,
                    alpha: if pressed(target) {
                        Level::MarkPressed
                    } else {
                        label_level
                    },
                });
                out.push(Shape::Text {
                    x: r.x + RADIO + 8.0,
                    y: r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round(),
                    text: label.clone(),
                    size: SIZE_TEXT,
                    ink: Ink::Text,
                    bold: false,
                    italic: false,
                    alpha: label_level,
                });
            }
            El::Field {
                target,
                rect: r,
                model,
                ..
            } => {
                let m = &self.input.models[*model];
                let focused = self.focused(target);
                out.push(Shape::Fill {
                    rect: *r,
                    radius: 4.0,
                    ink: Ink::Hover,
                    alpha: if hovered == Some(target) && !focused {
                        Level::FieldHover
                    } else {
                        Level::Full
                    },
                });
                out.push(Shape::Stroke {
                    rect: *r,
                    radius: 4.0,
                    width: 1.0,
                    ink: Ink::Border,
                    alpha: Level::Full,
                });
                let (line_h, ink) = if focused {
                    (2.0, Ink::Accent)
                } else {
                    (1.0, Ink::Muted)
                };
                out.push(Shape::Fill {
                    rect: rect(r.x + 2.0, bottom(r) - line_h, r.w - 4.0, line_h),
                    radius: 0.0,
                    ink,
                    alpha: Level::Full,
                });
                let tx = r.x + 8.0;
                let ty = r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round();
                let text = self.field_text(m);
                if text.is_empty() {
                    out.push(Shape::Text {
                        x: tx,
                        y: ty,
                        text: self.placeholder(&m.automatic, r.w - 16.0),
                        size: SIZE_TEXT,
                        ink: Ink::Muted,
                        bold: false,
                        italic: true,
                        alpha: Level::Placeholder,
                    });
                } else {
                    out.push(text_shape(tx, ty, &text, SIZE_TEXT, Ink::Text, false));
                }
                if focused {
                    let caret = self
                        .edit
                        .as_ref()
                        .filter(|e| e.key == m.key)
                        .map_or(text.len(), |e| e.caret.min(text.len()));
                    let cx = (tx + self.adv(&text[..caret], SIZE_TEXT)).round();
                    out.push(Shape::Fill {
                        rect: rect(cx, r.y + 7.0, 1.0, r.h - 14.0),
                        radius: 0.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
            }
            El::Button {
                target,
                rect: r,
                label,
                primary,
                enabled,
                ..
            } => {
                let hover = *enabled && hovered == Some(target);
                let down = *enabled && pressed(target);
                if *primary {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Accent,
                        alpha: if !*enabled {
                            Level::Disabled
                        } else if down {
                            Level::AccentPressed
                        } else if hover {
                            Level::AccentHover
                        } else {
                            Level::Full
                        },
                    });
                } else {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: if down {
                            Level::Pressed
                        } else if hover {
                            Level::Hovered
                        } else {
                            Level::Full
                        },
                    });
                    out.push(Shape::Stroke {
                        rect: *r,
                        radius: 4.0,
                        width: 1.0,
                        ink: Ink::Border,
                        alpha: Level::Full,
                    });
                }
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                let w = self.adv(label, SIZE_TEXT);
                out.push(Shape::Text {
                    x: (r.x + (r.w - w) / 2.0).round(),
                    y: r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round(),
                    text: label.clone(),
                    size: SIZE_TEXT,
                    ink: if *primary { Ink::OnAccent } else { Ink::Text },
                    bold: false,
                    italic: false,
                    alpha: if *enabled {
                        Level::Full
                    } else {
                        Level::Disabled
                    },
                });
            }
            El::Toggle {
                target,
                rect: r,
                label,
                count,
                open,
                bold,
                arrow,
                ..
            } => {
                if hovered == Some(target) {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: if pressed(target) {
                            Level::Hovered
                        } else {
                            Level::Full
                        },
                    });
                }
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: *r,
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                let ty = r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round();
                let sign = match (*arrow, *open) {
                    // Arrowheads the UI font has (it lacks the triangles).
                    (true, true) => "\u{02C5}",
                    (true, false) => "\u{02C3}",
                    (false, true) => "\u{2212}",
                    (false, false) => "+",
                };
                out.push(text_shape(
                    r.x + 8.0,
                    ty,
                    sign,
                    SIZE_TEXT,
                    Ink::Muted,
                    false,
                ));
                let lx = r.x + 22.0;
                out.push(text_shape(lx, ty, label, SIZE_TEXT, Ink::Text, *bold));
                if !count.is_empty() {
                    let cx = (lx + self.adv_styled(label, SIZE_TEXT, *bold) + 8.0).round();
                    out.push(text_shape(
                        cx,
                        r.y + ((r.h - lh(SIZE_NOTE)) / 2.0).round(),
                        count,
                        SIZE_NOTE,
                        Ink::Muted,
                        false,
                    ));
                }
            }
            El::Hint { .. } => {}
            El::Link {
                target,
                rect: r,
                label,
                pad,
                ..
            } => {
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: *r,
                        radius: 4.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: Level::Full,
                    });
                }
                let (x, y) = (r.x + pad, r.y + ((r.h - lh(SIZE_NOTE)) / 2.0).round());
                let down = pressed(target);
                let level = if down {
                    Level::AccentPressed
                } else {
                    Level::Full
                };
                out.push(Shape::Text {
                    x,
                    y,
                    text: label.clone(),
                    size: SIZE_NOTE,
                    ink: Ink::Accent,
                    bold: false,
                    italic: false,
                    alpha: level,
                });
                // Underlined under the pointer, as a link is.
                if down || hovered == Some(target) {
                    out.push(Shape::Fill {
                        rect: rect(
                            x,
                            y + lh(SIZE_NOTE) - 2.0,
                            self.adv(label, SIZE_NOTE).round(),
                            1.0,
                        ),
                        radius: 0.0,
                        ink: Ink::Accent,
                        alpha: level,
                    });
                }
            }
            El::Close { rect: r, .. } => {
                let t = Target::Close;
                if hovered == Some(&t) || self.ring(&t) {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: if pressed(&t) {
                            Level::Hovered
                        } else {
                            Level::Full
                        },
                    });
                }
                out.push(Shape::Cross {
                    rect: *r,
                    ink: Ink::Text,
                });
            }
        }
    }

    /// A name field's automatic name, as its placeholder: cut with an
    /// ellipsis to the field's width (a long cloud id makes a long name).
    pub(super) fn placeholder(&self, automatic: &str, width: f32) -> String {
        const MARK: &str = "  (automatic)";
        if automatic.trim().is_empty() {
            return "(automatic)".to_string();
        }
        let whole = format!("{automatic}{MARK}");
        // Italic leans about a pixel past its advance.
        if self.adv(&whole, SIZE_TEXT) + 1.0 <= width {
            return whole;
        }
        let room = width - 1.0 - self.adv(&format!("…{MARK}"), SIZE_TEXT);
        let mut end = 0;
        for (i, c) in automatic.char_indices() {
            let next = i + c.len_utf8();
            if self.adv(&automatic[..next], SIZE_TEXT) > room {
                break;
            }
            end = next;
        }
        format!("{}…{MARK}", automatic[..end].trim_end())
    }

    // ─── Hit testing ─────────────────────────────────────────────

    pub(super) fn hit(&self, f: &Frame, x: f32, y: f32) -> Hit {
        if !contains(&f.boxr, x, y) {
            return Hit::Outside;
        }
        // A scroll bar's strip, over what lies under it: the question's
        // while one is open (the list is still then), else the list's.
        match &f.ask {
            Some(a) if a.bar.is_some_and(|b| contains(&b.grip, x, y)) => {
                return Hit::Bar { ask: true };
            }
            None if f.bar.is_some_and(|b| contains(&b.grip, x, y)) => {
                return Hit::Bar { ask: false };
            }
            _ => {}
        }
        let in_list = f.in_list(x, y);
        let in_ask = f.in_ask_text(x, y);
        for (layer, el) in f.els.iter().rev() {
            let Some((t, r, _)) = el.active() else {
                continue;
            };
            // While a question is open only it and the cross answer.
            if self.ask.is_some()
                && !matches!(
                    t,
                    Target::Allow
                        | Target::AllowAll
                        | Target::Cancel
                        | Target::Close
                        | Target::Pick(_)
                )
            {
                continue;
            }
            if (*layer == Layer::Body && !in_list) || (*layer == Layer::AskBody && !in_ask) {
                continue;
            }
            if contains(&r, x, y) {
                return Hit::Target(t.clone());
            }
        }
        Hit::Nothing
    }

    /// The index in `f.els` of the row wash under the point.
    pub(super) fn hover_row(&self, f: &Frame, x: f32, y: f32) -> Option<usize> {
        if self.ask.is_some() || !f.in_list(x, y) {
            return None;
        }
        f.els.iter().position(|(layer, el)| {
            *layer == Layer::Body && matches!(el, El::RowBg(r) if contains(r, x, y))
        })
    }

    pub(super) fn hover_state(&self) -> (Hit, Option<usize>, Option<usize>) {
        match self.pointer {
            Some((x, y)) => {
                let f = self.frame();
                (
                    self.hit(&f, x, y),
                    self.hover_row(&f, x, y),
                    self.hint_at(&f, x, y),
                )
            }
            None => (Hit::Nothing, None, None),
        }
    }

    /// The description the pointer rests on: a hint element under it in the
    /// list, while no question is open and no button is held.
    pub(super) fn hint_at(&self, f: &Frame, x: f32, y: f32) -> Option<usize> {
        if self.ask.is_some() || self.pressed.is_some() || !f.in_list(x, y) {
            return None;
        }
        f.els
            .iter()
            .position(|(_, el)| matches!(el, El::Hint { rect, .. } if contains(rect, x, y)))
    }

    /// The description of the control the keyboard moved the focus to: the
    /// hint on its row, while it is in sight and no question is open.
    pub(super) fn focus_hint(&self, f: &Frame) -> Option<usize> {
        if !self.focus_visible || self.ask.is_some() || self.pressed.is_some() {
            return None;
        }
        let t = self.focus.as_ref()?;
        let r = f.els.iter().find_map(|(layer, el)| {
            let (et, r, _) = el.active()?;
            (*layer == Layer::Body && et == t).then_some(r)
        })?;
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        if !f.in_list(cx, cy) {
            return None;
        }
        let on_row = |i: usize| match &f.els[i] {
            (Layer::Body, El::Hint { rect, .. }) => rect.y == r.y && rect.h == r.h,
            _ => false,
        };
        // Another control's own description: a hint laid over exactly the
        // cell of a control other than the focused one (the Taskbar tick's
        // says what the taskbar shows, never what Widget or Tooltip does).
        let others: Vec<Rect> = f
            .els
            .iter()
            .filter_map(|(layer, el)| {
                let (et, er, _) = el.active()?;
                (*layer == Layer::Body && et != t).then_some(er)
            })
            .collect();
        let own = |i: usize| match &f.els[i].1 {
            El::Hint { rect, .. } => contains(rect, cx, cy),
            _ => false,
        };
        let foreign = |i: usize| match &f.els[i].1 {
            El::Hint { rect, .. } => others
                .iter()
                .any(|o| (o.x, o.y, o.w, o.h) == (rect.x, rect.y, rect.w, rect.h)),
            _ => true,
        };
        // The focused control's own description, else its row's.
        (0..f.els.len())
            .find(|&i| on_row(i) && own(i))
            .or_else(|| (0..f.els.len()).find(|&i| on_row(i) && !foreign(i)))
    }

    /// A description as a small box under what it describes (above it when
    /// there is no room below), inside the window. A row's description stops
    /// short of the tick columns, so the next row's ticks stay in reach.
    pub(super) fn hint_shapes(&self, f: &Frame, r: &Rect, text: &str) -> Vec<Shape> {
        const PAD: f32 = 8.0;
        const MAX_W: f32 = 260.0;
        let left = f.boxr.x + 8.0;
        let right = if r.x + r.w <= f.boxr.x + LABEL_RIGHT + 0.5 {
            f.boxr.x + LABEL_RIGHT + 6.0
        } else {
            f.boxr.x + f.boxr.w - 8.0
        };
        let one_line = self.adv(text, SIZE_NOTE);
        let inner = one_line.min(MAX_W.min(right - left) - PAD * 2.0);
        let lines = self.wrap(
            &[span(text, SIZE_NOTE, Ink::Text, false)],
            inner.ceil() + 1.0,
        );
        let w = (inner + PAD * 2.0).ceil();
        let h = (lines.len() as f32 * lh(SIZE_NOTE) + PAD * 1.5).ceil();
        let x = r.x.max(left).min(right - w).round();
        let below = bottom(r) + 4.0;
        let y = if below + h <= bottom(&f.boxr) - 8.0 {
            below
        } else {
            (r.y - 4.0 - h).max(f.boxr.y + 8.0)
        }
        .round();
        let boxr = rect(x, y, w, h);
        let mut out = vec![
            Shape::Shadow {
                rect: boxr,
                radius: 6.0,
            },
            Shape::Fill {
                rect: boxr,
                radius: 6.0,
                ink: Ink::Surface,
                alpha: Level::Full,
            },
            Shape::Stroke {
                rect: boxr,
                radius: 6.0,
                width: 1.0,
                ink: Ink::Border,
                alpha: Level::Full,
            },
        ];
        for (i, line) in lines.iter().enumerate() {
            let ly = y + (PAD * 0.75).round() + i as f32 * lh(SIZE_NOTE);
            out.extend(
                line.iter()
                    .map(|run| run_shape(run, x + PAD, ly, lh(SIZE_NOTE))),
            );
        }
        out
    }
}

/// The rectangle a laid-out element covers, for culling.
pub(super) fn el_rect(el: &El) -> Option<Rect> {
    match el {
        El::Shape(Shape::Fill { rect, .. })
        | El::Shape(Shape::Stroke { rect, .. })
        | El::Shape(Shape::Check { rect, .. })
        | El::Shape(Shape::Radio { rect, .. })
        | El::Shape(Shape::Cross { rect, .. })
        | El::Shape(Shape::Shadow { rect, .. })
        | El::RowBg(rect)
        | El::Tick { cell: rect, .. }
        | El::Pick { rect, .. }
        | El::Radio { rect, .. }
        | El::Field { rect, .. }
        | El::Button { rect, .. }
        | El::Close { rect, .. }
        | El::Toggle { rect, .. }
        | El::Hint { rect, .. }
        | El::Link { rect, .. } => Some(*rect),
        El::Shape(Shape::Text { x, y, size, .. }) => Some(rect(*x, *y, 1.0, lh(*size))),
    }
}
