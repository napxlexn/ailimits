# TUNING.md — fine-tuning in config.toml

The context menu and *Settings* cover what most people change. For
finer control over the widget, the taskbar panel and the tray icon (sizes,
spacing, colours, the usage at which a colour changes, how times read) the
config file has four tables of its own: `[tune.widget]`, `[tune.panel]`,
`[tune.tray]` and `[tune.format]`. They live only in the file: nothing in
the menu or in the window sets them.

Every key is optional. A key left out takes its default, and the defaults
are exactly the look the app has without them. A value that does not fit
costs only that key, never the rest of the file (see [Notes](#notes)).

## The file

`%APPDATA%\AiLimits\config.toml`. The rest of what it holds is described in
[CONFIG.md](CONFIG.md).

### Opening it

Right-click the widget, the tray icon or the taskbar panel, choose *Settings…*,
and click *Edit config.toml…* at the end of its *Display* tab. The file
opens in Notepad (`%SystemRoot%\System32\notepad.exe`). Any other editor
works too.

The four `[tune]` tables are always at the end of the file, as the app
writes it: the keys set as they are, every other key commented out with its
default and a note on its range:

```toml
[tune.widget]
# text_scale = 1.0        # 0.8 to 1.5: size of all text (factor)
```

To use a key, delete the `# ` before it, change the value and save. A table
may be in the file only once: put the keys under the table that is there,
never add a second `[tune.widget]` (the second one would be left out, with
every key under it, and a notification would name its lines). A file
the app has not saved since it was updated may not have the tables yet: add
the one you need at the end of the file then.

The rest of the file is explained the same way: a short note before each
table and each setting says what it does, the values it takes, its default
and the menu item that sets the same. Comments you add are not kept: the
app writes its own notes whenever it saves the file.

### When an edit applies

Save the file (Ctrl+S in Notepad); no restart is needed. The app looks at
the file's size and time (nothing is read until they change):

- every 2 seconds while you are at the PC (any input in the last 5
  minutes), so a saved edit shows within about 2 seconds;
- every 30 seconds while you are away, except for 30 minutes after
  *Edit config.toml…* opened it: then every 2 seconds, away or not (click
  the link again for another 30 minutes);
- besides, once per update cycle, right before it carries out a click in
  the context menu or in *Settings*, and when it quits.

Only what changed is redone: a `[tune]` edit redraws the widget, the panel
and the tray icon, and nothing is fetched again.

The app's own saves (a menu click, a move of the widget) are not taken for
edits. The app writes the file only when it has a change of its own, and
that save writes the whole file anew with its own notes: your `[tune]` keys
stay, comments you added go (see [Notes](#notes) for what else is left
out). A change you make in the
app while your edit is on its way is kept where the edit left that setting
alone; where both changed the same setting, the file wins. The details are
in [CONFIG.md, Live edits](CONFIG.md#live-edits).

### When the file does not read

A wrong `[tune]` value costs only its key. A `key = value` line that does not
read at all costs only that line: a missing quote (`low = "#4caf50`) or a
colour without quotes (`low = #4caf50`, where the `#` starts a comment and
leaves no value) is left out, its default used, and one notification names
it:

> In config.toml, line 12 does not read and is left out: the default is
> used there, and everything else is in use. Fix it in the file.

The file stays as you saved it until the app saves a change of its own,
which writes it anew without that line: fix the line first. (At start the
file as it was is also kept beside it, as `config.toml.<stamp>.left-out`,
once for each version of the file.)

A table written twice (a recipe pasted under the one already there) or a
broken table header (`[tune.widget` without its `]`) costs that table: its
header and its lines are left out, named in the same notification, and the
rest of the file is used, so its keys never land in another table. Put the
keys under the table that is there instead. Only these two kinds of line
are left out. A mistake on any other line (`text_scale 1.2`, with no `=`),
a mistake TOML gives no line for, a mistake found after 8 lines were left out (a
header counts every line of its table), or bytes that are not text make
the file not read at all. Such an edit is not applied: the app keeps
running on the settings it had, and one notification says so:

> config.toml was saved, but it does not read: not a usable config at line
> 12, column 7. AI Limits keeps running on the settings it had and does not
> save over the file until it reads again.

The line and the column point at the mistake; your text is never quoted.
While the file stays broken the app never writes over it, not even when it
quits. Changes you make in the app meanwhile apply but are not saved. Fix
the line and save: a second notification says "config.toml reads again,
and its settings apply now", and those changes are saved together with your
edit. If you quit before
fixing it, your text stays exactly as it is and those changes are lost.
Such a file found at start is set aside as `config.toml.<stamp>.corrupt`
instead, and the app starts as on a fresh install; the old settings stay
in that file.

A file emptied, commented out whole or removed is not applied (it would
read as a fresh install): the app keeps its settings and writes them back
with its next save or when it quits.

### Undoing a change

Delete the key, or put the `# ` back before it, and save: the default
comes back at once. The defaults are today's look, so with every `[tune]`
key commented out everything looks as before you started. The app's next
save of its own writes such a key commented out again, with its default.

Do not empty the file to start over: an emptied file is not applied (see
above). Nor replace the file with a snippet: a file with a `[tune.widget]`
alone, with neither `[reads]` nor any `[[providers]]`, would read as a
fresh install, so it is not applied. A notification says it is only a part
of a config, and the app saves nothing over it until the rest is back
(undo in the editor); found at start, it is set aside as
`config.toml.<stamp>.corrupt`. Paste a snippet into the file, never in
place of it.

## [tune.widget]: the floating widget

| Key | What it changes | Unit | Default | Range |
|---|---|---|---|---|
| `text_scale` | Every text in the widget, and with it the room the text takes: row heights, the name and percent columns, the widget's width. | factor | `1.0` | 0.8 to 1.5 |
| `row_gap` | Extra space between two rows (rows layout) or two columns (columns layout). | pixels | `0` | 0 to 24 |
| `bar_scale` | The bars' thickness: 3, 4 and 5 px in Compact, Medium and Expanded rows; 7, 8.5 and 11 px wide in columns. A thicker bar makes a Medium or Expanded row taller by what it adds. | factor | `1.0` | 0.5 to 3.0 |
| `padding_scale` | The space between the widget's edge and its rows (11 px at the sides, 4 px above and below) or its columns (7 px at the sides, 4 px above, 6 px below). The widget grows by what it adds. | factor | `1.0` | 0.0 to 3.0 |
| `corners` | How Windows 11 rounds the widget's corners: about 8 px, about 4 px, or not at all. Windows 10 draws every window square. | word | `"round"` | `"round"`, `"small"`, `"square"` |
| `low` | The colour of a low usage (below `[tune.format] mid_at`): its bar and its percent. | `"#rrggbb"` | the palette's | any colour |
| `mid` | The colour of a middle usage (from `mid_at` to below `high_at`). | `"#rrggbb"` | the palette's | any colour |
| `high` | The colour of a high usage (from `high_at`), and of the forecast ("~1h 30min"). | `"#rrggbb"` | the palette's | any colour |
| `provider_colors` | A colour for the names of a provider's rows, by provider id: `claude`, `codex`, `copilot`, `antigravity`, `grok`, `cursor`. | table of `"#rrggbb"` | none: every name grey | any colour |

The rows layout is the menu's *Rows* (bars lying down), the columns
layout its *Columns*. Under a `padding_scale` of about 0.5 the round
corners clip the text in them; `corners = "square"` does not.

About the colours:

- A colour is `#` and six hex digits, in quotes: `"#4caf50"`. A short form
  (`"#fff"`) or a name (`"red"`) does not count, and the palette's colour
  stays. An unknown provider id is ignored.
- Your colour is drawn as the palette's are: the bar at the same opacity,
  the percent a little brighter than the bar. *Brightness* in the menu still
  applies; *Saturation* does not change your colours. *Palette* still sets
  the levels you leave out.
- *Monochrome* ignores `low`, `mid`, `high` and `provider_colors`.
- A row whose data stopped updating, or is an estimate, is grey whatever the
  colours.
- These colours are the widget's only: the tray icon keeps its own, and the
  taskbar panel has an ink of its own (`[tune.panel]`).

```toml
[tune.widget]
text_scale = 1.2
row_gap = 2
corners = "small"
provider_colors = { claude = "#d97757", codex = "#10a37f" }
```

## [tune.panel]: the taskbar panel

The panel's size follows the taskbar: its digits are 0.225 of the taskbar's
thickness, the size of the system clock's digits (about 11 px of ink on a
48 px taskbar). Most keys here are a multiple of today's size, `1.0`. The
panel asks the taskbar for as much room as the keys make it need; a panel
that no longer fits beside the tray gives way to the tray icon, as it does
when the taskbar is full.

| Key | What it changes | Unit | Default | Range |
|---|---|---|---|---|
| `digit_size` | The digits' height. The number's column widens with it; the row gap and the bar's thickness are measured in digit heights, so they grow with it too. | factor | `1.0` | 0.5 to 1.5 |
| `bar_length` | The usage bar's length (about half the panel's width on a taskbar at the bottom or the top). Lying down, the panel grows or shrinks with the bar; on a taskbar at the left or the right the panel is as wide as the taskbar, and the bar stays centred and at most that wide. | factor | `1.0` | 0.25 to 3.0 |
| `bar_thickness` | The usage bar's thickness (half a digit's height). Its ends stay round. | factor | `1.0` | 0.25 to 2.0 |
| `row_gap` | The space between the two providers' rows (0.4 of a digit's height lying down, 0.6 standing up). `0` puts the rows edge to edge. | factor | `1.0` | 0.0 to 2.0 |
| `tray_gap` | The space between the panel and the notification area (the tray icons, the clock). It moves the panel and changes the room the panel needs. | screen pixels, whole numbers | `10` | 0 to 200 |
| `ink_dark` | The colour of the digits and the bar on a dark taskbar, in place of the system's near-white `#ececec`. | `"#rrggbb"` | none (the file shows `"#ececec"`, which looks the same) | any colour |
| `ink_light` | The same on a light taskbar, in place of the system's near-black `#242424`. | `"#rrggbb"` | none (the file shows `"#242424"`, which looks the same) | any colour |
| `tooltip_size` | The size of the panel's tooltip (text, padding and corners together), against the shell's own tooltip (12 px text at 100% display scaling). | factor | `1.0` | 0.85 to 2.0 |

