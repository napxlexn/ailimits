// app/menu_preview.rs — the context menu's look items shown while the
// pointer rests on them: the widget, the panel and the tray icon take the
// palette, opacity, layout... at once, before a click, and go back when the
// menu closes without one. Windows only (platform/menu_hover.rs tells what
// is highlighted).
//
// Nothing is saved and the menu's ticks do not move while it is open. A
// click applies the item as it always did (app/menu.rs, the same
// `context_menu::look`), on the look from before the menu, and the screen,
// already showing it, does not flicker.

use super::{widget::apply_ui_layout, widget_shape, App};
use crate::config::schema::{Config, UIConfig};
use crate::platform::menu_hover::MenuHover;
use crate::ui::context_menu::{look, MenuAction};
use tao::dpi::PhysicalPosition;

/// What a look item changes: the UI section and the background opacity.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::app) struct Look {
    ui: UIConfig,
    opacity: f32,
}

impl Look {
    fn of(config: &Config) -> Self {
        Self {
            ui: config.ui.clone(),
            opacity: config.window.opacity,
        }
    }

    fn put(&self, config: &mut Config) {
        config.ui = self.ui.clone();
        config.window.opacity = self.opacity;
    }
}

/// An open menu's preview.
pub(in crate::app) struct MenuPreview {
    /// The look before the menu opened.
    base: Look,
    /// Where the widget stood: a larger look shown may pull it inside the
    /// work area, and each look is placed from here.
    base_pos: Option<(i32, i32)>,
    /// The look item shown, if any.
    shown: Option<MenuAction>,
}

impl App {
    /// What the menu is doing (platform::menu_hover).
    pub(super) fn on_menu_hover(&mut self, ev: MenuHover) {
        match ev {
            MenuHover::Begin => self.begin_look(),
            MenuHover::Item { group, text } => {
                let action = self.menu.look_action(group.as_deref(), &text);
                self.show_look(action);
            }
            MenuHover::Nothing => self.show_look(None),
            MenuHover::End { chosen } => self.end_look(chosen),
        }
    }

    fn begin_look(&mut self) {
        if self.menu_preview.is_some() {
            return;
        }
        // A click of the last menu that never arrived: its look goes back.
        self.take_look_base();
        self.menu_preview = Some(MenuPreview {
            base: Look::of(&self.config),
            base_pos: self.window.outer_position().ok().map(|p| (p.x, p.y)),
            shown: None,
        });
    }

    /// Show `action`'s look over the one from before the menu (None: that
    /// one again).
    fn show_look(&mut self, action: Option<MenuAction>) {
        let Some(p) = self.menu_preview.as_mut() else {
            return;
        };
        if p.shown == action {
            return;
        }
        p.shown = action.clone();
        let (base, pos) = (p.base.clone(), p.base_pos);
        base.put(&mut self.config);
        if let Some(a) = &action {
            look(&mut self.config, a);
        }
        self.paint_look(pos);
    }

    /// The menu closed. Clicked: the look shown stays on screen and the
    /// click (on its way as a MenuEvent) applies and saves it, on the look
    /// from before the menu. Not clicked: that look comes back.
    fn end_look(&mut self, chosen: bool) {
        let Some(p) = self.menu_preview.take() else {
            return;
        };
        if p.shown.is_none() {
            return;
        }
        if chosen {
            self.look_base = Some(p.base);
        } else {
            p.base.put(&mut self.config);
            self.paint_look(p.base_pos);
        }
    }

    /// Before a menu click is applied: the look from before the menu, if a
    /// preview left another on screen for it. A click that arrives before
    /// the menu's end ends the preview here (the end then finds nothing to
    /// do), so a toggle never applies to its own preview.
    pub(super) fn take_look_base(&mut self) {
        if let Some(p) = self.menu_preview.take() {
            p.base.put(&mut self.config);
        }
        if let Some(base) = self.look_base.take() {
            base.put(&mut self.config);
        }
    }

    /// The config as it is to be saved: while a menu shows a look, with the
    /// look from before the menu, so a preview never reaches the file.
    pub(super) fn config_to_save(&self) -> Config {
        let mut config = self.config.clone();
        if let Some(p) = &self.menu_preview {
            p.base.put(&mut config);
        }
        config
    }

    /// Paint the widget, the panel and the tray icon with the config's look
    /// now: the menu's modal loop holds every queued repaint until it closes.
    /// The widget is placed from `pos`, then held inside its work area.
    fn paint_look(&mut self, pos: Option<(i32, i32)>) {
        if let Some((x, y)) = pos {
            if self.window.outer_position().ok().map(|p| (p.x, p.y)) != Some((x, y)) {
                self.window.set_outer_position(PhysicalPosition::new(x, y));
                self.win_state.pos = (x as f64, y as f64);
            }
        }
        self.shape = widget_shape(&self.config, &self.display, &self.models, &self.renderer);
        apply_ui_layout(
            &self.config,
            &mut self.win_layout,
            &mut self.theme,
            &mut self.pixmap,
            &mut self.surface,
            &self.window,
            &mut self.win_state,
            &self.shape,
        );
        self.redraw_widget();
        let visible = self.visible();
        self.tray.update(&visible, &self.theme, true);
        self.panel.update(&visible, true);
    }
}
