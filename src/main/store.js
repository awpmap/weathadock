'use strict';
// Tiny JSON settings store in the user's app-data folder.
const fs = require('fs');
const path = require('path');

const DEFAULTS = {
  zip: null,
  location: null,            // { zip, city, state, lat, lon }
  glass: 'clear',            // 'clear' | 'frosted'
  coveredMode: 'taskbar',    // 'taskbar' | 'tray'
  showWidget: true,
  launchAtLogin: true,
  alertNotifications: true,
  refreshMinutes: 10,
  widgetPos: null,           // { x, y }
  pillPos: null,             // { x, y } once the user drags the taskbar pill
  notifiedAlerts: [],
};
const PUBLIC_KEYS = ['zip', 'glass', 'coveredMode', 'showWidget', 'launchAtLogin', 'alertNotifications', 'refreshMinutes'];

class Store {
  constructor(dir) {
    this.file = path.join(dir, 'settings.json');
    this.data = { ...DEFAULTS };
    try {
      Object.assign(this.data, JSON.parse(fs.readFileSync(this.file, 'utf8')));
    } catch { /* first run or unreadable file: start from defaults */ }
  }
  get(key) { return this.data[key]; }
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
  publicSettings() {
    return Object.fromEntries(PUBLIC_KEYS.map((k) => [k, this.data[k]]));
  }
}

function sanitizePatch(patch) {
  const out = {};
  if (!patch || typeof patch !== 'object') return out;
  if (['clear', 'frosted'].includes(patch.glass)) out.glass = patch.glass;
  if (['taskbar', 'tray'].includes(patch.coveredMode)) out.coveredMode = patch.coveredMode;
  for (const k of ['showWidget', 'launchAtLogin', 'alertNotifications']) {
    if (typeof patch[k] === 'boolean') out[k] = patch[k];
  }
  if ([5, 10, 15, 30].includes(patch.refreshMinutes)) out.refreshMinutes = patch.refreshMinutes;
  if (typeof patch.zip === 'string' && /^\d{5}$/.test(patch.zip)) out.zip = patch.zip;
  return out;
}

module.exports = { Store, sanitizePatch };
