# WeathaDock renderer contract

The main process owns data, windows and settings. Renderer pages are plain HTML/CSS/JS
(no bundler, no framework, no CDN, no remote fonts: the app must work offline and CSP is
`default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:`).
Font stack: `"Segoe UI Variable Display", "Segoe UI Variable", "Segoe UI", system-ui, sans-serif`.

## Pages (all in `src/renderer/`)

| File | Window | Notes |
|---|---|---|
| `widget.html` | Desktop widget. Frameless, sits on the desktop behind apps. About 380 px wide, height fits content. | Main visual. Never receives keyboard focus. |
| `pill.html` | Taskbar pill. Frameless, transparent, always on top, sits over the Windows taskbar. Height is exactly the window height (40 to 48 px), width fits content (about 120 to 260 px). | Shown only while apps cover the widget. |
| `detail.html` | Detail app window. Frameless, resizable, default 1000x700, min 780x560. | Tabs/sections: Overview, Hourly, 7-day, Alerts, Settings. |
| `setup.html` | First-run setup. Frameless, 520x640, not resizable. | Zip entry + look choice. |

Every page is loaded with query params: `?glass=clear|frosted&platform=win32|linux|darwin`.
Put them on `<html data-glass=… data-platform=…>` before first paint.

- `glass=clear`: the window is truly transparent (wallpaper shows through, NO blur available).
  The page paints its own translucent panel with rounded corners. Text must stay legible over
  any wallpaper (bright, dark, busy): use a dark tint panel plus subtle text shadow.
- `glass=frosted`: the OS paints blurred acrylic behind the whole window and rounds the window
  corners itself. The page should fill the window edge to edge with only a faint tint
  (no own rounded outer panel, no outer margin, no drop shadow).
- `detail.html` always behaves as frosted-capable: fill the window edge to edge with a dark
  tint around `rgba(16,20,28,0.62)`; if the OS has no acrylic the backdrop is solid near-black,
  which must also look good.
- `pill.html` is always a transparent window: in both modes paint a small rounded translucent
  capsule; keep 4 px vertical breathing room.

## `window.sky` (preload API)

```ts
sky.getState(): Promise<WeatherState>
sky.onState(cb: (s: WeatherState) => void): void          // pushed on every refresh
sky.getSettings(): Promise<Settings>
sky.onSettings(cb: (s: Settings) => void): void
sky.setSettings(patch: Partial<Settings>): Promise<Settings>
sky.lookupZip(zip: string): Promise<{ok: true, zip, city, state, lat, lon} | {ok: false, error: string}>
sky.completeSetup(v: {zip: string, glass: 'clear'|'frosted'}): Promise<{ok: boolean, error?: string}>
sky.refresh(): Promise<void>
sky.openDetail(section?: 'overview'|'hourly'|'daily'|'alerts'|'settings'): void
sky.onNavigate(cb: (section: string) => void): void       // detail.html: main asks to show a section
sky.windowControl(action: 'close'|'minimize'|'maximize'): void   // detail + setup title bar buttons
sky.moveBy(dx: number, dy: number): void                  // widget + pill: drag the window (screen px deltas)
sky.dragEnd(): void                                       // persist position after a drag
sky.setContentSize(width: number, height: number): void   // widget + pill: fit window to content (CSS px)
sky.setTrayIcon(dataUrl: string): void                    // widget only, see below
sky.quit(): void
```

When `window.sky` is undefined (page opened in a plain browser for design work) load
`mock.js`, which must define a compatible `window.sky` backed by sample data, with a
`?scenario=` switch: `clear-day`, `rain-soon`, `snow-night`, `severe-alert`, `loading`, `error`.

### Dragging (widget and pill)
Do NOT use `-webkit-app-region: drag` there (it swallows clicks). On pointerdown capture the
pointer; once movement exceeds 4 px treat it as a drag and send `sky.moveBy(dx, dy)` with
`screenX/screenY` deltas per pointermove; on release call `sky.dragEnd()`. A press without
movement is a click. `detail.html` and `setup.html` DO use `-webkit-app-region: drag` on the
header (and `no-drag` on buttons).

### Click targets (widget and pill)
Clicking opens the detail window on the matching section: alert banner → `alerts`, hourly strip →
`hourly`, day rows → `daily`, anything else → `overview`. Pill click → `overview` (or `alerts`
if the alert badge is clicked).

### Content size
Widget and pill call `sky.setContentSize(w, h)` after each render and when their size changes
(ResizeObserver on the root). Widget width is fixed at 380; only height varies. Do not
animate the outer size.

