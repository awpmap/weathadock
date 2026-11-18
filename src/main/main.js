'use strict';
const {
  app, BrowserWindow, Tray, Menu, ipcMain, nativeImage, screen, shell, Notification, powerMonitor, net,
} = require('electron');
const path = require('path');
const os = require('os');
const { Store, sanitizePatch } = require('./store');
const weather = require('./weather');
const native = require('./native');
const { createUpdater } = require('./updater');

const APP_NAME = 'Skylight';
const RENDERER = path.join(__dirname, '..', 'renderer');
const ICON = path.join(__dirname, '..', '..', 'build', 'icon.png');
const IS_WIN = process.platform === 'win32';
// Acrylic backdrops need Windows 11 22H2 (build 22621) or newer.
const HAS_ACRYLIC = IS_WIN && Number(os.release().split('.')[2] || 0) >= 22621;
const COVERED_AT = 0.35;      // widget counts as covered when this share of it is hidden
const TICK_MS = 500;

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', () => openDetail('overview'));
}
app.setAppUserModelId('app.skylight.weather');

let store;
let tray = null;
let widget = null;
let pill = null;
let detail = null;
let setup = null;
let quitting = false;
let updater = null;
let refreshTimer = null;
let retryTimer = null;
let tickTimer = null;
let refreshing = null;
const pointCache = {};
const stateCache = new Map();  // zip -> last good WeatherState, for instant location switching
const sizes = new WeakMap();   // window -> intended { width, height } in DIP
const drags = new WeakMap();   // window -> { x, y } accumulated drag position

const emptyState = (status = 'loading', error = null) => ({
  status, error, stale: false, updatedAt: null, sources: { nws: false, openMeteo: false },
  location: null, current: null, today: null,
  nextPrecip: { kind: null, isNow: false, startsAt: null, endsAt: null, minutesUntil: null, headline: '' },
  minutely: [], hourly: [], daily: [], alerts: [],
});
let state = emptyState();

// Use Chromium's network stack so system proxy settings are respected.
const fetchFn = (url, init) => net.fetch(url, init);

// ---------------------------------------------------------------- helpers

const alive = (w) => w && !w.isDestroyed();
const allWindows = () => [widget, pill, detail, setup].filter(alive);
const broadcast = (channel, value) => allWindows().forEach((w) => w.webContents.send(channel, value));
// Widget style: 'none' (fully transparent), 'clear' (tinted) or 'frosted' (Windows 11 acrylic).
function glass() {
  const g = store.get('glass');
  if (g === 'frosted') return HAS_ACRYLIC ? 'frosted' : 'clear';
  return g === 'clear' ? 'clear' : 'none';
}

function load(win, page, extra = {}) {
  win.loadFile(path.join(RENDERER, page), { query: { glass: glass(), platform: process.platform, ...extra } });
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https:\/\//.test(url)) shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (e, url) => {
    e.preventDefault();
    if (/^https:\/\//.test(url)) shell.openExternal(url);
  });
}

const webPreferences = () => ({
  preload: path.join(__dirname, 'preload.js'), contextIsolation: true, sandbox: true, nodeIntegration: false,
  backgroundThrottling: false,
});

// Options that make a window see-through: true transparency ("clear") or OS acrylic ("frosted").
function glassOptions(forceAcrylic = false) {
  if ((forceAcrylic && HAS_ACRYLIC) || glass() === 'frosted') {
    return { transparent: false, backgroundMaterial: 'acrylic', backgroundColor: '#00000000' };
  }
  return { transparent: true, backgroundColor: '#00000000' };
}

function onScreen(pos, size) {
  if (!pos) return null;
  const d = screen.getDisplayMatching({ x: pos.x, y: pos.y, width: size.width, height: size.height });
  const a = d.workArea;
  const visible = pos.x + size.width > a.x + 40 && pos.x < a.x + a.width - 40
    && pos.y + size.height > a.y + 20 && pos.y < a.y + a.height - 20;
  return visible ? pos : null;
}

// ---------------------------------------------------------------- weather

function publish() {
  broadcast('sky:state', state);
  updateTray();
}

