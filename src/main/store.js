'use strict';
// Tiny JSON settings store in the user's app-data folder.
const fs = require('fs');
const path = require('path');

// Everything the desktop widget can show, and how it is arranged. The ready-made layouts are
// just presets of this object; the Settings window can change every field.
const WIDGET_DEFAULTS = {
  orientation: 'vertical',   // 'vertical' | 'horizontal'
  size: 'medium',            // 'small' | 'medium' | 'large' | 'xlarge'
  align: 'left',             // 'left' | 'center' | 'right'
  ink: 'light',              // text colour: 'light' for dark wallpapers, 'dark' for bright ones
  hours: 6,                  // 4 | 6 | 8 | 12
  days: 5,                   // 3 | 5 | 7
  show: {
    location: true, updated: true, refresh: true, icon: true, temp: true, condition: true,
    highLow: true, feelsLike: true, incoming: true, timeline: true, hourly: true, daily: true,
    alerts: true, wind: false, humidity: false, uv: false, sun: false,
  },
};
const WIDGET_ENUMS = {
  orientation: ['vertical', 'horizontal'], size: ['small', 'medium', 'large', 'xlarge'],
  align: ['left', 'center', 'right'], ink: ['light', 'dark'], hours: [4, 6, 8, 12], days: [3, 5, 7],
};

function sanitizeWidget(input, base = WIDGET_DEFAULTS) {
  const out = { ...base, show: { ...base.show } };
  if (!input || typeof input !== 'object') return out;
  for (const [key, allowed] of Object.entries(WIDGET_ENUMS)) {
    if (allowed.includes(input[key])) out[key] = input[key];
  }
  if (input.show && typeof input.show === 'object') {
    for (const key of Object.keys(WIDGET_DEFAULTS.show)) {
      if (typeof input.show[key] === 'boolean') out.show[key] = input.show[key];
    }
  }
  return out;
}

const DEFAULTS = {
  version: 2,
  locations: [],             // [{ zip, city, state, lat, lon }]
  activeZip: null,
  glass: 'none',             // widget style: 'none' (fully transparent) | 'clear' (tinted) | 'frosted'
  layout: 'full',            // which preset the widget started from, or 'custom' once edited
  widget: sanitizeWidget(null),
  coveredMode: 'tray',       // while apps cover the widget: 'tray' (temperature in the tray icon) or 'taskbar' (pill)
  showWidget: true,
  launchAtLogin: true,
  alertNotifications: true,
  refreshMinutes: 10,
  widgetPos: null,           // { x, y }
  pillPos: null,             // { x, y } once the user drags the taskbar pill
  notifiedAlerts: [],
};
const PUBLIC_KEYS = ['zip', 'glass', 'layout', 'widget', 'coveredMode', 'showWidget', 'launchAtLogin', 'alertNotifications', 'refreshMinutes'];

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
    d.widget = sanitizeWidget(d.widget);
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

function sanitizePatch(patch, current) {
  const out = {};
  if (!patch || typeof patch !== 'object') return out;
  if (['none', 'clear', 'frosted'].includes(patch.glass)) out.glass = patch.glass;
  if (['taskbar', 'tray'].includes(patch.coveredMode)) out.coveredMode = patch.coveredMode;
  for (const k of ['showWidget', 'launchAtLogin', 'alertNotifications']) {
    if (typeof patch[k] === 'boolean') out[k] = patch[k];
  }
  if ([5, 10, 15, 30].includes(patch.refreshMinutes)) out.refreshMinutes = patch.refreshMinutes;
  if (typeof patch.layout === 'string' && /^[a-z-]{1,24}$/.test(patch.layout)) out.layout = patch.layout;
  if (patch.widget && typeof patch.widget === 'object') out.widget = sanitizeWidget(patch.widget, current && current.widget);
  return out;
}

module.exports = { Store, sanitizePatch, sanitizeWidget, WIDGET_DEFAULTS };