### Tray icon (widget.js)
After each state update draw the current temperature (digits only, e.g. `72`, no degree sign
if 3 chars) on a 64x64 canvas: bold white text filling the square, soft dark outline so it
reads on light and dark taskbars; when there is an active alert add a small amber/red dot in
the top-right corner. Call `sky.setTrayIcon(canvas.toDataURL('image/png'))`. If there is no
temperature draw a simple cloud glyph.

## Types

```ts
type Settings = {
  zip: string | null
  glass: 'clear' | 'frosted'
  placement: 'desktop' | 'taskbar'     // desktop widget, or a strip docked at the left end of the taskbar (pill window; see getDock)
  coveredMode: 'taskbar' | 'tray'      // where it goes when apps cover the widget: taskbar pill + tray, or tray only
  showWidget: boolean
  launchAtLogin: boolean
  alertNotifications: boolean
  refreshMinutes: 5 | 10 | 15 | 30
}

type IconKey = 'clear' | 'mostly-clear' | 'partly-cloudy' | 'cloudy' | 'fog' | 'drizzle'
  | 'rain' | 'heavy-rain' | 'thunderstorm' | 'snow' | 'sleet' | 'wind'

type WeatherState = {
  status: 'loading' | 'ok' | 'error'     // 'error' only when there is no data at all
  error: string | null                   // human readable; may be set with status 'ok' + stale true
  stale: boolean                         // last refresh failed, showing older data
  updatedAt: string | null               // ISO
  sources: { nws: boolean, openMeteo: boolean }   // which sources answered on the last refresh
  location: { zip: string, city: string, state: string, lat: number, lon: number, timeZone: string } | null
  current: {
    temp: number | null                  // all temps °F, already rounded
    feelsLike: number | null
    condition: string                    // "Partly Cloudy"
    icon: IconKey
    isDay: boolean
    humidity: number | null              // %
    dewPoint: number | null
    windSpeed: number | null             // mph
    windGust: number | null
    windDir: string | null               // "NW"
    pressure: number | null              // inHg, 2 decimals
    visibility: number | null            // miles
    uvIndex: number | null
  } | null
  today: {
    high: number | null
    low: number | null
    sunrise: string | null               // ISO
    sunset: string | null
    summary: string                      // NWS detailed forecast sentence(s) for the current period
  } | null
  nextPrecip: {                          // "incoming weather"
    kind: 'rain' | 'snow' | 'mix' | 'storm' | null   // null = nothing in the next 12 h
    isNow: boolean                       // precipitating right now
    startsAt: string | null              // ISO
    endsAt: string | null
    minutesUntil: number | null
    headline: string                     // ready to show: "Rain starting in 25 min", "Snow until 9 PM", "Dry for the next 12 hours"
  }
  minutely: Array<{ time: string, precip: number, kind: 'rain' | 'snow' | 'mix' | null }>   // 15-min steps, next 6 h, precip in inches per 15 min
  hourly: Array<{                        // next 48 h
    time: string, temp: number, feelsLike: number | null, icon: IconKey, isDay: boolean,
    condition: string, precipProb: number | null, precipAmount: number | null,   // % and inches
    windSpeed: number | null, windDir: string | null, humidity: number | null
  }>
  daily: Array<{                         // 7 days, first is today
    date: string                         // "2026-11-18"
    name: string                         // "Today", "Thu"
    high: number | null, low: number | null
    icon: IconKey, condition: string, precipProb: number | null
    dayText: string, nightText: string   // NWS detailed forecasts (may be "")
    sunrise: string | null, sunset: string | null
  }>
  alerts: Array<{                        // official NWS alerts, most severe first
    id: string, event: string            // "Winter Storm Warning"
    severity: 'Extreme' | 'Severe' | 'Moderate' | 'Minor' | 'Unknown'
    urgency: string, certainty: string
    headline: string, description: string, instruction: string   // description/instruction are multi-paragraph plain text with \n
    onset: string | null, ends: string | null, expires: string | null
    sender: string, areaDesc: string
  }>
}
```

All times are ISO strings with offset; format them in `location.timeZone` with `Intl.DateTimeFormat`.

---

# Revision 2 (supersedes anything above that conflicts)

## Transparency rules
- **App windows are solid.** `detail.html` and `setup.html` are opaque windows (background
  `#121722`); they never show the desktop. No acrylic, no translucency there.
- **New default widget style `glass=none`:** "fully transparent, like TranslucentTB". The widget
  paints NO panel, NO tint, NO border, NO card backgrounds: only text, icons, bars and hairlines
  floating directly on the wallpaper. Legibility over any wallpaper (white, black, busy photo)
  must come from text shadows / soft dark halos on glyphs and icons only. Small functional chips
  may keep their own fill where colour carries meaning (alert banner, precipitation bars, range bars).
  The taskbar pill in `none` mode likewise has no capsule background (alert badge keeps its fill).
