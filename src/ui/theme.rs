// ui/theme.rs — theme system: colors and palettes.
//
// Every color is defined here. The renderer takes colors ONLY from this module.

use crate::config::schema::{DetailLevel, Palette, UIConfig};

/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// From a float alpha 0.0–1.0.
    pub fn with_alpha_f(r: u8, g: u8, b: u8, alpha: f32) -> Self {
        Self {
            r,
            g,
            b,
            a: (alpha * 255.0) as u8,
        }
    }

    /// To the tiny-skia format.
    pub fn to_skia_color(&self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

/// Usage level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsageLevel {
    /// 0–59%.
    Low,
    /// 60–79%.
    Mid,
    /// 80–100% (critical).
    High,
}

impl UsageLevel {
    pub fn from_percentage(pct: f32) -> Self {
        if pct < 60.0 {
            Self::Low
        } else if pct < 80.0 {
            Self::Mid
        } else {
            Self::High
        }
    }
}

/// Colors for one usage level.
#[derive(Debug, Clone)]
pub struct LevelColors {
    /// Progress bar color.
    pub bar: Color,
    /// Percentage text color.
    pub text: Color,
}

/// Computed theme for rendering.
#[derive(Debug, Clone)]
pub struct ComputedTheme {
    /// Window background.
    pub bg: Color,
    /// Progress bar track.
    pub bar_track: Color,
    /// Provider name.
    pub provider_name: Color,
    /// Metadata text.
    pub meta: Color,
    /// Level colors.
    pub low: LevelColors,
    pub mid: LevelColors,
    pub high: LevelColors,
    /// Whether the palette is monochrome (greys). The tray icon needs this to
    /// pick a system-theme ink instead of a fixed grey that vanishes on a
    /// light taskbar.
    pub monochrome: bool,
}

/// Palette base RGB + saturation deltas.
struct PaletteSpec {
    low_base: (f32, f32, f32),
    low_delta: (f32, f32, f32),
    mid_base: (f32, f32, f32),
    mid_delta: (f32, f32, f32),
    high_base: (f32, f32, f32),
    high_delta: (f32, f32, f32),
}

fn palette_spec(palette: &Palette) -> PaletteSpec {
    match palette {
        Palette::Default => PaletteSpec {
            low_base: (80.0, 155.0, 110.0),
            low_delta: (20.0, 25.0, 20.0),
            mid_base: (180.0, 145.0, 50.0),
            mid_delta: (30.0, 20.0, 20.0),
            high_base: (185.0, 75.0, 75.0),
            high_delta: (25.0, 15.0, 15.0),
        },
        Palette::Ocean => PaletteSpec {
            low_base: (60.0, 140.0, 180.0),
            low_delta: (30.0, 40.0, 20.0),
            mid_base: (50.0, 100.0, 170.0),
            mid_delta: (20.0, 30.0, 30.0),
            high_base: (20.0, 60.0, 140.0),
            high_delta: (15.0, 20.0, 30.0),
        },
        Palette::Sunset => PaletteSpec {
            low_base: (200.0, 140.0, 60.0),
            low_delta: (30.0, 20.0, 20.0),
            mid_base: (210.0, 80.0, 100.0),
            mid_delta: (20.0, 20.0, 20.0),
            high_base: (140.0, 40.0, 140.0),
            high_delta: (20.0, 15.0, 20.0),
        },
        Palette::Forest => PaletteSpec {
            low_base: (80.0, 160.0, 65.0),
            low_delta: (30.0, 35.0, 20.0),
            mid_base: (175.0, 160.0, 38.0),
            mid_delta: (25.0, 18.0, 20.0),
            high_base: (130.0, 80.0, 28.0),
            high_delta: (18.0, 14.0, 18.0),
        },
        Palette::Neon => PaletteSpec {
            low_base: (30.0, 180.0, 140.0),
            low_delta: (20.0, 55.0, 35.0),
            mid_base: (180.0, 180.0, 18.0),
            mid_delta: (55.0, 55.0, 18.0),
            high_base: (180.0, 18.0, 140.0),
            high_delta: (55.0, 18.0, 35.0),
        },
        Palette::Ice => PaletteSpec {
            low_base: (140.0, 200.0, 220.0),
            low_delta: (20.0, 18.0, 14.0),
            mid_base: (110.0, 160.0, 200.0),
            mid_delta: (18.0, 18.0, 18.0),
            high_base: (80.0, 100.0, 150.0),
            high_delta: (14.0, 14.0, 18.0),
        },
        Palette::Rose => PaletteSpec {
            low_base: (220.0, 150.0, 165.0),
            low_delta: (25.0, 20.0, 20.0),
            mid_base: (200.0, 80.0, 110.0),
            mid_delta: (20.0, 20.0, 18.0),
            high_base: (140.0, 15.0, 55.0),
            high_delta: (18.0, 12.0, 18.0),
        },
        Palette::Slate => PaletteSpec {
            low_base: (120.0, 155.0, 185.0),
            low_delta: (25.0, 22.0, 20.0),
            mid_base: (80.0, 110.0, 145.0),
            mid_delta: (18.0, 18.0, 20.0),
            high_base: (42.0, 65.0, 95.0),
            high_delta: (15.0, 15.0, 18.0),
        },
    }
}

