// app/events.rs — every event the loop gets, handed to the part of `App`
// it is for: the picker's window, the taskbar panel, the widget, the timers,
// and the app's own events (provider updates, model reads, the shell).

#[cfg(target_os = "windows")]
use super::indicator::{
    eval_indicator_fallback, FallbackVerdict, BOUNCE_EVERY, BOUNCE_FOR, SETTLE,
};
#[cfg(target_os = "windows")]
use super::widget::apply_windows_glass;
use super::{
    cache::{cacheable_provider_data, merge_cached_metrics, prune_provider_cache_map},
    surface_rows,
    widget::{show_widget, toggle_or_raise_widget},
    App, AppCommand, TrayKind, UserEvent,
};
use crate::{
    config::schema::Surface,
    hooks,
    models::Models,
    notifications::toast::ToastNotifier,
    providers::{ProviderData, ProviderId, ProviderStatus},
    rows,
    ui::renderer::format_duration,
};
use chrono::Utc;
use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
};
use tracing::{debug, warn};

impl App {
    /// One event of the loop.
    pub(super) fn handle(
        &mut self,
        event: Event<'_, UserEvent>,
        target: &EventLoopWindowTarget<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        // Sleep until the soonest timer, or until the next event.
        #[cfg(target_os = "windows")]
        {
            // The panel's countdown, while it shows one, is one more timer.
            let wake = [self.indicator.next_wake(), self.panel.next_tick()]
                .into_iter()
                .flatten()
                .min();
            *control_flow = match wake {
                Some(t) => ControlFlow::WaitUntil(t),
                None => ControlFlow::Wait,
            };
        }
        #[cfg(not(target_os = "windows"))]
        {
            *control_flow = ControlFlow::Wait;
        }

        match event {
            // Scheduled deadlines came due: re-check overlay coverage (toggle
            // the tray fallback), and/or show the hover tooltip after its
            // delay. Idle returns to 0% afterwards (no pending timer).
            #[cfg(target_os = "windows")]
            Event::NewEvents(_) => self.on_timers(),

            // The picker's own window: before every other window arm, which
            // would take its events (and its close) for the widget's.
            Event::WindowEvent {
                window_id, event, ..
            } if self.picker.as_ref().is_some_and(|p| p.id() == window_id) => {
                let actions = self
                    .picker
                    .as_mut()
                    .map(|p| p.handle_window_event(&event))
                    .unwrap_or_default();
                if !actions.is_empty() {
                    self.apply_picker_actions(actions);
                }
            }

            Event::RedrawRequested(id) if self.picker.as_ref().is_some_and(|p| p.id() == id) => {
                if let Some(p) = self.picker.as_mut() {
                    p.redraw();
                }
            }

            // The embedded panel's own events: a left-click toggles the
            // overlay (like the tray icon), a right-click opens the menu.
            Event::WindowEvent {
                window_id, event, ..
            } if window_id == self.panel.window_id() => self.on_panel_window(event),

            Event::WindowEvent { event, .. } => self.on_widget_window(event, target, control_flow),

            Event::RedrawRequested(id) if id == self.panel.window_id() => {
                self.panel.redraw(&self.visible());
            }

            Event::RedrawRequested(_) => self.redraw_widget(),

            Event::UserEvent(ue) => self.on_user_event(ue, target, control_flow),

            // tao calls process::exit() right after this event, and that does
            // not wait for the background writers. Hand them the final state
            // and wait for them to finish, so a setting changed moments
            // before quitting survives — and no in-flight background write
            // can land after it. This one arm covers every exit path (window
            // close, tray Quit, menu Quit).
            Event::LoopDestroyed => self.shut_down(),

            _ => {}
        }
    }

    // ─── Timers (Windows) ────────────────────────────────────────

