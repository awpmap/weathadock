# Skylight renderer contract

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