async function refresh() {
  if (refreshing) return refreshing;
  const loc = store.get('location');
  if (!loc) return undefined;
  refreshing = (async () => {
    clearTimeout(retryTimer);
    try {
      const next = await weather.fetchWeather(fetchFn, loc, { pointCache, previousAlerts: state.alerts });
      stateCache.set(loc.zip, next);
      if (store.get('zip') !== loc.zip) return; // the user switched places while this was loading
      state = next;
      notifyAlerts(next.alerts);
    } catch (e) {
      console.error('Refresh failed:', e.message);
      if (store.get('zip') !== loc.zip) return;
      state = state.current
        ? { ...state, stale: true, error: e.message }
        : { ...emptyState('error', e.message), location: { ...loc, timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone } };
      retryTimer = setTimeout(refresh, 60 * 1000);
    }
    publish();
  })().finally(() => {
    refreshing = null;
    // A switch during the fetch means the active place still needs its own refresh.
    if (store.get('zip') !== loc.zip) refresh();
  });
  return refreshing;
}

function scheduleRefresh() {
  clearInterval(refreshTimer);
  refreshTimer = setInterval(refresh, store.get('refreshMinutes') * 60 * 1000);
}

function notifyAlerts(alerts) {
  const seen = new Set(store.get('notifiedAlerts'));
  const fresh = alerts.filter((a) => !seen.has(a.id));
  if (!fresh.length) return;
  store.set({ notifiedAlerts: [...fresh.map((a) => a.id), ...store.get('notifiedAlerts')].slice(0, 80) });
  if (!store.get('alertNotifications') || !Notification.isSupported()) return;
  for (const a of fresh.slice(0, 3)) {
    const n = new Notification({ title: a.event, body: a.headline || a.areaDesc, icon: ICON });
    n.on('click', () => openDetail('alerts'));
    n.show();
  }
}

// ---------------------------------------------------------------- tray

function trayTooltip() {
  const c = state.current;
  if (!c || c.temp === null) return state.status === 'error' ? `${APP_NAME}: ${state.error}` : APP_NAME;
  const parts = [`${c.temp}° ${c.condition}`];
  if (state.today) parts.push(`H ${state.today.high ?? '--'}°  L ${state.today.low ?? '--'}°`);
  if (state.nextPrecip.headline) parts.push(state.nextPrecip.headline);
  if (state.alerts.length) parts.push(`⚠ ${state.alerts[0].event}${state.alerts.length > 1 ? ` +${state.alerts.length - 1}` : ''}`);
  // Windows limits tray tooltips to 127 characters.
  return parts.join('\n').slice(0, 127);
}

function updateTray() {
  if (!tray) return;
  tray.setToolTip(trayTooltip());
  const loc = state.location || store.get('location');
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: loc ? `${loc.city}, ${loc.state}` : APP_NAME, enabled: false },
    ...(store.get('locations').length > 1 ? [{
      label: 'Switch location',
      submenu: store.get('locations').map((l) => ({
        label: `${l.city}, ${l.state}  ${l.zip}`, type: 'radio', checked: l.zip === store.get('zip'),
        click: () => setActiveLocation(l.zip),
      })),
    }] : []),
    { type: 'separator' },
    { label: 'Open Skylight', click: () => openDetail('overview') },
    ...(state.alerts.length ? [{ label: `Alerts (${state.alerts.length})`, click: () => openDetail('alerts') }] : []),
    { label: 'Refresh now', click: () => refresh() },
    {
      label: 'Show desktop widget', type: 'checkbox', checked: store.get('showWidget'),
      click: (item) => applySettings({ showWidget: item.checked }),
    },
    {
      label: 'Reset widget and taskbar positions',
      click: () => { store.set({ widgetPos: null, pillPos: null }); rebuildGlassWindows(); placePill(); },
    },
    { label: 'Settings', click: () => openDetail('settings') },
    ...(updater && updater.getState().status === 'ready'
      ? [{ label: `Restart to update to ${updater.getState().newVersion}`, click: () => { quitting = true; updater.install(); } }] : []),
    { type: 'separator' },
    { label: 'Quit', click: () => quit() },
  ]));
}

function createTray() {
  const base = nativeImage.createFromPath(ICON);
  trayPlainImage = base.isEmpty() ? nativeImage.createEmpty() : multiRes(base);
  tray = new Tray(trayPlainImage);
  tray.on('click', () => openDetail('overview'));
  updateTray();
}

let trayNumberImage = null;
let trayPlainImage = null;

// Either/or: in 'tray' mode the tray icon is the temperature; in 'taskbar' mode the pill
// carries the weather and the tray icon is just the plain app icon (menu and quit live there).
function applyTrayImage() {
  if (!tray) return;
  const img = store.get('coveredMode') === 'tray' && trayNumberImage ? trayNumberImage : trayPlainImage;
  if (img) tray.setImage(img);
}