- `Settings.glass` is now `'none' | 'clear' | 'frosted'`; the query param is `glass=none|clear|frosted`
  (`boot.js` must accept `none` and treat unknown values as `none`). `clear` and `frosted` remain
  as optional widget styles, labelled e.g. "Transparent" (none, default), "Tinted glass" (clear),
  "Frosted" (frosted). The style applies to the desktop widget and pill only.

## Multiple locations
```ts
type Place = { zip: string, city: string, state: string }
sky.getLocations(): Promise<{ locations: Place[], activeZip: string | null }>
sky.onLocations(cb: (v: { locations: Place[], activeZip: string | null }) => void): void
sky.addLocation(zip: string): Promise<{ ok: true, place: Place } | { ok: false, error: string }>   // validates the zip, adds it and makes it active
sky.removeLocation(zip: string): Promise<void>     // the last remaining location cannot be removed (main ignores it)
sky.setActiveLocation(zip: string): Promise<void>  // main pushes a new state (cached or 'loading') then refreshes
```
- `WeatherState.location` is always the active place. All surfaces show the active place.
- `Settings.zip` is read-only now (mirrors `activeZip`); do not send `zip` in `setSettings`.
- Detail window: a locations manager (list with select / remove, add by zip with inline
  validation) and a quick switcher that is always reachable (e.g. the location name in the
  header opens a menu). Widget: when there is more than one location, an unobtrusive way to
  switch (e.g. click the city name to cycle, or small prev/next chevrons); never shown with one.
- `completeSetup({ zip, glass })` is unchanged; setup offers the three widget styles with
  Transparent preselected.

## Fetch button
- `sky.refresh()` resolves when the fetch has finished (state is pushed through `onState`).
- Widget: a small refresh button beside the "Updated …" time; detail: a clearly labelled
  "Refresh" button in the header. Both spin while the promise is pending and are disabled meanwhile.
- Clicking the big temperature on the widget opens the detail window (`overview`), as before.

## Widget layouts (Revision 2 addition)
- `Settings.layout: 'full' | 'compact' | 'wide' | 'minimal'` (default `'full'`), changed through
  `setSettings({ layout })` and pushed through `onSettings`; the widget re-renders live.
  - `full`: everything (current temp, high/low, incoming weather + timeline, hours, days, alerts). Vertical.
  - `compact`: current temp, condition, high/low, incoming-weather line, alert chip. Small vertical block.
  - `wide`: horizontal strip: current block on the left, next hours and/or days to the right.
  - `minimal`: just icon + big temperature + high/low (alert shown as a small badge).
- Layouts are chosen in the Settings section of the detail window with small visual previews.
  They apply to every glass style, but design them first for `glass=none`.
- The widget is no longer fixed at 380 wide: `sky.setContentSize(w, h)` now sets both
  (main clamps width to 140..900 and height to 60..900).

## Covered mode is either/or (Revision 2 addition)
`Settings.coveredMode` is now strictly one or the other, default `'tray'`:
- `'tray'`: the tray icon shows the temperature number; no taskbar pill.
- `'taskbar'`: the taskbar pill shows the weather; the tray icon is the plain app icon (main
  handles that; the widget keeps calling `setTrayIcon` as before).
Settings copy must present it as a choice between "Tray temperature" and "Taskbar pill", never "both".

## In-app updates (Revision 2 addition)
```ts
type UpdateState = {
  version: string                       // running version, e.g. "1.0.0"
  status: 'unsupported' | 'idle' | 'checking' | 'downloading' | 'ready' | 'up-to-date' | 'error'
  newVersion: string | null, percent: number | null, error: string | null, checkedAt: string | null
}
sky.getUpdateState(): Promise<UpdateState>
sky.onUpdateState(cb: (u: UpdateState) => void): void
sky.checkForUpdates(): Promise<UpdateState>   // downloads automatically when one is found
sky.installUpdate(): void                     // only when status is 'ready': restarts into the new version
```
Settings gets an "Updates" group: current version, a "Check for updates" button, live status
(checking, downloading NN%, up to date as of time, error text), and a prominent "Restart to update"
button when `ready`. `unsupported` means this copy cannot update itself (development run or the
plain zip): say "Updates are available in the installed version" and hide the buttons. When
`ready`, also show a small unobtrusive "Update ready" affordance in the detail window chrome.

---

# Revision 3: customizable widget (v1.1)

`Settings.widget` is the single source of truth for what the desktop widget shows and how it is
arranged. The four v1 layouts become presets that write this object; any manual change sets
`Settings.layout` to `'custom'`.