    #[cfg(target_os = "windows")]
    fn eval_fallback(&mut self) -> FallbackVerdict {
        let visible = self.visible();
        eval_indicator_fallback(
            &mut self.panel,
            &mut self.tray,
            &self.menu.menu,
            &visible,
            &self.theme,
            &mut self.indicator.fallback_was_active,
            &mut self.indicator.fullscreen_was_active,
            &mut self.indicator.covered_since,
            self.config.general.panel_display,
        )
    }

    #[cfg(target_os = "windows")]
    fn on_timers(&mut self) {
        let now = std::time::Instant::now();
        // The countdown on the panel reads differently now: repaint it, and
        // nothing else (no placement query).
        if self.panel.next_tick().is_some_and(|t| now >= t) {
            self.panel.redraw(&self.visible());
        }
        if let Some(t) = self.indicator.recheck_at {
            if now >= t {
                self.indicator.recheck_at = None;
                // Re-place before re-judging. The bar slides over ~200ms and
                // its move events arrive mid-slide, so acting on the first
                // pins the panel to an animation frame (measured: bar top
                // 1410, 1413, settling at 1392) - hanging below the bar, half
                // off the screen. Nothing else re-runs placement: the
                // evaluation below only redraws into the rectangle it is
                // given.
                self.panel.on_taskbar_moved(&self.visible());
                let verdict = self.eval_fallback();
                if verdict.left_fullscreen {
                    self.indicator.bounce_until = Some(now + BOUNCE_FOR);
                }
                if let Some(rest) = verdict.look_again_in {
                    self.indicator.recheck_by(now + rest);
                }
                // The verdict may now be resting on its trailing hold alone -
                // the game has let go and only the timer still says
                // otherwise. Look again exactly when it lapses; without this
                // the panel waits for whatever event happens by next, which
                // after an alt-tab can be a while, and the screen sits there
                // with no indicator.
                if let Some(rest) = crate::platform::fullscreen_hold_remaining() {
                    self.indicator.recheck_at =
                        Some(now + rest + std::time::Duration::from_millis(5));
                }
            }
        }
        // Straight after a fullscreen app lets go the shell keeps its bar in
        // a band an ordinary topmost window cannot beat, and the panel -
        // placed, painted, correct - is simply BEHIND the bar. Measured: the
        // panel's own pixel showed the taskbar for 174 ms after the alt-tab,
        // and what finally uncovered it was a raise on the next scheduled
        // look. So it is lifted again every BOUNCE_EVERY until it shows, and
        // given up on after BOUNCE_FOR - a few dozen cheap calls, once per
        // game, and nothing at all in ordinary use.
        if let Some(deadline) = self.indicator.bounce_until {
            if now >= deadline || !self.panel.is_covered() {
                self.indicator.bounce_until = None;
            } else {
                self.panel.raise();
                self.indicator.recheck_by(now + BOUNCE_EVERY);
            }
        }
        if let Some(t) = self.indicator.tooltip_at {
            if now >= t {
                self.indicator.tooltip_at = None;
                self.panel.show_tooltip(
                    &surface_rows(&self.config, &self.display, Surface::Tooltip, &self.models),
                    rows::nothing_chosen(&self.config),
                    &self.config.ui.tooltip,
                );
            }
        }
    }

    // ─── The taskbar panel ───────────────────────────────────────

    fn on_panel_window(&mut self, event: WindowEvent<'_>) {
        match event {
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                toggle_or_raise_widget(&self.window, &self.win_state, &mut self.window_visible);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                // Opened once this event is handled (app.rs, the loop).
                #[cfg(target_os = "windows")]
                {
                    self.menu_request = Some(self.panel.hwnd());
                }
            }
            // Our own hover tooltip: arm the system hover delay on
            // enter/hover (show only if the cursor lingers, like a tray-icon
            // tooltip), hide and cancel on leave.
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorMoved { .. } => {
                #[cfg(target_os = "windows")]
                if !self.panel.tooltip_shown() && self.indicator.tooltip_at.is_none() {
                    self.indicator.tooltip_at =
                        Some(std::time::Instant::now() + self.indicator.hover_delay);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.panel.hide_tooltip();
                #[cfg(target_os = "windows")]
                {
                    self.indicator.tooltip_at = None;
                }
            }
            _ => {}
        }
    }