function multiRes(src) {
  const img = nativeImage.createEmpty();
  for (const scaleFactor of [1, 1.25, 1.5, 2]) {
    const px = Math.round(16 * scaleFactor);
    img.addRepresentation({ scaleFactor, width: px, height: px, buffer: src.resize({ width: px, height: px, quality: 'best' }).toPNG() });
  }
  return img;
}

function setTrayIcon(dataUrl) {
  if (!tray || typeof dataUrl !== 'string' || !dataUrl.startsWith('data:image/png')) return;
  const src = nativeImage.createFromDataURL(dataUrl);
  if (src.isEmpty()) return;
  trayNumberImage = multiRes(src);
  applyTrayImage();
}

// ---------------------------------------------------------------- windows

function createWidget() {
  const size = { width: 380, height: 420 };
  const area = screen.getPrimaryDisplay().workArea;
  const pos = onScreen(store.get('widgetPos'), size)
    || { x: area.x + area.width - size.width - 28, y: area.y + 28 };
  widget = new BrowserWindow({
    ...size, ...pos, ...glassOptions(),
    frame: false, resizable: false, minimizable: false, maximizable: false, fullscreenable: false,
    skipTaskbar: true, focusable: !IS_WIN ? true : false, hasShadow: false, show: false,
    title: APP_NAME, icon: ICON, webPreferences: webPreferences(),
  });
  sizes.set(widget, size);
  load(widget, 'widget.html');
  widget.once('ready-to-show', () => {
    if (store.get('showWidget')) showWidget();
  });
  // Win+D and friends can minimise it; a desktop widget should come straight back.
  widget.on('minimize', () => widget.restore());
  widget.on('closed', () => { widget = null; });
}

function showWidget() {
  if (!alive(widget)) return;
  widget.showInactive();
  if (native.available) native.sendToBottom(native.hwndOf(widget));
}

function taskbarGeometry() {
  // Returns where the pill belongs, in DIP: over the taskbar, just left of the tray icons.
  const d = screen.getPrimaryDisplay();
  const b = d.bounds;
  const a = d.workArea;
  const barHeight = b.y + b.height - (a.y + a.height);
  let trayLeft = b.x + b.width - 340;
  if (native.available) {
    try {
      const r = native.taskbarRects();
      if (r && r.tray) {
        const dip = screen.screenToDipRect(null, {
          x: r.tray.left, y: r.tray.top, width: r.tray.right - r.tray.left, height: r.tray.bottom - r.tray.top,
        });
        if (dip.x > b.x + b.width / 2) trayLeft = dip.x;
      }
    } catch (e) { /* fall back to the estimate above */ }
  }
  if (barHeight >= 30) {
    return { docked: true, height: barHeight, right: trayLeft - 8, y: a.y + a.height };
  }
  // Taskbar is hidden or on another edge: float in the bottom-right corner instead.
  return { docked: false, height: 44, right: a.x + a.width - 16, y: a.y + a.height - 44 - 12 };
}

function placePill() {
  if (!alive(pill)) return;
  const g = taskbarGeometry();
  const size = { width: (sizes.get(pill) || { width: 180 }).width, height: g.height };
  sizes.set(pill, size);
  const saved = onScreenAnywhere(store.get('pillPos'), size);
  const pos = saved || { x: Math.round(g.right - size.width), y: g.y };
  pill.setBounds({ ...pos, ...size });
}

function onScreenAnywhere(pos, size) {
  if (!pos) return null;
  const b = screen.getDisplayMatching({ ...pos, ...size }).bounds;
  const inside = pos.x >= b.x - 20 && pos.x + size.width <= b.x + b.width + 20
    && pos.y >= b.y - 20 && pos.y + size.height <= b.y + b.height + 20;
  return inside ? pos : null;
}

function createPill() {
  pill = new BrowserWindow({
    width: 180, height: 48, transparent: true, backgroundColor: '#00000000',
    frame: false, resizable: false, minimizable: false, maximizable: false, fullscreenable: false,
    skipTaskbar: true, focusable: !IS_WIN ? true : false, hasShadow: false, show: false, alwaysOnTop: true,
    title: `${APP_NAME} taskbar`, webPreferences: webPreferences(),
  });
  pill.setAlwaysOnTop(true, 'screen-saver');
  load(pill, 'pill.html');
  pill.on('closed', () => { pill = null; });
  placePill();
}