/// Level color with conventional saturation: a lerp between greyscale
/// (Rec. 709 luminance) and the full palette color (base + delta).
/// 0% = greyscale, 100% = the full color.
fn level_color(base: (f32, f32, f32), delta: (f32, f32, f32), sat: f32) -> Color {
    let (r, g, b) = (base.0 + delta.0, base.1 + delta.1, base.2 + delta.2);
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let mix = |v: f32| (lum + (v - lum) * sat).clamp(0.0, 255.0) as u8;
    Color {
        r: mix(r),
        g: mix(g),
        b: mix(b),
        a: (0.82 * 255.0) as u8,
    }
}

/// Level text color: a brighter variant of the bar for readability.
fn level_text_color(bar: Color) -> Color {
    let lift = |v: u8| ((v as f32 * 1.12).clamp(0.0, 255.0)) as u8;
    Color {
        r: lift(bar.r),
        g: lift(bar.g),
        b: lift(bar.b),
        a: (0.88 * 255.0) as u8,
    }
}

impl ComputedTheme {
    /// Compute the theme from the config.
    pub fn compute(config: &UIConfig) -> Self {
        // Base colors — a single dark theme. The background alpha is set
        // by the renderer from window.opacity; full here.
        let bg = Color::rgba(14, 14, 14, 255);
        let bar_track = Color::with_alpha_f(255, 255, 255, 0.008);

        let text = |alpha: f32| Color::with_alpha_f(255, 255, 255, alpha);

        // Brightness 20–100% scales the text and bar alphas.
        let bright = (config.brightness.clamp(20, 100) as f32) / 100.0;
        let dim = |c: Color| Color {
            a: ((c.a as f32) * bright).clamp(0.0, 255.0) as u8,
            ..c
        };

        // Level colors: monochrome, or a palette scaled by saturation.
        let (low, mid, high) = if config.monochrome {
            (
                LevelColors {
                    bar: Color::with_alpha_f(220, 220, 220, 0.9),
                    text: text(0.60),
                },
                LevelColors {
                    bar: Color::with_alpha_f(195, 195, 195, 0.9),
                    text: text(0.45),
                },
                LevelColors {
                    bar: Color::with_alpha_f(170, 170, 170, 0.9),
                    text: text(0.35),
                },
            )
        } else {
            let spec = palette_spec(&config.palette);
            let sat = (config.saturation.min(100) as f32) / 100.0;
            let low_bar = level_color(spec.low_base, spec.low_delta, sat);
            let mid_bar = level_color(spec.mid_base, spec.mid_delta, sat);
            let high_bar = level_color(spec.high_base, spec.high_delta, sat);
            (
                LevelColors {
                    bar: low_bar,
                    text: level_text_color(low_bar),
                },
                LevelColors {
                    bar: mid_bar,
                    text: level_text_color(mid_bar),
                },
                LevelColors {
                    bar: high_bar,
                    text: level_text_color(high_bar),
                },
            )
        };

        // Apply brightness to text and bars.
        let dim_level = |lc: LevelColors| LevelColors {
            bar: dim(lc.bar),
            text: dim(lc.text),
        };

        Self {
            bg,
            bar_track,
            provider_name: dim(text(0.42)),
            meta: dim(text(0.22)),
            low: dim_level(low),
            mid: dim_level(mid),
            high: dim_level(high),
            monochrome: config.monochrome,
        }
    }

