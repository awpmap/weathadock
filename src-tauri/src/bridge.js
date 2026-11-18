// Injected into every WeathaDock window before its own scripts run. It provides the
// `window.sky` API described in docs/UI-CONTRACT.md on top of Tauri commands and events.
(function () {
  if (window.sky) return;
  var internals = window.__TAURI_INTERNALS__;
  var api = window.__TAURI__;
  if (!internals && !api) return; // not running inside the app (design preview in a browser)
  var invoke = api && api.core ? api.core.invoke : internals.invoke;
  var listen = api && api.event ? api.event.listen : function (event, handler) {
    return invoke('plugin:event|listen', { event: event, target: { kind: 'Any' }, handler: internals.transformCallback(handler) });
  };
  var on = function (name) { return function (cb) { listen(name, function (e) { cb(e.payload); }); }; };
  var call = function (cmd, args) { return invoke(cmd, args).catch(function (e) { console.error(cmd, e); }); };

  window.sky = {
    getState: function () { return invoke('get_state'); },
    onState: on('sky:state'),
    getSettings: function () { return invoke('get_settings'); },
    onSettings: on('sky:settings'),
    setSettings: function (patch) { return invoke('set_settings', { patch: patch }); },
    getLocations: function () { return invoke('get_locations'); },
    onLocations: on('sky:locations'),
    addLocation: function (zip) { return invoke('add_location', { zip: String(zip) }); },
    removeLocation: function (zip) { return invoke('remove_location', { zip: String(zip) }); },
    setActiveLocation: function (zip) { return invoke('set_active_location', { zip: String(zip) }); },
    lookupZip: function (zip) { return invoke('lookup_zip', { zip: String(zip) }); },
    completeSetup: function (v) { return invoke('complete_setup', { zip: String((v && v.zip) || ''), glass: String((v && v.glass) || 'none') }); },
    refresh: function () { return invoke('refresh_now'); },
    openDetail: function (section) { call('open_detail_cmd', { section: section || 'overview' }); },
    onNavigate: function (cb) {
      listen('sky:navigate', function (e) { cb(e.payload); });
      // The section requested when this window was opened, if any.
      invoke('take_pending_section').then(function (s) { if (s) cb(s); }).catch(function () {});
    },
    windowControl: function (action) { call('window_control', { action: action }); },
    moveBy: function (dx, dy) { call('move_by', { dx: dx, dy: dy }); },
    dragEnd: function () { call('drag_end'); },
    setContentSize: function (w, h) { call('set_content_size', { width: w, height: h }); },
    setTrayIcon: function (dataUrl) { call('set_tray_icon', { dataUrl: dataUrl }); },
    showContextMenu: function () { call('show_context_menu'); },
    getDock: function () { return invoke('get_dock'); },
    onDock: on('sky:dock'),
    getBattery: function () { return invoke('get_battery'); },
    onBattery: on('sky:battery'),
    getUpdateState: function () { return invoke('get_update_state'); },
    onUpdateState: on('sky:updateState'),
    checkForUpdates: function () { return invoke('check_for_updates'); },
    installUpdate: function () { call('install_update'); },
    quit: function () { call('quit_cmd'); },
  };

  var page = location.pathname.split('/').pop();

  // Title bars: the pages mark them with `-webkit-app-region: drag`, which this webview does
  // not act on, so start a native window drag from the same elements.
  if (page === 'detail.html' || page === 'setup.html') {
    var bar = function (t) { return t && t.closest && t.closest('.tb, .su-bar'); };
    var control = function (t) { return t.closest('button, a, input, select, textarea, .winctl, [role="button"], [data-no-drag]'); };
    document.addEventListener('mousedown', function (e) {
      if (e.button !== 0 || !bar(e.target) || control(e.target)) return;
      if (e.detail === 2) { if (page === 'detail.html') call('window_control', { action: 'maximize' }); return; }
      call('window_drag');
    }, true);
  }

  if (page === 'widget.html') {
    // Whole-widget opacity is a setting; apply it here so the page itself stays unaware.
    var applyOpacity = function (s) {
      var v = s && Number(s.widgetOpacity);
      document.documentElement.style.opacity = v >= 30 && v < 100 ? String(v / 100) : '';
    };
    invoke('get_settings').then(applyOpacity).catch(function () {});
    listen('sky:settings', function (e) { applyOpacity(e.payload); });

    // People who picked a ready-made layout in version 1.0 keep it: translate it once into
    // the customizable widget settings.
    window.addEventListener('DOMContentLoaded', function () {
      invoke('get_settings').then(function (s) {
        if (!s || s.widgetSet) return;
        var presets = window.WD_PRESETS || [];
        var match = null;
        for (var i = 0; i < presets.length; i++) if (presets[i].id === s.layout) match = presets[i];
        return invoke('set_settings', { patch: { widget: match ? match.widget : s.widget, layout: match ? match.id : 'full' } });
      }).catch(function () {});
    });
  }

  // A native app should not show the browser's page menu.
  document.addEventListener('contextmenu', function (e) {
    var t = e.target;
    if (t && t.closest && t.closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
  });
})();