    // ─── The widget ──────────────────────────────────────────────

    fn on_widget_window(
        &mut self,
        event: WindowEvent<'_>,
        target: &EventLoopWindowTarget<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

            WindowEvent::Focused(_) => {
                #[cfg(target_os = "windows")]
                apply_windows_glass(&self.window);
                self.window.request_redraw();
            }

            // The system switched light/dark theme — repaint the taskbar
            // panel and the tray icon so their monochrome ink keeps
            // contrasting with the bar (the tray follows the taskbar theme,
            // not the widget's).
            WindowEvent::ThemeChanged(_) => {
                self.panel.update(&self.visible(), true);
                self.tray.update(&self.visible(), &self.theme, true);
            }

            // A DPI change when crossing monitors: pin the physical size and
            // re-apply the accent — otherwise the second monitor shows a
            // white background.
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                *new_inner_size =
                    PhysicalSize::new(self.win_layout.width as u32, self.win_layout.height as u32);
                #[cfg(target_os = "windows")]
                apply_windows_glass(&self.window);
                self.window.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_inside_window = true;
                self.win_state.cursor_in_window = (position.x, position.y);
                if let Ok(wp) = self.window.outer_position() {
                    // Screen cursor = window position + in-window position.
                    let screen = (wp.x as f64 + position.x, wp.y as f64 + position.y);
                    let win_state = &mut self.win_state;
                    if win_state.drag.active {
                        // Hold Shift while dragging → soft-magnet the widget
                        // to the nearest work-area edge of the monitor it is
                        // currently over.
                        let magnet = if crate::platform::shift_held() {
                            let cx = (win_state.pos.0 + win_state.size.0 / 2.0) as i32;
                            let cy = (win_state.pos.1 + win_state.size.1 / 2.0) as i32;
                            crate::platform::work_area_at(cx, cy).map(|(l, t, r, b)| {
                                (
                                    (l as f64, t as f64, r as f64, b as f64),
                                    crate::ui::window::MAGNET_RANGE,
                                )
                            })
                        } else {
                            None
                        };
                        if let Some((nx, ny)) = win_state.update_drag(screen, magnet) {
                            self.window
                                .set_outer_position(PhysicalPosition::new(nx, ny));
                        }
                    } else if win_state.drag_pending {
                        // The drag starts from the REAL current cursor
                        // position, not the one cached at click time —
                        // otherwise the window jumped to the click point
                        // after the context menu.
                        win_state.drag_pending = false;
                        win_state.pos = (wp.x as f64, wp.y as f64);
                        win_state.begin_drag(screen);
                    }
                }
                self.window.request_redraw();
            }