    /// Colors for a usage level.
    pub fn level(&self, level: UsageLevel) -> &LevelColors {
        match level {
            UsageLevel::Low => &self.low,
            UsageLevel::Mid => &self.mid,
            UsageLevel::High => &self.high,
        }
    }
}

/// UI element dimensions.
pub struct Dimensions;

impl Dimensions {
    /// Browser mockup density=19 maps to a 1.11 scale.
    pub const DENSITY_SCALE: f32 = 1.11;

    pub fn scale(value: f32) -> f32 {
        value * Self::DENSITY_SCALE
    }

    /// Provider row height.
    pub fn row_height(detail: &DetailLevel) -> f32 {
        Self::scale(match detail {
            DetailLevel::Compact => 20.0,
            DetailLevel::Medium => 40.0,
            // Expanded adds the second (weekly) metric line under the bar.
            DetailLevel::Expanded => 52.0,
        })
    }

    /// Horizontal padding.
    pub const PADDING_H: f32 = 11.1;
}

// ─── Picker ──────────────────────────────────────────────────────

/// Colours of the "Rows and models" window (ui/picker.rs). It follows
/// Windows' apps mode, light or dark, like the app's other windows, and High
/// contrast's own colours when that is on; the light and dark values are the
/// workshop mock's (design/workshop/countdown/page.html, `.rpk`). Separate
/// from `ComputedTheme`: the widget keeps its one dark look.
#[derive(Debug, Clone)]
pub struct PickerTheme {
    /// The box.
    pub surface: Color,
    /// Hairlines: the box, buttons, fields.
    pub border: Color,
    pub text: Color,
    /// Notes, hints, column heads, an unticked box's outline.
    pub muted: Color,
    pub accent: Color,
    /// Text and marks on the accent.
    pub on_accent: Color,
    /// The wash of a hovered row, a field, a secondary button.
    pub hover: Color,
    pub separator: Color,
    /// The shadow's colour; the rings below set its alpha.
    pub shadow_color: Color,
    /// Shadow rings around a box, outermost first: (spread px, alpha). The
    /// alphas compound, as in the tray tooltip's shadow.
    pub shadow: [(f32, u8); 6],
    /// How far the shadow falls below the box.
    pub shadow_drop: f32,
    /// How strongly each state lays an ink on (`picker::Level`).
    pub levels: PickerLevels,
}

/// Factors on an ink's own alpha, one per `picker::Level` besides `Full`.
/// A factor above 1 deepens a translucent wash (a hovered button).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickerLevels {
    pub field_hover: f32,
    pub hovered: f32,
    pub pressed: f32,
    pub accent_hover: f32,
    pub accent_pressed: f32,
    pub mark_pressed: f32,
    pub disabled: f32,
    pub disabled_mark: f32,
    /// A field's automatic name: at least 4.5:1 against the field.
    pub placeholder: f32,
    /// A scroll bar's thumb: at least 3:1 against the box.
    pub thumb: f32,
}

impl PickerLevels {
    fn standard(placeholder: f32, thumb: f32) -> Self {
        Self {
            field_hover: 1.6,
            hovered: 2.0,
            pressed: 3.0,
            accent_hover: 0.9,
            accent_pressed: 0.8,
            mark_pressed: 0.8,
            disabled: 0.4,
            disabled_mark: 0.35,
            placeholder,
            thumb,
        }
    }
}