function setPillVisible(visible) {
  if (!alive(pill) || pill.isVisible() === visible) return;
  if (visible) {
    placePill();
    pill.showInactive();
  } else {
    pill.hide();
  }
}

function openDetail(section = 'overview') {
  if (!store.get('location')) return openSetup();
  if (!state.updatedAt || Date.now() - Date.parse(state.updatedAt) > 60 * 1000) refresh();
  if (alive(detail)) {
    if (detail.isMinimized()) detail.restore();
    detail.show();
    detail.focus();
    detail.webContents.send('sky:navigate', section);
    return undefined;
  }
  detail = new BrowserWindow({
    width: 1000, height: 700, minWidth: 780, minHeight: 560,
    backgroundColor: '#121722', // solid: app windows never show the desktop through them
    frame: false, show: false, title: APP_NAME, icon: ICON, webPreferences: webPreferences(),
  });
  load(detail, 'detail.html', { section });
  detail.once('ready-to-show', () => {
    detail.show();
    detail.webContents.send('sky:navigate', section);
  });
  detail.on('closed', () => { detail = null; });
  return undefined;
}

function openSetup() {
  if (alive(setup)) { setup.show(); setup.focus(); return; }
  setup = new BrowserWindow({
    width: 520, height: 640, resizable: false, maximizable: false, fullscreenable: false,
    backgroundColor: '#121722', frame: false, show: false, center: true,
    title: `Welcome to ${APP_NAME}`, icon: ICON, webPreferences: webPreferences(),
  });
  load(setup, 'setup.html', { acrylic: HAS_ACRYLIC ? '1' : '0' });
  setup.once('ready-to-show', () => setup.show());
  setup.on('closed', () => { setup = null; });
}

function rebuildGlassWindows() {
  // The glass style is baked into the window when it is created, so recreate them.
  if (alive(widget)) { widget.removeAllListeners('closed'); widget.destroy(); widget = null; }
  createWidget();
}

function startApp() {
  if (!alive(widget)) createWidget();
  if (!alive(pill)) createPill();
  scheduleRefresh();
  refresh();
  startTicking();
}

// ---------------------------------------------------------------- desktop / taskbar switching

let coveredVotes = 0;
let covered = false;
let desktopWasForeground = false;

function isFullscreen(rect) {
  if (!rect) return false;
  return screen.getAllDisplays().some((d) => {
    const p = screen.dipToScreenRect(null, d.bounds);
    return rect.left <= p.x && rect.top <= p.y && rect.right >= p.x + p.width && rect.bottom >= p.y + p.height;
  });
}

function tick() {
  if (!alive(widget) || !alive(pill)) return;
  const wantPillMode = store.get('coveredMode') === 'taskbar';
  const widgetOn = store.get('showWidget');
  let snap;
  try {
    snap = native.snapshot(native.hwndOf(widget));
  } catch (e) {
    console.error('Window check failed:', e.message);
    return;
  }
  // Keep the widget at desktop level: show it when the desktop itself is in front
  // (for example after Win+D), and tuck it under other apps as soon as one is used.
  if (widgetOn && widget.isVisible()) {
    if (snap.desktopForeground && !desktopWasForeground) native.raise(native.hwndOf(widget));
    if (!snap.desktopForeground && desktopWasForeground && !snap.fgIsOurs) native.sendToBottom(native.hwndOf(widget));
  }
  desktopWasForeground = snap.desktopForeground;

  const nowCovered = !widgetOn || !widget.isVisible() || (!snap.desktopForeground && snap.coverage >= COVERED_AT);
  // Two matching readings in a row before switching, so dragging a window past does not flicker.
  coveredVotes = nowCovered === covered ? 0 : coveredVotes + 1;
  if (coveredVotes >= 2) { covered = nowCovered; coveredVotes = 0; }

  const fullscreenApp = !snap.desktopForeground && !snap.fgIsOurs && isFullscreen(snap.fgRect);
  const showPill = covered && wantPillMode && !fullscreenApp;
  setPillVisible(showPill);
  if (showPill) native.keepOnTop(native.hwndOf(pill)); // the taskbar likes to climb back over it
}

function startTicking() {
  clearInterval(tickTimer);
  if (native.available) {
    tickTimer = setInterval(tick, TICK_MS);
  } else if (process.env.SKYLIGHT_SHOW_PILL) {
    setPillVisible(true); // development aid on non-Windows platforms
  }
}

