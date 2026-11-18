'use strict';
// In-app updates through electron-updater and GitHub Releases.
// Only the installed (NSIS) build can update itself; a development run or the plain zip
// reports 'unsupported' and the UI explains that.
const { app } = require('electron');
const fs = require('fs');
const path = require('path');

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

function createUpdater(onChange) {
  const state = {
    version: app.getVersion(), status: 'unsupported', newVersion: null, percent: null, error: null, checkedAt: null,
  };
  const set = (patch) => { Object.assign(state, patch); onChange({ ...state }); };
  const supported = app.isPackaged && fs.existsSync(path.join(process.resourcesPath, 'app-update.yml'));
  let autoUpdater = null;

  if (supported) {
    try {
      ({ autoUpdater } = require('electron-updater'));
      autoUpdater.autoDownload = true;
      autoUpdater.autoInstallOnAppQuit = true;
      state.status = 'idle';
      autoUpdater.on('checking-for-update', () => set({ status: 'checking', error: null }));
      autoUpdater.on('update-available', (info) => set({ status: 'downloading', newVersion: info.version, percent: 0 }));
      autoUpdater.on('update-not-available', () => set({ status: 'up-to-date', newVersion: null, checkedAt: new Date().toISOString() }));
      autoUpdater.on('download-progress', (p) => set({ status: 'downloading', percent: Math.round(p.percent) }));
      autoUpdater.on('update-downloaded', (info) => set({ status: 'ready', newVersion: info.version, percent: 100, checkedAt: new Date().toISOString() }));
      autoUpdater.on('error', (e) => set({ status: 'error', error: String((e && e.message) || e).split('\n')[0].slice(0, 200) }));
    } catch (e) {
      autoUpdater = null;
      console.error('Updater unavailable:', e.message);
    }
  }

  async function check() {
    if (!autoUpdater) return { ...state };
    if (['checking', 'downloading', 'ready'].includes(state.status)) return { ...state };
    try {
      await autoUpdater.checkForUpdates();
    } catch (e) { /* reported through the 'error' event */ }
    return { ...state };
  }

  function install() {
    if (autoUpdater && state.status === 'ready') autoUpdater.quitAndInstall(true, true);
  }

  if (autoUpdater) {
    setTimeout(check, 15 * 1000);
    setInterval(check, CHECK_EVERY_MS);
  }
  return { getState: () => ({ ...state }), check, install };
}

module.exports = { createUpdater };
