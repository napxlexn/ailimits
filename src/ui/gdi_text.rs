// ui/gdi_text.rs — the Settings window's text as Windows itself draws it
// (GDI: the font's own hinting, Windows' smoothing), for crisp text at
// every scale. fontdue draws without hinting, so at 11 to 13 px its stems
// spread over two pixels and the text reads soft; the widget keeps fontdue
// (its pinned look), the picker uses this.
//
// A run of text is drawn white on black into a small DIB with a font of the
// size asked, and what lands there is its coverage: one value a pixel for
// grayscale smoothing, three (red, green, blue) for ClearType. The picker
// blends its own colour through it, over whatever is already under the text.
// Imports: CreateFontIndirectW is the one GDI call the app had not made;
// DrawTextW, CreateDIBSection, SelectObject and the rest it imports already.

use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontIndirectW, DeleteDC, DeleteObject, DrawTextW,
    SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    DT_CALCRECT, DT_NOCLIP, DT_NOPREFIX, DT_SINGLELINE, HDC, HFONT, HGDIOBJ, LOGFONTW, TRANSPARENT,
};

/// How the text is smoothed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Smoothing {
    /// Grayscale, as Windows 11's own apps draw their text.
    Gray,
    /// ClearType: each colour channel its own coverage.
    ClearType,
}

/// A run of text drawn white on black: its coverage, `w` by `h` pixels,
/// three bytes a pixel (equal for grayscale), its top at the font's cell top.
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

/// Fonts and drawn runs, kept while the window lives.
pub struct GdiText {
    dc: HDC,
    fonts: RefCell<HashMap<(i32, bool, bool, Smoothing), HFONT>>,
}

impl GdiText {
    pub fn new() -> Option<Self> {
        let dc = unsafe { CreateCompatibleDC(None) };
        (!dc.is_invalid()).then(|| Self {
            dc,
            fonts: RefCell::new(HashMap::new()),
        })
    }

    /// Segoe UI at `px` pixels a em (the cell's height is about 1.33 em),
    /// semibold for `bold`.
    fn font(&self, px: i32, bold: bool, italic: bool, smoothing: Smoothing) -> HFONT {
        *self
            .fonts
            .borrow_mut()
            .entry((px, bold, italic, smoothing))
            .or_insert_with(|| {
                let mut lf = LOGFONTW {
                    lfHeight: -px.max(1),
                    lfWeight: if bold { 600 } else { 400 },
                    lfItalic: italic as u8,
                    lfQuality: windows::Win32::Graphics::Gdi::FONT_QUALITY(match smoothing {
                        Smoothing::Gray => 4,      // ANTIALIASED_QUALITY
                        Smoothing::ClearType => 5, // CLEARTYPE_QUALITY
                    }),
                    ..Default::default()
                };
                for (i, c) in "Segoe UI".encode_utf16().enumerate() {
                    lf.lfFaceName[i] = c;
                }
                unsafe { CreateFontIndirectW(&lf) }
            })
    }

    fn rect_of(&self, text: &[u16], font: HFONT) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let old = SelectObject(self.dc, HGDIOBJ(font.0));
            let mut t = text.to_vec();
            DrawTextW(
                self.dc,
                &mut t,
                &mut r,
                DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
            );
            SelectObject(self.dc, old);
        }
        r
    }

    /// How wide `text` is at `px`, in pixels.
    pub fn width(&self, text: &str, px: f32, bold: bool) -> f32 {
        let w: Vec<u16> = text.encode_utf16().collect();
        if w.is_empty() {
            return 0.0;
        }
        let r = self.rect_of(
            &w,
            self.font(px.round() as i32, bold, false, Smoothing::Gray),
        );
        (r.right - r.left) as f32
    }

    /// `text` at `px` drawn white on black: its coverage.
    pub fn mask(
        &self,
        text: &str,
        px: f32,
        bold: bool,
        italic: bool,
        smoothing: Smoothing,
    ) -> Option<Mask> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return None;
        }
        let font = self.font(px.round() as i32, bold, italic, smoothing);
        let r = self.rect_of(&wide, font);
        // Room for an italic's overhang on the right.
        let w = (r.right - r.left) as usize + if italic { (px * 0.3) as usize + 2 } else { 2 };
        let h = (r.bottom - r.top).max(1) as usize;
        unsafe {
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let dib =
                CreateDIBSection(Some(self.dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let old_bmp = SelectObject(self.dc, HGDIOBJ(dib.0));
            let old_font = SelectObject(self.dc, HGDIOBJ(font.0));
            SetBkMode(self.dc, TRANSPARENT);
            SetTextColor(self.dc, COLORREF(0x00FF_FFFF));
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: w as i32,
                bottom: h as i32,
            };
            let mut t = wide.clone();
            DrawTextW(
                self.dc,
                &mut t,
                &mut rect,
                DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP,
            );
            let px_bytes = std::slice::from_raw_parts(bits as *const u8, w * h * 4);
            let mut rgb = Vec::with_capacity(w * h * 3);
            for p in px_bytes.as_chunks::<4>().0 {
                // BGRA in the DIB.
                rgb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            SelectObject(self.dc, old_font);
            SelectObject(self.dc, old_bmp);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            Some(Mask { w, h, rgb })
        }
    }
}

impl Drop for GdiText {
    fn drop(&mut self) {
        unsafe {
            for f in self.fonts.borrow().values() {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
            let _ = DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows draws and measures the Settings window's text: a run has
    /// ink, a semibold one is no narrower, grayscale is the same in every
    /// channel.
    #[test]
    fn windows_draws_and_measures_the_text() {
        let g = GdiText::new().expect("a memory DC");
        let regular = g.width("Claude Code", 12.0, false);
        let bold = g.width("Claude Code", 12.0, true);
        assert!((40.0..120.0).contains(&regular), "{regular}");
        assert!(bold >= regular, "{bold} < {regular}");
        assert_eq!(g.width("", 12.0, false), 0.0);
        for smoothing in [Smoothing::Gray, Smoothing::ClearType] {
            let m = g
                .mask("Claude Code", 12.0, false, false, smoothing)
                .expect("a run");
            assert!(m.w >= regular as usize && m.h >= 12, "{}x{}", m.w, m.h);
            assert!(m.rgb.iter().any(|&c| c > 200), "no ink");
            if smoothing == Smoothing::Gray {
                assert!(m.rgb.chunks(3).all(|p| p[0] == p[1] && p[1] == p[2]));
            }
        }
        assert!(g.mask("", 12.0, false, false, Smoothing::Gray).is_none());
    }
}
