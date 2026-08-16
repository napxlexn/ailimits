"""Measure the real Windows tray tooltip, so our own tooltip can copy it.

Why this exists: the tooltip metrics were first written from memory and every
one of them was a little wrong — padding too small, the body too transparent,
the text too dim, and a border that the shell does not draw at all. Guessing
does not converge. This measures.

Method: put a flat backdrop behind the tray area, hover a system tray icon
until the shell's own tooltip appears, and capture it — once over black, once
over white. Two backdrops are the point: the tooltip is slightly translucent,
so the pair gives both the true body colour and its alpha:

    on black:  C*a                     -> 44
    on white:  C*a + 255*(1-a)         -> 54
    hence      a = 1 - (54-44)/255 = 0.96,  C = 44/0.96 = 46

Requirements: Chrome (for the backdrop), Pillow, a bottom taskbar on the
primary display. Re-run it after a Windows feature update; shell metrics move.

    python tools/measure-shell-tooltip.py

Measured 2026-08-16 on Win11 26200, dark theme, 100% scaling, 48px bar:

    box height 30      padding 10 left/right, 10 top, 8 bottom
    body rgb(46,46,46) at alpha ~244        text pure white
    corner radius 2-3  no border in dark mode, but a soft outer shadow
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import tempfile
import time

try:
    from PIL import Image, ImageChops, ImageGrab
except ImportError:
    sys.exit("needs Pillow:  pip install pillow")

CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
TMP = os.path.join(tempfile.gettempdir(), "ailimits-tipmeasure")

u = ctypes.windll.user32
u.SetProcessDPIAware()

MOUSEEVENTF_MOVE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_VIRTUALDESK = 0x0001, 0x8000, 0x4000


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD),
                ("dwFlags", wt.DWORD), ("time", wt.DWORD),
                ("dwExtraInfo", ctypes.POINTER(wt.ULONG))]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("mi", MOUSEINPUT)]
    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", _U)]


VS = [u.GetSystemMetrics(m) for m in (76, 77, 78, 79)]


def move_to(x, y):
    """Absolute move as a REAL input event.

    SetCursorPos is not enough: the Win11 tray is XAML and its hover timer
    ignores a teleported cursor, so the tooltip never appears.
    """
    ax = int((x - VS[0]) * 65535 / (VS[2] - 1))
    ay = int((y - VS[1]) * 65535 / (VS[3] - 1))
    inp = INPUT(type=0)
    inp.mi = MOUSEINPUT(ax, ay, 0,
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                        0, None)
    u.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def backdrop(colour, rect):
    os.makedirs(TMP, exist_ok=True)
    page = os.path.join(TMP, f"bg-{colour}.html")
    with open(page, "w", encoding="utf-8") as f:
        f.write("<!doctype html><meta charset=utf-8>"
                f"<style>html,body{{margin:0;height:100%;background:{colour}}}</style>")
    return subprocess.Popen([
        CHROME, f"--app=file:///{page.replace(os.sep, '/')}",
        "--no-first-run", "--no-default-browser-check",
        f"--user-data-dir={os.path.join(TMP, 'profile')}",
        f"--window-position={rect[0]},{rect[1]}",
        f"--window-size={rect[2]},{rect[3]}",
    ])


def capture(colour, icon, grab, win_rect):
    proc = backdrop(colour, win_rect)
    time.sleep(6)
    move_to(icon[0] - 120, grab[3] - 2)      # slide an auto-hidden bar out
    time.sleep(1.0)
    for i in range(1, 20):                    # walk in so the hover timer starts
        move_to(int(icon[0] - 120 + 120 * i / 19), icon[1])
        time.sleep(0.02)
    base = ImageGrab.grab(bbox=grab, all_screens=True).convert("RGB")
    best, best_score = base, 0
    for i in range(16):
        move_to(icon[0] + (i % 2), icon[1])
        time.sleep(0.45)
        frame = ImageGrab.grab(bbox=grab, all_screens=True).convert("RGB")
        upper = (0, 0, frame.width, frame.height - 60)
        d = ImageChops.difference(frame.crop(upper), base.crop(upper)).convert("L")
        score = sum(d.point(lambda v: 255 if v > 24 else 0).getdata()) // 255
        if score > best_score:
            best, best_score = frame, score
    proc.terminate()
    return best, best_score


def lum(p):
    return 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]


def analyse(img, backdrop_is_dark):
    """Box bounds, body colour, text colour and padding, in image coordinates."""
    px = img.load()
    w, h = img.size

    def off_backdrop(p):
        return lum(p) > 22 if backdrop_is_dark else lum(p) < 232

    # The tooltip is the blob above the taskbar; find its rows first.
    band = [y for y in range(0, h - 62)
            if sum(1 for x in range(0, w) if off_backdrop(px[x, y])) > w // 8]
    if not band:
        return None
    top, bot = band[0], band[-1]
    mid = (top + bot) // 2
    left = next(x for x in range(0, w) if off_backdrop(px[x, mid]))
    body = px[left + 8, top + 2]

    def inky(x, y):
        return sum(abs(a - b) for a, b in zip(px[x, y], body)) > 70

    rows = [y for y in range(top, bot + 1)
            if sum(1 for x in range(left + 6, w - 20) if inky(x, y)) > 2]
    cols = [x for x in range(left + 1, w) if any(inky(x, y) for y in range(top + 1, bot))]
    extreme = max((px[x, y] for x in range(left + 6, w - 20) for y in range(top, bot + 1)),
                  key=lambda p: abs(lum(p) - lum(body)))
    return {
        "height": bot - top + 1, "body": body, "text": extreme,
        "pad_top": rows[0] - top if rows else None,
        "pad_bottom": bot - rows[-1] if rows else None,
        "pad_left": cols[0] - left if cols else None,
        "text_ink_height": rows[-1] - rows[0] + 1 if rows else None,
    }


def main():
    # Defaults suit a 3440-wide primary with a bottom bar; override as needed.
    icon = (int(os.environ.get("TIP_ICON_X", 3315)), int(os.environ.get("TIP_ICON_Y", 1416)))
    grab = (icon[0] - 715, icon[1] - 136, icon[0] + 125, icon[1] + 29)
    win_rect = (grab[0] - 400, grab[1] - 580, 1240, 745)

    results = {}
    # Grey is the one that shows the SHADOW. Over black a dark shadow is
    # invisible; over white it blends into the same falloff as the body edge.
    # Over mid grey both the shadow and any border stand out.
    for colour, dark in (("black", True), ("white", False), ("#808080", False)):
        img, score = capture(colour, icon, grab, win_rect)
        out = os.path.join(TMP, f"sys-tip-{colour.strip('#')}.png")
        img.save(out)
        if score < 500:
            print(f"{colour}: no tooltip appeared (changed pixels {score}). "
                  f"Check TIP_ICON_X/TIP_ICON_Y; capture left at {out}")
            continue
        results[colour] = analyse(img, dark)
        print(f"\n=== over {colour} ===  ({out})")
        for k, v in results[colour].items():
            print(f"  {k:16} {v}")

    if "black" in results and "white" in results:
        cb, cw = results["black"]["body"][0], results["white"]["body"][0]
        alpha = 1 - (cw - cb) / 255
        print("\n=== derived ===")
        print(f"  body alpha       {alpha:.3f}  ({round(alpha*255)}/255)")
        print(f"  body true colour {round(cb/alpha)} grey")


if __name__ == "__main__":
    main()