/// The colours High contrast sets (Windows' system colours), as RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContrastColors {
    pub window: [u8; 3],
    pub window_text: [u8; 3],
    pub highlight: [u8; 3],
    pub highlight_text: [u8; 3],
    pub gray_text: [u8; 3],
}

impl Default for ContrastColors {
    /// Windows' black contrast scheme, for a colour it does not give.
    fn default() -> Self {
        Self {
            window: [0, 0, 0],
            window_text: [255, 255, 255],
            highlight: [26, 235, 255],
            highlight_text: [0, 0, 0],
            gray_text: [63, 242, 63],
        }
    }
}

impl PickerTheme {
    pub fn dark() -> Self {
        Self {
            surface: Color::rgba(44, 44, 44, 255),
            border: Color::rgba(62, 62, 62, 255),
            text: Color::rgba(255, 255, 255, 255),
            muted: Color::rgba(197, 197, 197, 255),
            accent: Color::rgba(76, 194, 255, 255),
            on_accent: Color::rgba(0, 0, 0, 255),
            hover: Color::rgba(255, 255, 255, 15),
            separator: Color::rgba(62, 62, 62, 255),
            shadow_color: Color::rgba(0, 0, 0, 255),
            shadow: [
                (10.0, 3),
                (8.0, 5),
                (6.0, 7),
                (4.0, 10),
                (2.0, 14),
                (1.0, 18),
            ],
            shadow_drop: 3.0,
            levels: PickerLevels::standard(0.78, 0.55),
        }
    }

    pub fn light() -> Self {
        Self {
            surface: Color::rgba(249, 249, 249, 255),
            border: Color::rgba(217, 217, 217, 255),
            text: Color::rgba(27, 27, 27, 255),
            muted: Color::rgba(95, 95, 95, 255),
            accent: Color::rgba(0, 95, 184, 255),
            on_accent: Color::rgba(255, 255, 255, 255),
            hover: Color::rgba(0, 0, 0, 11),
            separator: Color::rgba(225, 225, 225, 255),
            shadow_color: Color::rgba(0, 0, 0, 255),
            shadow: [(10.0, 1), (8.0, 2), (6.0, 3), (4.0, 4), (2.0, 6), (1.0, 8)],
            shadow_drop: 3.0,
            levels: PickerLevels::standard(0.92, 0.75),
        }
    }

    /// High contrast: its own colours, solid. Text and notes alike in the
    /// text colour, every line in it too, the accent its highlight; no
    /// shadow, nothing faded but what cannot be used.
    pub fn high_contrast(c: &ContrastColors) -> Self {
        let rgb = |[r, g, b]: [u8; 3], a: u8| Color::rgba(r, g, b, a);
        Self {
            surface: rgb(c.window, 255),
            border: rgb(c.window_text, 255),
            text: rgb(c.window_text, 255),
            muted: rgb(c.window_text, 255),
            accent: rgb(c.highlight, 255),
            on_accent: rgb(c.highlight_text, 255),
            hover: rgb(c.highlight, 48),
            separator: rgb(c.gray_text, 255),
            shadow_color: rgb(c.window_text, 255),
            shadow: [(10.0, 0), (8.0, 0), (6.0, 0), (4.0, 0), (2.0, 0), (1.0, 0)],
            shadow_drop: 0.0,
            levels: PickerLevels {
                disabled: 0.6,
                disabled_mark: 0.6,
                ..PickerLevels::standard(1.0, 1.0)
            },
        }
    }

    /// The theme for Windows' apps mode.
    pub fn for_system(light: bool) -> Self {
        if light {
            Self::light()
        } else {
            Self::dark()
        }
    }
}

// ─── Tray icon, its tooltip, the taskbar panel ───────────────────
//
// These follow the taskbar's own light or dark theme
// (`platform::system_uses_light_theme`), not the widget's palette; the values
// were measured against the shell's own clock and tooltips.

