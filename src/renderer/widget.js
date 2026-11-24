/* WeathaDock desktop widget: hosts the shared renderer (widget-view.js), reports its size,
   handles drag / clicks and keeps the tray icon current. */
(function () {
  'use strict';
  var U = window.SkyUI, W = window.WDWidget;
  var root = document.getElementById('root');
  var state = null, settings = null, places = { locations: [], activeZip: null };
  var first = true, refreshing = false;
  var darkText = false; /* text colour "Auto": whether the wallpaper behind the widget calls for dark text */
  var ready = false;    /* nothing is drawn before the settings and that answer are in */
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
    if (!s || !ready) return;
    if (s.current) shown = s;
    else if (s.status === 'loading' && shown) { s = shown; pending = true; }
    root.classList.toggle('no-anim', !first);
    root.classList.toggle('is-locked', !!(settings && settings.lockPosition));
    var changed = W.mount(root, s, config(), { glass: document.documentElement.dataset.glass, places: places, refreshing: refreshing || pending, pending: pending, darkText: darkText });
    if (changed) { W.fit(root, MAX_W, MAX_H); if (s.current) first = false; }
    reportSize();
    U.tray(s);
  }

  function reportSize() {
    var r = root.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) window.sky.setContentSize(Math.min(MAX_W, Math.ceil(r.width)), Math.min(MAX_H, Math.ceil(r.height)));
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
    /* Main looks at the wallpaper behind the widget and says when the answer changes. The first
       answer is waited for (half a second at most), so the text does not start in one colour
       and change to the other; whatever main has said since then is the newer word. */
    var told = false;
    if (sky.onBackdrop) sky.onBackdrop(function (b) { told = true; darkText = !!(b && b.dark); render(); });
    var backdrop = !sky.getBackdrop ? null : Promise.race([
      sky.getBackdrop().catch(function () { return null; }),
      new Promise(function (done) { setTimeout(done, 500, null); })
    ]);
    Promise.all([sky.getSettings().catch(function () { return null; }), sky.getLocations ? sky.getLocations().catch(function () { return null; }) : null, backdrop]).then(function (r) {
      settings = r[0]; if (r[1]) places = r[1];
      if (r[2] && !told) darkText = !!r[2].dark;
      ready = true;
      return sky.getState();
    }).then(function (s) { state = s; render(); });
    new ResizeObserver(reportSize).observe(root);
  });
})();
