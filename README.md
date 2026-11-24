# WeathaDock

A translucent desktop weather widget for Windows 10 and 11.

- **On the desktop** it is a glass widget showing the current temperature, today's high and low,
  what is coming (rain or snow timing down to 15 minutes), the next hours, the next days, and
  official National Weather Service alerts.
- **When apps cover the widget** it moves to a small pill on the taskbar (or only the tray icon,
  your choice). The tray icon always shows the current temperature.
- **Click anything** to open the full window: overview, 48-hour chart, 7-day forecast, full alert
  text, and settings.

## Install

Download the `-setup.exe` file from the [latest release](https://github.com/awpmap/weathadock/releases/latest)
and run it. After that the app updates itself: it checks for new releases in the background and
offers a restart when one is ready (Settings, Updates).

The first run asks for a US zip code. No account or API key is needed.

## Data

| Source | Used for |
|---|---|
| [National Weather Service](https://www.weather.gov/documentation/services-web-api) (api.weather.gov) | Forecast highs and lows, hourly forecast, forecast text, official alerts |
| [Open-Meteo](https://open-meteo.com/) | Current conditions, 15-minute precipitation timing, UV, sunrise and sunset |
| [Zippopotam.us](https://api.zippopotam.us/) | Zip code to coordinates (Open-Meteo geocoding as a fallback) |

If one weather source is down the app keeps working with the other. Alerts are checked every two
minutes, and the app says so when they could not be checked instead of showing "no alerts".

## Run from source

Requires [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/) 20 or newer, on Windows.

```
npm install
npm run dev
```

## Build the Windows installer

```
npm run build
```

The installer is written to `target/release/bundle/nsis/`. The GitHub Actions workflow builds and
smoke-tests it on every push and pull request. It publishes a release only from `main`, when the
commit message contains `[release]` and the `version` in `src-tauri/tauri.conf.json` has no
release yet.

## How it works

| Path | Role |
|---|---|
| `src-tauri/src/main.rs` | Windows, tray, settings, refresh schedule, desktop/taskbar switching |
| `src-tauri/src/net.rs` | Zip lookup and the weather requests |
| `src-tauri/src/win32.rs` | Windows window-stack checks (is the widget covered?) and the wallpaper behind it |
| `src-tauri/src/updater.rs` | In-app updates from GitHub Releases |
| `src-tauri/src/bridge.js` | Connects the screens to the Rust side (`window.sky`) |
| `crates/core/` | Merging of the weather sources and the settings model, with tests |
| `src/renderer/` | The screens: `widget`, `pill`, `detail`, `setup` |
| `docs/UI-CONTRACT.md` | The data shape and API shared by both sides |

## Your data

Settings are stored in `%APPDATA%\app.weathadock.desktop\settings.json`. The file never leaves
your PC, and there is no account, no analytics and no telemetry. To do its job the app sends:

- the zip code you type, once, to the zip lookup service, to find its coordinates;
- those coordinates to the National Weather Service and Open-Meteo at every refresh;
- a request to GitHub for the latest release, when it checks for updates.

The same folder holds what helps when something goes wrong: `settings.backup.json` (the last
settings that loaded correctly, restored by the app itself if the main file is ever damaged; the
damaged file is kept as `settings.damaged.json`) and `crash.log` (written only if the app stops
unexpectedly). Like the settings, they stay on your PC.

To choose between light and dark text by itself, the widget looks at how bright your wallpaper is
behind it. The picture is read on your PC, nothing of it is stored, and nothing about it is sent
anywhere.

## Tests

```
cargo test -p weathadock-core
```

`crates/core/tests/truth.rs` states the rules the weather on screen must obey (the "Now" column is
the current reading, a picture never contradicts its words, and so on). `tests/snapshot.rs` compares
whole results with recorded ones; after an intended change, record them again with
`WEATHADOCK_BLESS=1 cargo test -p weathadock-core --test snapshot` and review the difference.
