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

Download `WeathaDock-Setup-<version>.exe` from the [latest release](https://github.com/awpmap/weathadock/releases/latest)
and run it. After that the app updates itself: it checks for new releases in the background and
offers a restart when one is ready (Settings, Updates).

The first run asks for a US zip code. No account or API key is needed.

## Data

| Source | Used for |
|---|---|
| [National Weather Service](https://www.weather.gov/documentation/services-web-api) (api.weather.gov) | Forecast highs and lows, hourly forecast, forecast text, official alerts |
| [Open-Meteo](https://open-meteo.com/) | Current conditions, 15-minute precipitation timing, UV, sunrise and sunset |
| [Zippopotam.us](https://api.zippopotam.us/) | Zip code to coordinates (Open-Meteo geocoding as a fallback) |

If one weather source is down the app keeps working with the other.

## Run from source

Requires [Node.js](https://nodejs.org/) 20 or newer.

```
npm install
npm start
```

## Build the Windows installer

On Windows:

```
npm run dist
```

This writes an installer (`WeathaDock-Setup-<version>.exe`) to `dist/`.
The included GitHub Actions workflow does the same on every push and attaches the files to the run.

## How it works

| Path | Role |
|---|---|
| `src/main/main.js` | Windows, tray, settings, refresh schedule, desktop/taskbar switching |
| `src/main/weather.js` | Fetches and merges the data sources into one state object |
| `src/main/native.js` | Windows window-stack checks (is the widget covered?) through `koffi` |
| `src/renderer/` | The four screens: `widget`, `pill`, `detail`, `setup` |
| `docs/UI-CONTRACT.md` | The data shape and API shared by the main process and the screens |

Glass styles: **Clear** is a truly transparent window (the wallpaper shows through, sharp).
**Frosted** uses Windows 11 acrylic blur and needs Windows 11 22H2 or newer.

Settings are stored in `%APPDATA%\WeathaDock\settings.json`.

## Tests

```
npm test
```