/// The tray icon's ink: near-black on a light taskbar, near-white on a dark
/// one (the panel's ink's sibling), so it reads on either. `a` is the alpha:
/// 255 for a solid fill or digit, about 64 for the faint track ring, about
/// 200 for an inactive dot.
pub fn tray_ink(light: bool, a: u8) -> Color {
    if light {
        Color::rgba(20, 20, 20, a)
    } else {
        Color::rgba(236, 236, 236, a)
    }
}

/// The tray tooltip's colours on a light or a dark taskbar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipColors {
    /// Shadow rings, outermost first: (step px, alpha), in `shadow_color`.
    /// The alphas COMPOUND: each ring is painted over the previous one, so
    /// the cumulative darkening at distance d is 1-prod(1-a). Fitted to the
    /// measured falloff (-3,-4,-7,-10 luma at 5..2px out over grey 128) rather
    /// than by eye; a first pass at 6/8/10/14 compounded to -15..-31, three
    /// times too heavy. The light theme casts a much softer shadow: measured
    /// -2/-4/-6 at 4..2px out against the dark theme's -4/-7/-10.
    pub shadow: [(f32, u8); 5],
    pub shadow_color: Color,
    /// The light tooltip's 1px hairline under its body (a near-white body
    /// needs one); the dark one has none: scanning inward from the edge of
    /// the measured tooltip over black gives 45,44,45,45, flat body from the
    /// first pixel.
    pub hairline: Option<Color>,
    /// The body. Dark: grey 46 at 246, which reproduces the measured
    /// tooltip over black (49) now that the box is punched out of its
    /// shadow. Light: 249 at 219, markedly more translucent: measured over
    /// grey 128 it reads 232, over white 249. Windows' light acrylic is not a
    /// plain alpha blend, so this is fitted to the two backdrops that occur
    /// under a tooltip, a light taskbar and light windows.
    pub body: Color,
    pub text: Color,
}

/// The tray tooltip's colours for the taskbar's theme.
pub fn tip_colors(light: bool) -> TipColors {
    if light {
        TipColors {
            shadow: [(5.0, 1), (4.0, 2), (3.0, 3), (2.0, 4), (1.0, 6)],
            shadow_color: Color::rgba(0, 0, 0, 255),
            hairline: Some(Color::rgba(0, 0, 0, 36)),
            body: Color::rgba(249, 249, 249, 219),
            text: Color::rgba(26, 26, 26, 255),
        }
    } else {
        TipColors {
            shadow: [(5.0, 4), (4.0, 4), (3.0, 5), (2.0, 6), (1.0, 8)],
            shadow_color: Color::rgba(0, 0, 0, 255),
            hairline: None,
            body: Color::rgba(46, 46, 46, 246),
            text: Color::rgba(255, 255, 255, 255),
        }
    }
}

/// The taskbar panel's ink and bar track: near-black on a light taskbar,
/// near-white on a dark one; the track is the ink at a low alpha so it reads
/// as a faint groove rather than a filled box.
///
/// The light ink is 36, not the 20 it used to be. Measured against the
/// shell's own clock on the same bar: our digits had a median ink luma of 110
/// where the clock's was 119, over a background of 228 — we were ~8% darker,
/// and read as heavier and higher-contrast than everything else in the tray.
/// Solving `median = ink*cov + bg*(1-cov)` at the measured coverage of 0.567
/// gives 36. This is a CONTRAST correction, not a weight one: fontdue
/// rasterises with crisper edges than DirectWrite, so matching the shell's
/// darkness is the closest we get without swapping the rasteriser.
pub fn panel_ink(light: bool) -> (Color, Color) {
    if light {
        (Color::rgba(36, 36, 36, 255), Color::rgba(36, 36, 36, 64))
    } else {
        (
            Color::rgba(236, 236, 236, 255),
            Color::rgba(236, 236, 236, 66),
        )
    }
}

/// The panel's otherwise transparent background: just enough alpha that
/// every pixel catches the mouse (a layered window passes clicks through a
/// fully transparent pixel to the taskbar beneath), yet imperceptible over
/// the taskbar.
pub const PANEL_HIT: Color = Color::rgba(0, 0, 0, 1);
