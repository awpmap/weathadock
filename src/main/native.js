'use strict';
// Windows-only helpers built on user32/dwmapi through koffi (prebuilt, no compiler needed).
// They answer one question for the app: "is the desktop widget covered by other windows?"
// and keep the widget at desktop level. On other platforms, or if anything fails to load,
// `available` is false and the app simply keeps the widget as a normal window.

let api = null;

function load() {
  if (process.platform !== 'win32') return null;
  try {
    const koffi = require('koffi');
    const user32 = koffi.load('user32.dll');
    const dwmapi = koffi.load('dwmapi.dll');
    return {
      GetTopWindow: user32.func('intptr_t __stdcall GetTopWindow(intptr_t hWnd)'),
      GetWindow: user32.func('intptr_t __stdcall GetWindow(intptr_t hWnd, uint32_t uCmd)'),
      IsWindowVisible: user32.func('int __stdcall IsWindowVisible(intptr_t hWnd)'),
      IsIconic: user32.func('int __stdcall IsIconic(intptr_t hWnd)'),
      GetWindowRect: user32.func('int __stdcall GetWindowRect(intptr_t hWnd, void *lpRect)'),
      GetClassNameW: user32.func('int __stdcall GetClassNameW(intptr_t hWnd, void *lpClassName, int nMaxCount)'),
      GetWindowLongW: user32.func('int32_t __stdcall GetWindowLongW(intptr_t hWnd, int nIndex)'),
      GetForegroundWindow: user32.func('intptr_t __stdcall GetForegroundWindow()'),
      GetWindowThreadProcessId: user32.func('uint32_t __stdcall GetWindowThreadProcessId(intptr_t hWnd, void *lpdwProcessId)'),
      SetWindowPos: user32.func('int __stdcall SetWindowPos(intptr_t hWnd, intptr_t hWndInsertAfter, int X, int Y, int cx, int cy, uint32_t uFlags)'),
      FindWindowW: user32.func('intptr_t __stdcall FindWindowW(str16 lpClassName, str16 lpWindowName)'),
      FindWindowExW: user32.func('intptr_t __stdcall FindWindowExW(intptr_t hWndParent, intptr_t hWndChildAfter, str16 lpszClass, str16 lpszWindow)'),
      DwmGetWindowAttribute: dwmapi.func('int32_t __stdcall DwmGetWindowAttribute(intptr_t hwnd, uint32_t dwAttribute, void *pvAttribute, uint32_t cbAttribute)'),
    };
  } catch (e) {
    console.error('Native window helpers unavailable:', e.message);
    return null;
  }
}
api = load();

const GW_HWNDNEXT = 2;
const GWL_EXSTYLE = -20;
const WS_EX_TRANSPARENT = 0x20;
const WS_EX_LAYERED = 0x80000;
const DWMWA_EXTENDED_FRAME_BOUNDS = 9;
const DWMWA_CLOAKED = 14;
const SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOACTIVATE = 0x10;
const HWND_BOTTOM = 1n, HWND_TOPMOST = -1n, HWND_NOTOPMOST = -2n;
const DESKTOP_CLASSES = new Set(['Progman', 'WorkerW']);
const SHELL_CLASSES = new Set(['Progman', 'WorkerW', 'Shell_TrayWnd', 'Shell_SecondaryTrayWnd']);

const big = (v) => (typeof v === 'bigint' ? v : BigInt(v || 0));

function hwndOf(win) {
  const buf = win.getNativeWindowHandle();
  return buf.length >= 8 ? buf.readBigInt64LE(0) : BigInt(buf.readInt32LE(0));
}

function rectOf(hwnd, visibleBounds) {
  const buf = Buffer.alloc(16);
  // The DWM frame bounds exclude the invisible resize border that GetWindowRect includes.
  const ok = visibleBounds && api.DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, buf, 16) === 0;
  if (!ok && !api.GetWindowRect(hwnd, buf)) return null;
  return { left: buf.readInt32LE(0), top: buf.readInt32LE(4), right: buf.readInt32LE(8), bottom: buf.readInt32LE(12) };
}