            WindowEvent::CursorLeft { .. } => {
                self.cursor_inside_window = false;
                self.window.request_redraw();
            }

            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => {
                    // Drag from anywhere, but it starts only on the first
                    // cursor move; a locked widget will not move.
                    if !self.config.window.locked {
                        self.win_state.drag_pending = true;
                    }
                }
                (ElementState::Released, MouseButton::Left) => {
                    // "Choose what to show / Click here", or the rows chosen
                    // have nothing to show: a click that did not drag opens
                    // the picker, with the focus.
                    let dragged = self.win_state.end_drag();
                    if !dragged && (rows::nothing_chosen(&self.config) || self.shape.unshown) {
                        self.open_picker(target, None, true);
                    }
                    if dragged {
                        // Persist the position to the config.
                        self.config.window.pos_x = self.win_state.pos.0.round() as i32;
                        self.config.window.pos_y = self.win_state.pos.1.round() as i32;
                        self.save_config();
                        // The accent gets lost when dragging to another
                        // monitor — re-apply it.
                        #[cfg(target_os = "windows")]
                        apply_windows_glass(&self.window);
                        self.window.request_redraw();
                    }
                }
                (ElementState::Pressed, MouseButton::Right) => {
                    // The native context menu at the cursor.
                    // Opened once this event is handled (app.rs, the loop).
                    #[cfg(target_os = "windows")]
                    {
                        use tao::platform::windows::WindowExtWindows;
                        self.menu_request = Some(self.window.hwnd() as _);
                    }
                }
                _ => {}
            },

            _ => {}
        }
    }

    pub(super) fn redraw_widget(&mut self) {
        // Render only the visible providers, as the widget's rows.
        let visible = surface_rows(&self.config, &self.display, Surface::Widget, &self.models);
        self.renderer.draw(
            &mut self.pixmap,
            &self.win_layout,
            &self.theme,
            &visible,
            self.config.window.opacity,
            &self.config.ui.detail,
            self.cursor_inside_window.then_some((
                self.win_state.cursor_in_window.0 as f32,
                self.win_state.cursor_in_window.1 as f32,
            )),
            if self.config.ui.show_forecast {
                &self.forecasts
            } else {
                &self.empty_forecasts
            },
            &self.last_errors,
        );
        // Copy the pixmap into softbuffer: RGBA → 0xAARRGGBB.
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            for (dst, src) in buffer.iter_mut().zip(self.pixmap.pixels().iter()) {
                let c = src.demultiply();
                *dst = ((c.alpha() as u32) << 24)
                    | ((c.red() as u32) << 16)
                    | ((c.green() as u32) << 8)
                    | (c.blue() as u32);
            }
            if let Err(e) = buffer.present() {
                warn!("present failed: {e}");
            }
        }
    }

    // ─── The app's own events ────────────────────────────────────

    fn on_user_event(
        &mut self,
        event: UserEvent,
        target: &EventLoopWindowTarget<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            #[cfg(target_os = "windows")]
            UserEvent::MenuHover(ev) => self.on_menu_hover(ev),
            UserEvent::Command(cmd) => match cmd {
                AppCommand::UpdateProvider { data, serial } => {
                    self.on_provider_update(data, serial, target)
                }
                AppCommand::CursorSignIn { approval, ticket } => {
                    self.on_cursor_sign_in(approval, ticket)
                }
                AppCommand::UsageToken {
                    pid,
                    check,
                    token,
                    ticket,
                } => self.on_usage_token(pid, check, token, ticket),
                AppCommand::Models(generation, read) => self.on_models(generation, read),
                AppCommand::Quit => *control_flow = ControlFlow::Exit,
            },

            UserEvent::TaskbarMoved => {
                // While fullscreen-suppressed this is a no-op (the panel
                // gates every presentation path itself).
                self.panel.on_taskbar_moved(&self.visible());
                // The bar settled into a new position; re-check coverage a
                // beat later (the "rude topmost" front lands after the slide).
                #[cfg(target_os = "windows")]
                {
                    self.indicator.recheck_at = Some(std::time::Instant::now() + SETTLE);
                }
            }

            UserEvent::PanelRaise => self.on_foreground_changed(target),

            UserEvent::ShowWidget => {
                show_widget(&self.window, &mut self.window_visible);
                // Started again by hand: the user is at the desktop, not in a
                // fullscreen app, so an owed window that waited opens now.
                self.try_auto_open(target, || false);
            }

            #[cfg(target_os = "windows")]
            UserEvent::PanelMenu => {
                // `raise` steps under a menu that reaches over the panel
                // instead of fronting it, so this one call serves both the
                // menu opening and its closing.
                self.panel.raise();
            }

            #[cfg(target_os = "windows")]
            UserEvent::DisplaysChanged => self.on_displays_changed(),

            // Arm the coalesced re-check; a burst of reorders collapses to
            // one.
            #[cfg(target_os = "windows")]
            UserEvent::PanelRecheck => {
                self.indicator.recheck_at = Some(std::time::Instant::now() + SETTLE);
            }

            UserEvent::Tray(TrayKind::LeftClick) => {
                // Bring the overlay to the front (or hide it if it is already
                // there); the tray icon stays either way.
                toggle_or_raise_widget(&self.window, &self.win_state, &mut self.window_visible);
            }

            UserEvent::Menu(id) => self.on_menu(id, target, control_flow),
        }
    }

    /// The foreground changed (tray overflow flyout, Start menu, a
    /// fullscreen game, an app).
    fn on_foreground_changed(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        // Evaluate the fallback FIRST: if a fullscreen app just took over,
        // this hides the panel, and the raise below becomes a no-op
        // (rect=None) — never re-asserting the overlay above a game. Arm a
        // delayed re-check too — the auto-hide bar can peek back (or the
        // shell can flip its fullscreen state) a moment later with no
        // further event.
        #[cfg(target_os = "windows")]
        {
            let verdict = self.eval_fallback();
            let now = std::time::Instant::now();
            if verdict.left_fullscreen {
                self.indicator.bounce_until = Some(now + BOUNCE_FOR);
            }
            if let Some(rest) = verdict.look_again_in {
                self.indicator.recheck_by(now + rest);
            }
            // 150ms for the bar's own settling, or sooner when the
            // fullscreen verdict is only resting on its hold: the alt-tab
            // out of a game is exactly this case, and the panel should be
            // back the moment the hold lapses.
            let now = std::time::Instant::now();
            let wait = if self.indicator.bounce_until.is_some() {
                BOUNCE_EVERY
            } else {
                crate::platform::fullscreen_hold_remaining()
                    .map(|rest| rest + std::time::Duration::from_millis(5))
                    .unwrap_or(SETTLE)
            };
            self.indicator.recheck_at = Some(now + wait);
        }
        self.panel.raise();
        // Another window is in front now: an owed window that waited for a
        // fullscreen app may be due.
        self.try_auto_open(target, crate::platform::foreground_fills_its_monitor);
    }

    /// Both are fixed here rather than at the next start: the overlay,
    /// which is borderless and skips the taskbar, so a position on no screen
    /// makes it invisible AND un-draggable; and the panel, whose bar may
    /// have been on the monitor that went (`resolve_taskbar` then falls back
    /// to the primary bar, but only when something asks it to).
    #[cfg(target_os = "windows")]
    fn on_displays_changed(&mut self) {
        if let Ok(wp) = self.window.outer_position() {
            let (nx, ny) = crate::platform::ensure_on_screen(wp.x, wp.y);
            if (nx, ny) != (wp.x, wp.y) {
                self.window
                    .set_outer_position(PhysicalPosition::new(nx, ny));
                self.win_state.pos = (nx as f64, ny as f64);
                self.config.window.pos_x = nx;
                self.config.window.pos_y = ny;
                self.save_config();
                tracing::info!("the overlay was on no monitor; moved to {nx},{ny}");
                apply_windows_glass(&self.window);
                self.window.request_redraw();
            }
        }
        // Re-point the watch: the bar it compares against may be a destroyed
        // window now, and a dead handle fires nothing.
        crate::platform::watch_taskbar(self.config.general.panel_display);
        self.panel.on_taskbar_moved(&self.visible());
        self.indicator.recheck_at = Some(std::time::Instant::now() + SETTLE);
    }

    // ─── Provider updates ────────────────────────────────────────

    fn on_provider_update(
        &mut self,
        data: ProviderData,
        serial: u64,
        target: &EventLoopWindowTarget<UserEvent>,
    ) {
        // Made by an instance that has since been replaced: it was in flight
        // while the user signed out, signed in as someone else, changed the
        // key or took the consent back. Its numbers are not shown, cached or
        // alerted on.
        if !self.providers.is_current(&data.id, serial) {
            debug!(
                "provider {:?}: an update from before a rebuild dropped",
                data.id
            );
            return;
        }
        self.alerts_and_trend(&data);
        self.hold_or_show(data);
        // A provider appeared/disappeared (e.g. became NotConfigured) →
        // resize.
        if self.relayout_if_reshaped() {
            self.sync_tray();
            self.panel.update(&self.visible(), true);
        }
        self.window.request_redraw();
        self.tray.update(&self.visible(), &self.theme, false);
        self.panel.update(&self.visible(), false);
        self.refresh_picker();
        // A minute-by-minute chance for an owed window that waited, should
        // no foreground change have said so.
        self.try_auto_open(target, crate::platform::foreground_fills_its_monitor);
    }

    /// What a fresh reading sets off: the threshold alert and hook, the
    /// reset hook, the burn-rate forecast, the startup hook.
    fn alerts_and_trend(&mut self, data: &ProviderData) {
        let active = self
            .config
            .providers
            .iter()
            .find(|p| ProviderId::from_config_id(&p.id) == Some(data.id.clone()))
            .is_some_and(|p| self.config.provider_active(p));
        // Threshold crossing drives BOTH the toast and the `on_threshold`
        // hook: once per limit window, not again every cooldown while usage
        // stays up, and not again after a restart (notifications/alerts.rs).
        // The hook fires even if toasts are disabled (it's its own opt-in).
        if let Some(pct) = data.primary_percentage() {
            let threshold = self.providers.threshold(&data.id) as f32;
            let cooldown =
                chrono::Duration::minutes(self.config.notifications.cooldown_minutes.max(1) as i64);
            // A provider switched off meanwhile alerts nothing.
            let seen = active.then(|| {
                self.alert_book.observe(
                    &data.id,
                    pct,
                    threshold,
                    data.headline_reset(),
                    Utc::now(),
                    cooldown,
                )
            });
            if seen.is_some_and(|s| s.changed) {
                self.persist_alerts();
            }
            if seen.is_some_and(|s| s.fire) {
                if self.config.notifications.enabled {
                    let body = match data.headline_reset() {
                        Some(t) => {
                            let secs = t.signed_duration_since(Utc::now()).num_seconds().max(0);
                            format!(
                                "{}% used, resets in {}",
                                pct.round() as u32,
                                format_duration(secs)
                            )
                        }
                        None => format!("{}% used", pct.round() as u32),
                    };
                    let title = format!("{} limit", data.id.display_name());
                    if let Err(e) = ToastNotifier::show(&title, &body) {
                        warn!("toast failed: {e}");
                    }
                }
                hooks::run(
                    &self.runtime,
                    &self.config.hooks.on_threshold,
                    hooks::HookEvent::Threshold,
                    data.id.clone(),
                    Some(pct),
                    data.headline_reset(),
                );
            }

            // Reset edge: a provider that was near its limit (>= threshold)
            // dropping sharply means a window rolled over. Self-debouncing:
            // after firing, the tracked % is low, so it won't refire until
            // usage climbs back above the threshold and drops again.
            if let Some(&prev) = self.last_pct.get(&data.id) {
                if prev >= threshold && pct + 30.0 < prev {
                    hooks::run(
                        &self.runtime,
                        &self.config.hooks.on_reset,
                        hooks::HookEvent::Reset,
                        data.id.clone(),
                        Some(pct),
                        data.headline_reset(),
                    );
                }
            }
            self.last_pct.insert(data.id.clone(), pct);

            // Burn-rate trend: record the sample and recompute this
            // provider's forecast (suppressed if the window resets before the
            // projected hit).
            let trend = self.trends.entry(data.id.clone()).or_default();
            trend.push(Utc::now(), pct);
            match trend.forecast(data.headline_reset()) {
                Some(f) if f.hit_at > Utc::now() => {
                    self.forecasts.insert(data.id.clone(), f.hit_at);
                }
                _ => {
                    self.forecasts.remove(&data.id);
                }
            }
        }

        // Startup edge: once, after the first successful fetch.
        if !self.startup_fired && matches!(data.status, ProviderStatus::Ok) {
            self.startup_fired = true;
            hooks::run(
                &self.runtime,
                &self.config.hooks.on_startup,
                hooks::HookEvent::Startup,
                data.id.clone(),
                data.primary_percentage(),
                data.headline_reset(),
            );
        }
    }

    /// Put a fresh reading on screen and into the cache - or, for an error,
    /// keep the last real data on screen.
    fn hold_or_show(&mut self, data: ProviderData) {
        // Remember the failure text (or clear it) for the hover explanation
        // on greyed/estimated rows.
        match &data.status {
            ProviderStatus::NetworkError(msg) | ProviderStatus::AuthError(msg) => {
                self.last_errors.insert(data.id.clone(), msg.clone());
            }
            ProviderStatus::Ok => {
                self.last_errors.remove(&data.id);
            }
            _ => {}
        }

        // A transient error must NOT wipe the last real data — it stays on
        // screen; the renderer greys it out with its age. Applies to every
        // provider.
        let is_error = matches!(
            data.status,
            ProviderStatus::NetworkError(_) | ProviderStatus::AuthError(_)
        );
        prune_provider_cache_map(&mut self.provider_cache);

        let is_success = matches!(data.status, ProviderStatus::Ok);
        let cached_before_update = self.provider_cache.get(&data.id).cloned();
        let display_data = if is_success {
            merge_cached_metrics(data.clone(), cached_before_update.as_ref())
        } else {
            data.clone()
        };
        if is_success {
            self.provider_cache.remove(&data.id);
        }
        if let Some(cached_data) = cacheable_provider_data(&display_data) {
            self.provider_cache
                .insert(cached_data.id.clone(), cached_data);
        }
        // Written only when the file is behind: new numbers, expired ones
        // pruned, or fetch times grown old.
        self.persist_cache();
        let had_real_data = self.display.iter().any(|d| {
            d.id == data.id && matches!(d.status, ProviderStatus::Ok) && !d.metrics.is_empty()
        }) || self.provider_cache.contains_key(&data.id);
        if is_error && had_real_data {
            if let Some(cached) = self.provider_cache.get(&data.id).cloned() {
                if let Some(slot) = self.display.iter_mut().find(|d| d.id == data.id) {
                    if !matches!(slot.status, ProviderStatus::Ok) || slot.metrics.is_empty() {
                        *slot = cached;
                    } else {
                        *slot = merge_cached_metrics(slot.clone(), Some(&cached));
                    }
                }
            }
            warn!(
                "provider {:?}: {:?} — keeping last data on screen",
                data.id, data.status
            );
        } else if let Some(slot) = self.display.iter_mut().find(|d| d.id == data.id) {
            *slot = display_data;
        }
    }

    /// The model labels were read.
    fn on_models(&mut self, generation: u64, read: Models) {
        // A read that began before the request changed (or was withdrawn) is
        // stale: only the newest shows.
        let current = self.model_job.generation();
        if generation == current && read != self.models {
            self.models = read;
            self.relayout_if_reshaped();
            self.window.request_redraw();
            self.refresh_picker();
        }
    }

    // ─── Exit ────────────────────────────────────────────────────

    fn shut_down(&mut self) {
        if let Some(saver) = self.saver_at_exit.take() {
            #[cfg(target_os = "windows")]
            let config = self.config_to_save();
            #[cfg(not(target_os = "windows"))]
            let config = self.config.clone();
            saver.shutdown(config, self.runtime.handle());
        }
        // The cache file catches up on the last fetch times; the alert book
        // has nothing left in flight but is waited for.
        if let Some(writer) = self.cache_writer.take() {
            let last = self
                .cache_ledger
                .at_exit(&self.provider_cache)
                .map(super::cache::FileJob::from);
            writer.shutdown(last, self.runtime.handle());
        }
        if let Some(writer) = self.alert_writer.take() {
            writer.shutdown(None, self.runtime.handle());
        }
    }
}
