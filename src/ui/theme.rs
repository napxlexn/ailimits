// ui/theme.rs — theme system: colors and palettes.
//
// Every color is defined here. The renderer takes colors ONLY from this module.

use crate::config::schema::{DetailLevel, Palette, UIConfig};

/// RGBA color.
#[derive(Debug, Clone, Copy)]
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
    // Rec. 709 luma — perceptual luminance.
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