```ts
type WidgetConfig = {
  orientation: 'vertical' | 'horizontal'   // stack sections top to bottom, or flow them left to right
  size: 'small' | 'medium' | 'large' | 'xlarge'   // overall scale of the widget
  align: 'left' | 'center' | 'right'       // text/content alignment within the widget
  ink: 'light' | 'dark'                    // light text for dark wallpapers, dark text (light halo) for bright ones
  hours: 4 | 6 | 8 | 12                    // how many hours in the hourly strip
  days: 3 | 5 | 7                          // how many days in the daily list
  show: {                                  // every element is individually toggleable
    location, updated, refresh, icon, temp, condition, highLow, feelsLike,
    incoming,   // the "rain starting in 25 min" line
    timeline,   // the precipitation bar chart
    hourly, daily, alerts,
    wind, humidity, uv, sun                // extra stats, off by default (sun = sunrise/sunset)
  : boolean }
}
Settings.widget: WidgetConfig
Settings.layout: string      // preset id ('full', 'compact', 'wide', 'minimal', plus new presets you add) or 'custom'
sky.setSettings({ widget: Partial<WidgetConfig>, layout?: string })   // main merges `widget` (and `show`) over the stored config and validates it
```

Rules:
- The widget must render sensibly for ANY combination, including everything off except one
  element, horizontal with many sections, and nothing at all (show a tiny placeholder that still
  opens the app). The window size follows the content (`setContentSize`), width 140..900, height 60..900;
  horizontal layouts must wrap or trim rather than exceed 900.
- Presets: keep the four from v1 and add more distinct ready-made styles (for example a slim
  one-line "ticker", a "forecast" style with days only, a centered "hero" temperature). A preset
  is just a `WidgetConfig`; define them in one shared file used by both the widget and Settings.
- Settings (solid detail window) gets a "Customize widget" area: preset gallery, orientation,
  size, alignment, text colour, hour/day counts, and a grouped list of toggles for every element,
  with a LIVE PREVIEW of the widget that updates instantly, shown over a selectable sample
  backdrop (dark, bright, photo-like) so people can judge legibility. Changes apply to the real
  widget immediately. Include "Reset to preset".
- The taskbar pill is not affected by `widget` config.
- Mock: add `widget` to mock settings and a `?widget=<url-encoded JSON>` design param.

---

# Revision 4: quality-of-life pass (ships together with Revision 3)

Guiding words from the person: "need a smooth clean sleek experience, lightweight as-well dont
add a bunch of" extras. So: polish and small conveniences only. No new dependencies, no
continuous animation loops, no decorative features.

## Polish requirements
- **Optical sizing and alignment.** Reported bug: in the minimal layout the weather icon is
  visibly smaller than the temperature digits and the degree sign, so the pair looks offset.
  Wherever an icon sits beside the big temperature (every preset and any custom combination,
  every size) the icon's visual height must match the digits' cap height and share their optical
  centre line; the degree sign must not make the number look taller than the icon. Apply the same
  care to every icon+text pair in the widget, pill, detail window and setup (baseline or centre
  alignment chosen deliberately, consistent gaps).
- Smoothness: state changes (refresh, location switch, layout change) must not flash, jump or
  reflow visibly; keep transitions short and subtle; respect reduced motion.

## New settings (main already implements the behaviour)
```ts
Settings.lockPosition: boolean      // widget cannot be dragged
Settings.snapToEdges: boolean       // widget snaps to screen edges while dragging (default true)
Settings.widgetOnTop: boolean       // keep the widget above other windows instead of on the desktop
Settings.widgetOpacity: number      // 30..100 (%), whole widget
sky.showContextMenu(): void         // widget + pill: call on right-click (contextmenu event); main shows a native menu
```
- Settings UI: put opacity (slider) with the other design options in "Customize widget";
  lock position, snap to edges and keep above other windows in a compact "Behaviour" group.
- When `lockPosition` is on, the widget should not show a move cursor or start a drag.
- Detail window keyboard: Ctrl+R refreshes, Ctrl+1..5 switch sections, Esc closes an open menu.
- Main now remembers the detail window's size and position and shows the last forecast
  instantly at startup; nothing needed from the renderer.

## Revision 5: taskbar dock

`Settings.placement = 'taskbar'` replaces the desktop widget with the pill window docked in the free
space left of the Start button. `sky.getDock()` / `sky.onDock(cb)` give `{ room, light }`: the logical
pixels available (0 when the taskbar has no free stretch, in which case the pill keeps its place by
the tray) and whether the taskbar is light. `Settings.taskbar = { condition, highLow, hint: boolean, hours: 0|2|4|6 }`
chooses what the strip shows; patch it as a whole object. The pill drops hours, then the hint, then the words to fit.

## Revision 6: mouse battery

`Settings.mouseBattery: boolean` (default off). `sky.getBattery()` / `sky.onBattery(cb)` give
`{ percent, charging, low } | null`, read from a Logitech wireless mouse over HID++ (see `hidpp.rs`).