- `tray_gap` is in screen pixels and does not follow the display scaling.
  The panel shows only where the panel plus this gap fit between the last
  app button and the tray. Where the tray's edge could not be measured, 10
  more pixels are kept. `[general] panel_offset_x` / `panel_offset_y` are
  something else: see [Related settings](#related-settings).
- The panel follows the Windows mode (Settings > Personalization > Colors,
  "Choose your default Windows mode"), so each mode has an ink of its own.
  The bar's groove and the dimmer ink of a stale or estimated row are made
  from your colour as they are from the default.
- Whatever `tooltip_size` says, the tooltip is never smaller than 85% of
  its size at 100% display scaling, nor larger than three times that.

```toml
[tune.panel]
digit_size = 1.1
bar_length = 0.8
tray_gap = 6
tooltip_size = 1.2
```

## [tune.tray]: the tray icon's rings

The rings of the *Tray icon* indicator, and of the icon that stands in for
the taskbar panel while the panel cannot show (no room on the taskbar, the
Start menu open). The legacy `indicator = "bars"` icon has no rings and
ignores this table.

The icon is drawn on a 32 x 32 canvas, and Windows scales it to the tray:
16 px at 100% display scaling, 20 at 125%, 24 at 150%, 32 at 200%. The
pixels here are canvas pixels: at 100%, 2 of them make 1 on the screen. The
outer ring keeps its place 1 px inside the canvas; the keys move the inner
ring and the hole in the middle.

| Key | What it changes | Unit | Default | Range |
|---|---|---|---|---|
| `ring_width` | Each ring's stroke. A thicker ring leaves less room for short arcs: under a few percent the inner ring shows a dot. | canvas pixels | `5.0` | 2.0 to 6.0 |
| `ring_gap` | The bare space between the outer and the inner ring. Below about 1.5 the two rings start to read as one thick band at 16 px. | canvas pixels | `2.0` | 1.0 to 6.0 |

The inner ring always keeps a hole of at least 1 px in the middle: a gap
that would close it is cut down to fit (twice `ring_width` plus `ring_gap`
stays at most 14). With `ring_width = 6`, for example, the gap is at most 2.

```toml
[tune.tray]
ring_width = 4
ring_gap = 3
```

## [tune.format]: levels and times

| Key | What it changes | Unit | Default | Range |
|---|---|---|---|---|
| `mid_at` | The usage from which a row reads as middle (the mid colour) rather than low: in the widget and in the legacy `"bars"` tray icon. | percent | `60` | 0 to 100 |
| `high_at` | The usage from which a row reads as high (the high colour). Never below `mid_at`: a lower value is raised to it, and the middle level is skipped. | percent | `80` | 0 to 100 |
| `countdown` | How the widget writes a time left: the reset countdown, the forecast, the age of data that stopped updating, Compact's hint line. | word | `"standard"` | `"standard"`, `"compact"`, `"long"` |

| `countdown` | 2 days 5 h 12 min | 1 h 12 min | 12 min | 45 s |
|---|---|---|---|---|
| `"standard"` | 2d 5h | 1h 12min | 12min | 45s |
| `"compact"` | 2d5h | 1h12m | 12m | 45s |
| `"long"` | 2d 5h 12min | 1h 12min | 12min | 45s |

Where a long time does not fit (an Expanded column, say), the standard one
shows. The panel's tooltip and the notifications keep the standard style.
The taskbar panel writes its own times, short like the clock beside it
("3d4h", "2h14", "12m", "<1m"), whatever `countdown` says; the tray icon's
tooltip shows no times. `mid_at` and `high_at` change colours only; when a
notification comes is set elsewhere (see [Related settings](#related-settings)).

```toml
[tune.format]
mid_at = 70
high_at = 90
countdown = "compact"
```

## Recipes

Each snippet names the table its keys go under. The file already has every
`[tune]` table, so put the keys under the table of the same name (take the
`# ` away from a key that is there, or write the key under the header) and
save. Do not paste a second table of the same name: a table may appear only
once in the file, and a second one is left out with every key under it (a
notification names its lines).

### Bigger text on a 4K screen

The widget has the same size in pixels at every display scaling, so on a
4K screen at 150% or 200% it reads small. Make the text, the bars and the
spacing larger together:

```toml
[tune.widget]
text_scale = 1.5
bar_scale = 1.5
padding_scale = 1.5
row_gap = 2
```

`text_scale` stops at 1.5. The taskbar panel and the tray icon already
follow the display scaling.

### A slimmer taskbar panel

The percentage alone, without the bar, with slightly smaller digits and
closer to the tray:

```toml
[ui.panel]
percent = true
reset = false
bar = false

[tune.panel]
digit_size = 0.9
tray_gap = 4
```

A key left out of `[ui.panel]` keeps the panel's default (the percentage,
with its bar), so `bar = false` alone gives the percentage without its bar.
*Settings* sets the same three keys (Percentage, Time to reset and Progress
bar in the Taskbar column of its *Display* tab) and writes `[ui.panel]` once you change them
there; if your file has the table already, change the keys in it.

To keep the bar, but shorter and thinner:

```toml
[tune.panel]
bar_length = 0.6
bar_thickness = 0.75
```

### Your own level colours

```toml
[tune.widget]
low = "#4caf50"
mid = "#ffb300"
high = "#e53935"
```

Leave a level out to keep the palette's colour for it. *Monochrome* ignores
all three.

### Calmer colours: one change, at 90%

The low colour up to 90%, then straight to the high colour, with no middle
level:

```toml
[tune.format]
mid_at = 90
high_at = 90
```

This changes the colours only, not when a notification comes.

### Panel ink in your accent colour

The app does not read the Windows accent colour: write it in. A light shade
for a dark taskbar, a dark one for a light taskbar:

```toml
[tune.panel]
ink_dark = "#7fd4ff"
ink_light = "#0b5394"
```

Setting only the one for the mode you use is enough.

### Thicker tray rings

```toml
[tune.tray]
ring_width = 6
ring_gap = 2
```

At 100% display scaling that is a 3 px ring with 1 px between the two. 6
is the widest, and with it the gap is at most 2. For thinner rings with
more air between them: `ring_width = 3.5` and `ring_gap = 3`.

### A tighter widget

Square corners, less padding, shorter times:

```toml
[tune.widget]
corners = "square"
padding_scale = 0.4

[tune.format]
countdown = "compact"
```

### Provider names in colour

```toml
[tune.widget]
provider_colors = { claude = "#d97757", codex = "#10a37f", grok = "#8b949e" }
```

## Related settings

Some settings outside `[tune]` are config-only too, or take more values in
the file than the menu offers. They are described in the
[full example in CONFIG.md](CONFIG.md#full-example); in short:

| Setting | What it does |
|---|---|
| `[general] panel_offset_x`, `panel_offset_y` | Nudge the panel after it is placed, by -200 to 200 pixels, in either direction and across the taskbar too. Unlike `[tune.panel] tray_gap`, they do not change the room the panel asks of the taskbar: a repair for shells and layouts where the placement misses. |
| `[ui.panel] percent`, `reset`, `bar` | What each row of the panel shows; also in *Settings*, *Display*, Taskbar column. `[tune.panel]` sizes what they show. |
| `alert_threshold` in each `[[providers]]` | The usage, in percent, at which that provider's notification comes. `[tune.format] mid_at` and `high_at` never change it. |
| `[notifications] cooldown_minutes` | The shortest time between two notifications of one provider. |
| `[general] update_interval_secs` | Seconds between updates: any value from 60 (a smaller one reads as 60), where the menu offers 1, 5, 15 and 30 minutes. |
| `[window] opacity`, `[ui] brightness`, `[ui] saturation` | Any value in their range (0.10 to 0.85, 20 to 100, 0 to 100), not only the menu's steps. |

## Notes

- **Out of range is held to the range.** A number outside its range reads
  as the nearest end of it: `text_scale = 3` reads as 1.5. The file keeps
  what you wrote.
- **A wrong value costs only its key.** A value of the wrong kind (a word
  where a number belongs, `tray_gap = 12.5` where a whole number belongs,
  `corners = "rounded"`), a colour that is not `"#rrggbb"`, or `nan` and
  `inf` read as unset: that key takes its default, and the rest of the file
  applies. No message says so: if a change shows nothing, check the value
  against the tables above. Where a decimal is asked for, a whole number
  works too (`row_gap = 4`).
- **Unknown keys are ignored**, quietly: a misspelt key (`text_size`) or
  table (`[tune.widgets]`) too.
- **What the app writes back.** The app's next save of its own writes your
  `[tune]` keys as you wrote them, out-of-range numbers and colours that do
  not count included. It leaves out unknown keys, values of the wrong kind
  and your comments, and writes its own notes instead. It writes all four
  `[tune]` tables every time, every key not set commented out with its
  default.
- **Monochrome** (menu: Palette) ignores `low`, `mid`, `high` and
  `provider_colors`; `mid_at` and `high_at` still choose between its three
  greys.
- **The tray icon's tooltip is drawn by Windows** and keeps the system's
  size. `tooltip_size` sizes only the taskbar panel's tooltip, which the app
  draws itself.