// ---------------------------------------------------------------- settings

async function applySettings(patch) {
  const clean = sanitizePatch(patch);
  const before = { ...store.publicSettings() };
  store.set(clean);
  const after = store.publicSettings();
  if (after.launchAtLogin !== before.launchAtLogin) applyLoginItem();
  if (after.refreshMinutes !== before.refreshMinutes) scheduleRefresh();
  if (after.glass !== before.glass) {
    rebuildGlassWindows();
    if (alive(pill)) load(pill, 'pill.html');
  }
  if (after.showWidget !== before.showWidget && alive(widget)) {
    if (after.showWidget) showWidget(); else widget.hide();
  }
  if (after.coveredMode !== before.coveredMode) { applyTrayImage(); if (after.coveredMode === 'tray') setPillVisible(false); }
  broadcast('sky:settings', after);
  updateTray();
  return after;
}

function publishLocations() {
  broadcast('sky:locations', store.publicLocations());
  broadcast('sky:settings', store.publicSettings());
  updateTray();
}

function setActiveLocation(zip) {
  const loc = store.get('locations').find((l) => l.zip === zip);
  if (!loc) return;
  store.set({ activeZip: zip });
  // Show what we already know for this place straight away, then fetch fresh data.
  state = stateCache.get(zip) || { ...emptyState('loading'), location: { ...loc, timeZone: 'UTC' } };
  publish();
  publishLocations();
  refresh();
}

async function addLocation(zipInput) {
  const found = await weather.lookupZip(fetchFn, zipInput);
  if (!found.ok) return found;
  const { ok, ...location } = found;
  const rest = store.get('locations').filter((l) => l.zip !== location.zip);
  if (rest.length >= 12) return { ok: false, error: 'You can keep up to 12 locations. Remove one first.' };
  store.set({ locations: [...rest, location] });
  setActiveLocation(location.zip);
  return { ok: true, place: { zip: location.zip, city: location.city, state: location.state } };
}

function removeLocation(zip) {
  const list = store.get('locations');
  if (list.length <= 1 || !list.some((l) => l.zip === zip)) return;
  const wasActive = store.get('zip') === zip;
  const rest = list.filter((l) => l.zip !== zip);
  store.set({ locations: rest });
  stateCache.delete(zip);
  if (wasActive) setActiveLocation(rest[0].zip); else publishLocations();
}

function applyLoginItem() {
  // Only register the real installed app, never a development copy of Electron.
  if (app.isPackaged && (IS_WIN || process.platform === 'darwin')) {
    app.setLoginItemSettings({ openAtLogin: !!store.get('launchAtLogin'), args: ['--hidden'] });
  }
}

function quit() {
  quitting = true;
  app.quit();
}

// ---------------------------------------------------------------- IPC

const senderWindow = (e) => BrowserWindow.fromWebContents(e.sender);

