/* WeathaDock desktop widget: hosts the shared renderer (widget-view.js), reports its size,
   handles drag / clicks and draws the tray icon. */
(function () {
  'use strict';
  var U = window.SkyUI, W = window.WDWidget;
  var root = document.getElementById('root');
  var state = null, settings = null, places = { locations: [], activeZip: null };
  var lastTray = '', first = true, refreshing = false;
  var MAX_W = 900, MAX_H = 900;

  function config() {
    var s = settings || {};
    return W.normalize(s.widget || W.presetConfig(s.layout) || W.presetConfig('full'));
  }

  /* While a new place or the first refresh is loading, keep showing the last forecast (dimmed)
     instead of collapsing to the loading view: no jump, no resize. */
  var shown = null;
  function render() {
    var s = state, pending = false;
    if (!s) return;
    if (s.current) shown = s;
    else if (s.status === 'loading' && shown) { s = shown; pending = true; }
    root.classList.toggle('no-anim', !first);
    root.classList.toggle('is-locked', !!(settings && settings.lockPosition));
    var changed = W.mount(root, s, config(), { glass: document.documentElement.dataset.glass, places: places, refreshing: refreshing || pending, pending: pending });
    if (changed) { W.fit(root, MAX_W, MAX_H); if (s.current) first = false; }
    reportSize();
    drawTray(s);
  }

  function reportSize() {
    var r = root.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) window.sky.setContentSize(Math.min(MAX_W, Math.ceil(r.width)), Math.min(MAX_H, Math.ceil(r.height)));
  }

  /* 64x64 tray icon: temperature on a sky-blue tile, alert dot */
  function drawTray(s) {
    var temp = s.current && s.current.temp != null ? String(Math.round(s.current.temp)) : null;
    var a = s.alerts && s.alerts[0];
    var sig = temp + '|' + (a ? U.sev(a) : '');
    if (sig === lastTray) return;
    lastTray = sig;
    var c = document.createElement('canvas');
    c.width = c.height = 64;
    var g = c.getContext('2d');
    g.lineJoin = 'round'; g.lineCap = 'round';
    if (temp != null) {
      /* A sky-blue tile in the app icon's colors, so the number reads as weather and is not
         mistaken for the bare CPU or GPU temperatures that hardware monitors put in the tray. */
      var sky = g.createLinearGradient(8, 0, 56, 64);
      sky.addColorStop(0, '#4FB0FF'); sky.addColorStop(0.55, '#2A7FEF'); sky.addColorStop(1, '#1A52D2');
      g.beginPath(); g.roundRect(1, 1, 62, 62, 15);
      g.fillStyle = sky; g.fill();
      var text = temp, F = '"Segoe UI Variable Display","Segoe UI",system-ui,sans-serif';
      /* a degree mark when there is room for it (one or two digits, no alert dot) */
      var deg = text.length <= 2 && !a;
      var room = deg ? 44 : 54;
      g.font = '700 60px ' + F;
      var size = Math.min(56, Math.floor(60 * room / g.measureText(text).width));
      g.font = '700 ' + size + 'px ' + F;
      g.textAlign = 'center'; g.textBaseline = 'alphabetic';
      var m = g.measureText(text);
      var asc = m.actualBoundingBoxAscent, desc = m.actualBoundingBoxDescent;
      /* stretch vertically (max 1.3x) so the number fills the tile */
      var sy = Math.max(1, Math.min(1.3, 44 / (asc + desc)));
      var half = m.width / 2, cx = deg ? 28 : 32;
      g.save();
      g.translate(cx, 33); g.scale(1, sy);
      g.fillStyle = '#fff'; g.fillText(text, 0, (asc - desc) / 2);
      g.restore();
      if (deg) {
        g.beginPath(); g.arc(Math.min(55, cx + half + 6), 17, 4.5, 0, Math.PI * 2);
        g.strokeStyle = '#fff'; g.lineWidth = 3.5; g.stroke();
      }
    } else {
      g.beginPath();
      g.moveTo(18, 48); g.lineTo(44, 48);
      g.arc(44, 37, 11, Math.PI / 2, -Math.PI / 2.6, true);
      g.arc(31, 28, 14, -0.25, Math.PI * 1.05, true);
      g.arc(18, 38, 10, -Math.PI / 2, Math.PI / 2, true);
      g.closePath();
      g.strokeStyle = 'rgba(0,0,0,0.55)'; g.lineWidth = 6; g.stroke();
      g.fillStyle = '#fff'; g.fill();
    }
    if (a) {
      g.beginPath(); g.arc(54, 10, 9, 0, Math.PI * 2);
      g.fillStyle = U.sevColor(U.sev(a)); g.fill();
      g.strokeStyle = 'rgba(0,0,0,0.65)'; g.lineWidth = 3; g.stroke();
    }
    try { window.sky.setTrayIcon(c.toDataURL('image/png')); } catch (e) {}
  }

  function doRefresh() {
    if (refreshing) return;
    refreshing = true; render();
    Promise.resolve(window.sky.refresh()).catch(function () {}).then(function () { refreshing = false; render(); });
  }
  function nextPlace() {
    var ls = places.locations || [];
    if (ls.length < 2) return;
    var idx = ls.findIndex(function (p) { return p.zip === places.activeZip; });
    var next = ls[(idx + 1) % ls.length];
    places = { locations: ls, activeZip: next.zip }; /* optimistic, main confirms through onLocations */
    render();
    window.sky.setActiveLocation(next.zip);
  }

  U.dragOrClick(root, function (target) {
    var act = target.closest && target.closest('[data-action]');
    if (act && act.dataset.action === 'refresh') { doRefresh(); return; }
    if (act && act.dataset.action === 'next-place') { nextPlace(); return; }
    var el = target.closest && target.closest('[data-open]');
    window.sky.openDetail(el ? el.dataset.open : 'overview');
  }, function () { return !!(settings && settings.lockPosition); });

  W.onMetrics = function () { root.__wvSig = null; render(); };
  if (U.params.get('guides') === '1') root.classList.add('wv-guides'); /* design aid */

  U.watchVisibility();

  U.ready().then(function (sky) {
    sky.onState(function (s) { state = s; render(); });
    sky.onSettings(function (s) { settings = s; render(); });
    if (sky.onLocations) sky.onLocations(function (v) { places = v || places; render(); });
    Promise.all([sky.getSettings().catch(function () { return null; }), sky.getLocations ? sky.getLocations().catch(function () { return null; }) : null]).then(function (r) {
      settings = r[0]; if (r[1]) places = r[1];
      return sky.getState();
    }).then(function (s) { state = s; render(); });
    new ResizeObserver(reportSize).observe(root);
  });
})();
