// app/indicator.rs — when the taskbar panel hands over to the tray icon,
// and the timers the event loop keeps for the panel and its tooltip.
// Windows only.

use crate::{
    providers::ProviderData,
    ui::{taskbar_panel::TaskbarPanel, theme::ComputedTheme, tray::Tray},
};

/// The panel's and the tooltip's timers and the last verdicts, kept by the
/// event loop between events.
pub(super) struct Indicator {
    /// The previous fallback state, so the panel is re-presented exactly
    /// when the fallback clears. The shell can cover the overlay with no
    /// move/foreground event, so a reorder cue arms a coalesced re-check; at
    /// idle no reorders fire, so CPU stays 0%.
    pub(super) fallback_was_active: bool,
    /// Whether the last evaluation saw a fullscreen app: the panel is hidden
    /// while true; the falling edge re-presents it (alt-tab back to desktop).
    pub(super) fullscreen_was_active: bool,
    pub(super) recheck_at: Option<std::time::Instant>,
    pub(super) bounce_until: Option<std::time::Instant>,
    /// Since when the panel has been out of sight; the tray icon waits it out.
    pub(super) covered_since: Option<std::time::Instant>,
    /// The hover tooltip appears only after the cursor lingers for the system
    /// mouse-hover time (the same delay tray-icon tooltips use), tracked by
    /// this deadline; it is cancelled the moment the cursor leaves.
    pub(super) tooltip_at: Option<std::time::Instant>,
    pub(super) hover_delay: std::time::Duration,
}

impl Indicator {
    pub(super) fn new() -> Self {
        Self {
            fallback_was_active: false,
            fullscreen_was_active: false,
            recheck_at: None,
            bounce_until: None,
            covered_since: None,
            tooltip_at: None,
            hover_delay: std::time::Duration::from_millis(crate::platform::mouse_hover_time_ms()),
        }
    }

    /// The soonest timer, if any: the loop sleeps until then.
    pub(super) fn next_wake(&self) -> Option<std::time::Instant> {
        [self.recheck_at, self.tooltip_at]
            .into_iter()
            .flatten()
            .min()
    }

    /// Look again at `at`, or sooner if a look is already due before it.
    pub(super) fn recheck_by(&mut self, at: std::time::Instant) {
        self.recheck_at = Some(self.recheck_at.map_or(at, |t| t.min(at)));
    }
}

/// How long, and how often, the panel keeps trying to get back over the bar
/// after a fullscreen app lets go. 500ms is generous for a band the shell
/// drops within about 200; 24ms is a frame and a half, which is as often as
/// the compositor can show a difference anyway.
pub(super) const BOUNCE_FOR: std::time::Duration = std::time::Duration::from_millis(500);
pub(super) const BOUNCE_EVERY: std::time::Duration = std::time::Duration::from_millis(24);