ipcMain.handle('sky:getState', () => state);
ipcMain.handle('sky:getSettings', () => store.publicSettings());
ipcMain.handle('sky:setSettings', (_e, patch) => applySettings(patch));
ipcMain.handle('sky:getLocations', () => store.publicLocations());
ipcMain.handle('sky:addLocation', (_e, zip) => addLocation(zip));
ipcMain.handle('sky:removeLocation', (_e, zip) => removeLocation(zip));
ipcMain.handle('sky:setActiveLocation', (_e, zip) => setActiveLocation(zip));
ipcMain.handle('sky:lookupZip', (_e, zip) => weather.lookupZip(fetchFn, zip));
ipcMain.handle('sky:refresh', () => refresh());
ipcMain.handle('sky:completeSetup', async (_e, v) => {
  const found = await weather.lookupZip(fetchFn, v && v.zip);
  if (!found.ok) return { ok: false, error: found.error };
  const { ok, ...location } = found;
  store.set({ locations: [location], activeZip: location.zip, ...sanitizePatch({ glass: v.glass }) });
  applyLoginItem();
  state = emptyState('loading');
  startApp();
  if (alive(setup)) setup.close();
  broadcast('sky:settings', store.publicSettings());
  return { ok: true };
});
ipcMain.on('sky:openDetail', (_e, section) => {
  const allowed = ['overview', 'hourly', 'daily', 'alerts', 'settings'];
  openDetail(allowed.includes(section) ? section : 'overview');
});
ipcMain.on('sky:windowControl', (e, action) => {
  const w = senderWindow(e);
  if (!w || (w !== detail && w !== setup)) return;
  if (action === 'close') {
    // Closing setup before finishing it leaves nothing running, so quit.
    if (w === setup && !store.get('location')) quit(); else w.close();
  } else if (action === 'minimize') w.minimize();
  else if (action === 'maximize') { if (w.isMaximized()) w.unmaximize(); else w.maximize(); }
});
ipcMain.on('sky:moveBy', (e, dx, dy) => {
  const w = senderWindow(e);
  if (!w || (w !== widget && w !== pill) || !Number.isFinite(dx) || !Number.isFinite(dy)) return;
  const size = sizes.get(w) || w.getBounds();
  let d = drags.get(w);
  if (!d) { const b = w.getBounds(); d = { x: b.x, y: b.y }; drags.set(w, d); }
  d.x += dx; d.y += dy;
  // Always pass the intended size: moving by position alone lets the size drift on scaled displays.
  w.setBounds({ x: Math.round(d.x), y: Math.round(d.y), width: size.width, height: size.height });
});
ipcMain.on('sky:dragEnd', (e) => {
  const w = senderWindow(e);
  if (!w || !drags.has(w)) return;
  drags.delete(w);
  const b = w.getBounds();
  if (w === widget) store.set({ widgetPos: { x: b.x, y: b.y } });
  if (w === pill) store.set({ pillPos: { x: b.x, y: b.y } });
});
ipcMain.on('sky:setContentSize', (e, width, height) => {
  const w = senderWindow(e);
  if (!w || !Number.isFinite(width) || !Number.isFinite(height)) return;
  if (w === widget) {
    const size = {
      width: Math.max(140, Math.min(900, Math.ceil(width))),
      height: Math.max(60, Math.min(900, Math.ceil(height))),
    };
    sizes.set(w, size);
    const b = w.getBounds();
    if (b.height === size.height && b.width === size.width) return;
    // A widget parked on the right half of the screen keeps its right edge when its width changes.
    const area = screen.getDisplayMatching(b).workArea;
    const rightSide = b.x + b.width / 2 > area.x + area.width / 2;
    w.setBounds({ x: rightSide ? b.x + b.width - size.width : b.x, y: b.y, ...size });
  } else if (w === pill) {
    const prev = sizes.get(w) || { height: 48 };
    const next = { width: Math.max(80, Math.min(360, Math.ceil(width))), height: prev.height };
    if (prev.width === next.width) return;
    sizes.set(w, next);
    const b = w.getBounds();
    // Keep the right edge fixed so the pill grows toward the middle of the taskbar.
    const x = store.get('pillPos') ? b.x : b.x + b.width - next.width;
    w.setBounds({ x, y: b.y, ...next });
  }
});
ipcMain.on('sky:setTrayIcon', (_e, dataUrl) => setTrayIcon(dataUrl));
ipcMain.on('sky:quit', () => quit());
ipcMain.handle('sky:getUpdateState', () => updater.getState());
ipcMain.handle('sky:checkForUpdates', () => updater.check());
ipcMain.on('sky:installUpdate', () => { quitting = true; updater.install(); });

// ---------------------------------------------------------------- lifecycle

app.whenReady().then(() => {
  store = new Store(app.getPath('userData'));
  Menu.setApplicationMenu(null);
  let announced = null;
  updater = createUpdater((u) => {
    broadcast('sky:updateState', u);
    if (u.status === 'ready' && announced !== u.newVersion && Notification.isSupported()) {
      announced = u.newVersion;
      const n = new Notification({ title: `${APP_NAME} ${u.newVersion} is ready`, body: 'It installs the next time Skylight restarts. Click to restart now.', icon: ICON });
      n.on('click', () => { quitting = true; updater.install(); });
      n.show();
    }
    updateTray();
  });
  createTray();
  if (store.get('location')) {
    state = { ...emptyState('loading'), location: { ...store.get('location'), timeZone: 'UTC' } };
    startApp();
    if (!process.argv.includes('--hidden') && !app.getLoginItemSettings().wasOpenedAtLogin && !store.get('showWidget')) {
      openDetail('overview');
    }
  } else {
    openSetup();
  }
  powerMonitor.on('resume', () => setTimeout(refresh, 4000));
  screen.on('display-metrics-changed', () => { placePill(); });
});

// Tray app: keep running when windows close.
app.on('window-all-closed', () => { if (quitting) app.quit(); });
app.on('before-quit', () => { quitting = true; });
