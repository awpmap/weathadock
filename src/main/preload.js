'use strict';
const { contextBridge, ipcRenderer } = require('electron');

const on = (channel) => (cb) => { ipcRenderer.on(channel, (_e, value) => cb(value)); };

contextBridge.exposeInMainWorld('sky', {
  getState: () => ipcRenderer.invoke('sky:getState'),
  onState: on('sky:state'),
  getSettings: () => ipcRenderer.invoke('sky:getSettings'),
  onSettings: on('sky:settings'),
  setSettings: (patch) => ipcRenderer.invoke('sky:setSettings', patch),
  lookupZip: (zip) => ipcRenderer.invoke('sky:lookupZip', zip),
  completeSetup: (v) => ipcRenderer.invoke('sky:completeSetup', v),
  refresh: () => ipcRenderer.invoke('sky:refresh'),
  openDetail: (section) => ipcRenderer.send('sky:openDetail', section),
  onNavigate: on('sky:navigate'),
  windowControl: (action) => ipcRenderer.send('sky:windowControl', action),
  moveBy: (dx, dy) => ipcRenderer.send('sky:moveBy', dx, dy),
  dragEnd: () => ipcRenderer.send('sky:dragEnd'),
  setContentSize: (w, h) => ipcRenderer.send('sky:setContentSize', w, h),
  setTrayIcon: (dataUrl) => ipcRenderer.send('sky:setTrayIcon', dataUrl),
  quit: () => ipcRenderer.send('sky:quit'),
});