function classOf(hwnd) {
  const buf = Buffer.alloc(512);
  const n = api.GetClassNameW(hwnd, buf, 255);
  return n > 0 ? buf.toString('utf16le', 0, n * 2) : '';
}

function pidOf(hwnd) {
  const buf = Buffer.alloc(4);
  api.GetWindowThreadProcessId(hwnd, buf);
  return buf.readUInt32LE(0);
}

function isCloaked(hwnd) {
  const buf = Buffer.alloc(4);
  return api.DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, buf, 4) === 0 && buf.readInt32LE(0) !== 0;
}

/**
 * Look at the windows stacked above `targetHwnd` and report how much of it they cover.
 * Returns { coverage: 0..1, fgClass, fgRect, desktopForeground }.
 */
function snapshot(targetHwnd) {
  const target = big(targetHwnd);
  const tr = rectOf(target, false);
  const covering = [];
  if (tr && tr.right > tr.left && tr.bottom > tr.top) {
    let h = big(api.GetTopWindow(0n));
    for (let guard = 0; h !== 0n && h !== target && guard < 2000; guard++, h = big(api.GetWindow(h, GW_HWNDNEXT))) {
      if (!api.IsWindowVisible(h) || api.IsIconic(h)) continue;
      const r = rectOf(h, true);
      if (!r || r.right <= tr.left || r.left >= tr.right || r.bottom <= tr.top || r.top >= tr.bottom) continue;
      if (r.right - r.left < 2 || r.bottom - r.top < 2) continue;
      const ex = api.GetWindowLongW(h, GWL_EXSTYLE);
      if ((ex & WS_EX_TRANSPARENT) && (ex & WS_EX_LAYERED)) continue; // click-through overlays
      if (isCloaked(h)) continue;                                      // hidden store apps, other virtual desktops
      if (pidOf(h) === process.pid) continue;                          // our own windows
      if (SHELL_CLASSES.has(classOf(h))) continue;
      covering.push(r);
    }
  }
  let coverage = 0;
  if (covering.length) {
    const N = 8;
    let hit = 0;
    for (let iy = 0; iy < N; iy++) {
      for (let ix = 0; ix < N; ix++) {
        const x = tr.left + ((ix + 0.5) / N) * (tr.right - tr.left);
        const y = tr.top + ((iy + 0.5) / N) * (tr.bottom - tr.top);
        if (covering.some((r) => x >= r.left && x < r.right && y >= r.top && y < r.bottom)) hit++;
      }
    }
    coverage = hit / (N * N);
  }
  const fg = big(api.GetForegroundWindow());
  const fgClass = fg ? classOf(fg) : '';
  return {
    coverage,
    fgClass,
    fgRect: fg ? rectOf(fg, true) : null,
    fgIsOurs: fg ? pidOf(fg) === process.pid : false,
    desktopForeground: DESKTOP_CLASSES.has(fgClass),
  };
}

const NOMOVE = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
const sendToBottom = (hwnd) => api.SetWindowPos(big(hwnd), HWND_BOTTOM, 0, 0, 0, 0, NOMOVE);
const keepOnTop = (hwnd) => api.SetWindowPos(big(hwnd), HWND_TOPMOST, 0, 0, 0, 0, NOMOVE);
// Bring to the front of the normal windows without taking focus or staying always-on-top.
function raise(hwnd) {
  api.SetWindowPos(big(hwnd), HWND_TOPMOST, 0, 0, 0, 0, NOMOVE);
  api.SetWindowPos(big(hwnd), HWND_NOTOPMOST, 0, 0, 0, 0, NOMOVE);
}

/** Physical-pixel rectangles of the primary taskbar and its notification area (tray). */
function taskbarRects() {
  const bar = big(api.FindWindowW('Shell_TrayWnd', null));
  if (!bar) return null;
  const tray = big(api.FindWindowExW(bar, 0n, 'TrayNotifyWnd', null));
  return { taskbar: rectOf(bar, false), tray: tray ? rectOf(tray, false) : null };
}

module.exports = {
  available: !!api,
  hwndOf, snapshot, sendToBottom, keepOnTop, raise, taskbarRects,
};