/// How long after a move or a re-stack the coverage is looked at again: the
/// bar's own slide settles first.
pub(super) const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// Re-evaluate the Panel indicator's tray fallback: a tray icon stands in
/// while the overlay cannot be seen — under the Start/Search scrim, under the
/// cursor-peek "rude topmost" taskbar, under a fullscreen app, or with no room
/// on the bar at all.
/// No-op outside the Panel modes (every panel method guards on the mode).
///
/// `target` is the display the panel is attached to: the scrim and fullscreen
/// signals are asked about that display in particular, so neither blanks a
/// panel that is plainly visible on another.
#[allow(clippy::too_many_arguments)]
pub(super) fn eval_indicator_fallback(
    panel: &mut TaskbarPanel,
    tray: &mut Tray,
    menu: &muda::Menu,
    providers: &[ProviderData],
    theme: &ComputedTheme,
    fallback_was_active: &mut bool,
    fullscreen_was_active: &mut bool,
    covered_since: &mut Option<std::time::Instant>,
    target: crate::config::schema::PanelDisplay,
) -> FallbackVerdict {
    let scrim = crate::platform::foreground_scrim_active(target, panel.screen_rect());
    let fullscreen = crate::platform::fullscreen_foreground_active(target);
    // While hidden for fullscreen the rect is None, so is_covered reads false —
    // ordering matters: check coverage before this pass may hide the panel.
    let covered = panel.is_covered();
    // `covered` can latch the fallback on forever. Closing the Start menu is the
    // repro: the scrim clears, but the panel has meanwhile lost the z-fight to
    // the taskbar, so `is_covered` still reads true, the fallback stays on, and
    // nothing ever lifts the panel again — it keeps its rectangle (so it is not
    // "hidden") while the bar owns its pixels. Verified live: after Start closed
    // the log read `scrim false, covered true` on every evaluation and the panel
    // never came back.
    //
    // So when nothing explains the obstruction, try lifting the panel once and
    // ask again. This cannot weaken the guarantees that matter: a scrim or a
    // fullscreen app on this display is judged separately above, and during the
    // cursor-edge "rude topmost" peek the compositor keeps the bar on top
    // regardless, so the panel stays covered and the fallback still engages.
    let covered = if covered && !scrim && !fullscreen {
        panel.raise();
        panel.is_covered()
    } else {
        covered
    };
    // A panel that could not place itself at all (no taskbar resolved, or a
    // vertical bar we refuse to draw on) reads as "not covered" — an absent
    // rectangle cannot be obstructed. Without this the user is left with no
    // indicator whatsoever and no hint why.
    let unavailable = panel.is_unavailable();
    let wanted =
        crate::platform::taskbar_geom::should_fall_back(scrim, covered, fullscreen, unavailable);
    // Out of sight, but for how long? A verdict that has not stood for
    // HANDOVER_WAIT is not acted on, and the loop is told to ask again when
    // it has - nothing else would wake it.
    let mut look_again_in = None;
    let fallback = if wanted {
        let since = covered_since.get_or_insert_with(std::time::Instant::now);
        match HANDOVER_WAIT.checked_sub(since.elapsed()) {
            Some(rest) if !*fallback_was_active => {
                look_again_in = Some(rest + std::time::Duration::from_millis(10));
                false
            }
            _ => true,
        }
    } else {
        *covered_since = None;
        false
    };
    // Which of the four inputs decided it. Without this a stuck fallback is
    // indistinguishable from a correct one: the panel is simply absent and the
    // tray icon is simply present, with nothing saying why.
    if fallback != *fallback_was_active {
        tracing::debug!(
            "indicator fallback {} (scrim {}, covered {}, fullscreen {}, unavailable {})",
            if fallback { "ON" } else { "off" },
            scrim,
            covered,
            fullscreen,
            unavailable
        );
    } else {
        tracing::trace!(
            "indicator fallback unchanged: {} (scrim {}, covered {}, fullscreen {}, unavailable {})",
            fallback,
            scrim,
            covered,
            fullscreen,
            unavailable
        );
    }
    // The handover, in that order and never overlapping: the panel goes down
    // first and the icon follows, the icon goes first and the panel follows.
    // Both at once is what a user reads as a swap.
    //
    // Only for the reasons that do not depend on the panel being there.
    // `covered` is not one: hiding it would make `is_covered` read false on
    // the next pass, take the fallback off and show it again. `unavailable`
    // is not one either: the panel is already hidden, and standing it down
    // would stop `reposition` ever learning that room came back.
    //
    // No icon at all for a fullscreen app: the bar is buried then and the
    // icon with it, so it achieves nothing but widening the tray by its own
    // 32 px - and the panel then returns at the wider tray's place and steps
    // sideways as the icon goes.
    let stand_down = fullscreen || (scrim && fallback);
    if stand_down {
        // Idempotent - also re-hides the panel if anything re-presented it
        // since the last evaluation.
        panel.stand_down();
    }
    tray.set_scrim_fallback(fallback && !fullscreen, menu, providers, theme);
    let left_fullscreen = *fullscreen_was_active && !fullscreen;
    if !stand_down && (left_fullscreen || (*fallback_was_active && !fallback)) {
        panel.stand_up(providers);
    }
    *fullscreen_was_active = fullscreen;
    *fallback_was_active = fallback;
    FallbackVerdict {
        left_fullscreen,
        look_again_in,
    }
}

/// What an evaluation leaves for the loop to act on.
pub(super) struct FallbackVerdict {
    /// A fullscreen app just let go: the panel has to fight its way back
    /// over a bar the shell still holds in a raised band.
    pub(super) left_fullscreen: bool,
    /// Ask again after this - a verdict is waiting out `HANDOVER_WAIT` and
    /// no event is owed.
    pub(super) look_again_in: Option<std::time::Duration>,
}

/// How long the panel has to be out of sight before the tray icon stands in
/// for it. Pressing Win and pressing it again puts the shell's bar over the
/// panel for about a second: without this the user watches the panel go and
/// an icon arrive for a blink, which reads as the indicator jumping between
/// two places. The icon is for being without a panel, not for a flicker.
const HANDOVER_WAIT: std::time::Duration = std::time::Duration::from_millis(700);
