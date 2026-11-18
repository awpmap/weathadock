'use strict';
// Tiny JSON settings store in the user's app-data folder.
const fs = require('fs');
const path = require('path');

const DEFAULTS = {
  version: 2,
  locations: [],             // [{ zip, city, state, lat, lon }]
  activeZip: null,
  glass: 'none',             // widget style: 'none' (fully transparent) | 'clear' (tinted) | 'frosted'
  layout: 'full',            // widget layout: 'full' | 'compact' | 'wide' | 'minimal'
  coveredMode: 'tray',       // while apps cover the widget: 'tray' (temperature in the tray icon) or 'taskbar' (pill)
  showWidget: true,
  launchAtLogin: true,
  alertNotifications: true,
  refreshMinutes: 10,
  widgetPos: null,           // { x, y }
  pillPos: null,             // { x, y } once the user drags the taskbar pill
  notifiedAlerts: [],
};
const PUBLIC_KEYS = ['zip', 'glass', 'layout', 'coveredMode', 'showWidget', 'launchAtLogin', 'alertNotifications', 'refreshMinutes'];

class Store {
  constructor(dir) {
    this.file = path.join(dir, 'settings.json');
    this.data = { ...DEFAULTS };
    try {
      Object.assign(this.data, JSON.parse(fs.readFileSync(this.file, 'utf8')));
    } catch { /* first run or unreadable file: start from defaults */ return; }
    this.migrate();
  }
  migrate() {
    const d = this.data;
    if (!Array.isArray(d.locations)) d.locations = [];
    // Version 1 stored a single `location` and defaulted to tinted glass.
    if (d.location && !d.locations.length) {
      d.locations = [d.location];
      d.activeZip = d.location.zip;
    }
    if ('location' in d) { d.glass = 'none'; d.coveredMode = 'tray'; }
    delete d.location;
    delete d.zip;
    d.version = 2;
  }
  get(key) {
    if (key === 'location') {
      const list = this.data.locations;
      return list.find((l) => l.zip === this.data.activeZip) || list[0] || null;
    }
    if (key === 'zip') { const l = this.get('location'); return l ? l.zip : null; }
    return this.data[key];
  }
  set(patch) {
    Object.assign(this.data, patch);
    try {
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      const tmp = `${this.file}.tmp`;
      fs.writeFileSync(tmp, JSON.stringify(this.data, null, 2));
      fs.renameSync(tmp, this.file);
    } catch (e) {
      console.error('Could not save settings:', e.message);
    }
  }
  publicLocations() {
    return {
      locations: this.data.locations.map(({ zip, city, state }) => ({ zip, city, state })),
      activeZip: this.get('zip'),
    };
  }
  publicSettings() {
    return Object.fromEntries(PUBLIC_KEYS.map((k) => [k, this.get(k)]));
  }
}

function sanitizePatch(patch) {
  const out = {};
  if (!patch || typeof patch !== 'object') return out;
  if (['none', 'clear', 'frosted'].includes(patch.glass)) out.glass = patch.glass;
  if (['taskbar', 'tray'].includes(patch.coveredMode)) out.coveredMode = patch.coveredMode;
  for (const k of ['showWidget', 'launchAtLogin', 'alertNotifications']) {
    if (typeof patch[k] === 'boolean') out[k] = patch[k];
  }
  if ([5, 10, 15, 30].includes(patch.refreshMinutes)) out.refreshMinutes = patch.refreshMinutes;
  if (['full', 'compact', 'wide', 'minimal'].includes(patch.layout)) out.layout = patch.layout;
  return out;
}

module.exports = { Store, sanitizePatch };
