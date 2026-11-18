/* Skylight desktop widget */
(function () {
  'use strict';
  var U = window.SkyUI, I = window.SkyIcons, esc = U.esc;
  var root = document.getElementById('root');
  var state = null, settings = null, places = { locations: [], activeZip: null };
  var lastSig = '', lastTray = '', first = true, refreshing = false;
  var LAYOUTS = ['full', 'compact', 'wide', 'minimal'];
  function layout() { return settings && LAYOUTS.indexOf(settings.layout) >= 0 ? settings.layout : 'full'; }

  function alertBanner(s) {
    var a = s.alerts && s.alerts[0];
    if (!a) return '';
    var tz = s.location && s.location.timeZone;
    var until = a.ends || a.expires;
    var sub = until ? 'Until ' + U.when(until, tz, s.updatedAt) : (a.headline || 'Active now');
    if (a.onset && s.updatedAt && new Date(a.onset) > new Date(s.updatedAt)) sub = 'From ' + U.when(a.onset, tz, s.updatedAt) + (until ? ' to ' + U.when(until, tz, s.updatedAt) : '');
    var more = s.alerts.length > 1 ? '<span class="w-alert-more">+' + (s.alerts.length - 1) + '</span>' : '';
    return '<div class="w-alert sev-' + U.sev(a) + '" data-open="alerts" role="button" aria-label="' + esc(a.event) + ', open alerts">' +
      '<span class="w-alert-ic">' + I.gl('alert') + '</span>' +
      '<span class="w-alert-tx"><div class="w-alert-ev" title="' + esc(a.event) + '">' + esc(a.event) + '</div><div class="w-alert-sub">' + esc(sub) + '</div></span>' +
      more + '<span class="w-alert-chev">' + I.gl('chevron-right') + '</span></div>';
  }

  function refreshBtn(s) {
    var busy = refreshing || s.status === 'loading';
    return '<span class="w-ref' + (busy ? ' is-busy' : '') + '" data-action="refresh" role="button" aria-label="Refresh now" aria-disabled="' + busy + '" title="Refresh now">' + I.gl('refresh') + '</span>';
  }

  function placeDots() {
    var ls = places.locations || [];
    if (ls.length < 2) return '';
    var idx = Math.max(0, ls.findIndex(function (p) { return p.zip === places.activeZip; }));
    if (ls.length > 6) return '<span class="w-dots"><small>' + (idx + 1) + '/' + ls.length + '</small></span>';
    return '<span class="w-dots">' + ls.map(function (p, i) { return '<i' + (i === idx ? ' class="on"' : '') + '></i>'; }).join('') + '</span>';
  }

  function topRow(s, short) {
    var tz = s.location && s.location.timeZone;
    var loc = s.location ? esc(s.location.city) + ', ' + esc(s.location.state) : 'Skylight';
    var dots = placeDots();
    var upd;
    if (s.stale) upd = '<span class="w-upd is-stale" title="' + esc(s.error || '') + '"><i class="w-dot"></i>Offline' + (short ? '' : ' · as of ' + esc(U.time(s.updatedAt, tz))) + '</span>';
    else if (refreshing || s.status === 'loading') upd = '<span class="w-upd">Updating…</span>';
    else upd = '<span class="w-upd">' + (s.updatedAt ? (short ? '' : 'Updated ') + esc(U.time(s.updatedAt, tz)) : '') + '</span>';
    return '<div class="sec w-top" data-open="overview"><div class="w-loc"' + (dots ? ' data-action="next-place" role="button" title="' + loc + ' \u2013 click to switch location" aria-label="' + loc + ', switch location"' : ' title="' + loc + '"') + '>' +
      I.gl('pin') + '<span>' + loc + '</span>' + dots + '</div><div class="w-right">' + upd + refreshBtn(s) + '</div></div>';
  }

  var UP = '<svg class="w-arrow" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 9V1.500M1.8 4.5 5 1.300l3.2 3.2"/></svg>';
  var DOWN = '<svg class="w-arrow" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 1v7.500M1.8 5.5 5 8.700l3.2-3.2"/></svg>';

  function hero(s) {
    var c = s.current || {}, t = s.today || {};
    var temp = c.temp == null ? '–' : Math.round(c.temp);
    var meta = '<span class="w-hl" title="Today’s high">' + UP + '<b>' + U.deg(t.high) + '</b></span>' +
      '<span class="w-hl" title="Today’s low">' + DOWN + '<b>' + U.deg(t.low) + '</b></span>';
    if (c.feelsLike != null) meta += '<i class="w-meta-sep"></i><span>Feels like <b>' + U.deg(c.feelsLike) + '</b></span>';
    return '<div class="sec w-hero" data-open="overview">' +
      '<div class="w-temp">' + temp + (c.temp == null ? '' : '<sup>°</sup>') + '</div>' +
      '<div class="w-icon">' + I.wx(c.icon || 'cloudy', c.isDay !== false, { animated: true }) + '</div>' +
      '<div class="w-cond">' + esc(c.condition || '') + '</div>' +
      '<div class="w-meta">' + meta + '</div></div>';
  }

  function incoming(s, noTimeline) {
    var np = s.nextPrecip;
    if (!np || !np.headline) return '';
    var tz = s.location && s.location.timeZone;
    var tl = np.kind && !noTimeline ? U.precipTimeline(s.minutely, tz, { height: 30 }) : '';
    var sub = '';
    if (np.kind && !np.isNow && np.endsAt) sub = 'until ' + U.hour(np.endsAt, tz);
    else if (np.kind && np.isNow && tl) sub = 'next 6 hours';
    if (noTimeline) sub = '';
    return '<div class="sec w-inc ' + (np.kind ? 'k-' + np.kind : 'is-dry') + '" data-open="overview">' +
      '<div class="w-inc-row"><span class="w-inc-ic">' + I.gl(np.kind ? U.kindGlyph(np.kind) : 'umbrella') + '</span><span>' + esc(np.headline) + '</span>' +
      (sub ? '<span class="w-inc-sub">' + esc(sub) + '</span>' : '') + '</div>' + tl + '</div>';
  }

  function hours(s) {
    var hs = (s.hourly || []).slice(0, 6);
    if (!hs.length) return '';
    var tz = s.location && s.location.timeZone;
    var anyPop = hs.some(function (h) { return h.precipProb != null && h.precipProb >= 15; });
    return '<div class="sec w-hours' + (anyPop ? '' : ' no-pop') + '" data-open="hourly">' + hs.map(function (h, i) {
      var p = h.precipProb != null && h.precipProb >= 15 ? h.precipProb + '%' : '';
      return '<div class="w-h"><span class="w-h-t">' + (i === 0 ? 'Now' : esc(U.hour(h.time, tz))) + '</span>' + I.wx(h.icon, h.isDay) +
        '<span class="w-h-v">' + U.deg(h.temp) + '</span><span class="w-h-p">' + p + '</span></div>';
    }).join('') + '</div>';
  }

  function days(s) {
    var ds = (s.daily || []).slice(0, 5);
    if (!ds.length) return '';
    var lo = Infinity, hi = -Infinity;
    ds.forEach(function (d) { if (d.low != null) lo = Math.min(lo, d.low); if (d.high != null) hi = Math.max(hi, d.high); });
    var cur = s.current && s.current.temp;
    if (cur != null) { lo = Math.min(lo, cur); hi = Math.max(hi, cur); }
    var span = Math.max(1, hi - lo);
    return '<div class="sec w-days" data-open="daily">' + ds.map(function (d, i) {
      var bar = '';
      if (d.low != null && d.high != null) {
        var l = ((d.low - lo) / span) * 100, r = ((hi - d.high) / span) * 100;
        bar = '<i style="left:' + l.toFixed(1) + '%;right:' + r.toFixed(1) + '%;background:linear-gradient(90deg,' + U.tempColor(d.low) + ',' + U.tempColor(d.high) + ')"></i>';
        if (i === 0 && cur != null) bar += '<b style="left:' + (((cur - lo) / span) * 100).toFixed(1) + '%"></b>';
      }
      var p = d.precipProb != null && d.precipProb >= 20 ? d.precipProb + '%' : '';
      return '<div class="w-d"><span class="w-d-n">' + esc(d.name) + '</span>' + I.wx(d.icon, true) + '<span class="w-d-p">' + p + '</span>' +
        '<span class="w-d-lo">' + U.deg(d.low) + '</span><span class="range">' + bar + '</span><span class="w-d-hi">' + U.deg(d.high) + '</span></div>';
    }).join('') + '</div>';
  }

  function loadingView(s) {
    var where = s.location ? esc(s.location.city) + ', ' + esc(s.location.state) : 'Skylight';
    /* transparent style and small layouts: no skeleton blocks, just a quiet line */
    if (document.documentElement.dataset.glass === 'none' || layout() !== 'full') {
      return '<div class="w-wait" data-open="overview">' + I.wx('partly-cloudy', true) + '<div><b>' + where + '</b><span>' + I.gl('refresh', 'spin') + 'Getting the latest forecast…</span></div></div>';
    }
    var bar = function (w, h, extra) { return '<span class="skel" style="width:' + w + 'px;height:' + h + 'px;' + (extra || '') + '"></span>'; };
    return '<div class="w-skel" data-open="overview">' +
      '<div class="w-skel-row">' + bar(120, 14) + bar(70, 11) + '</div>' +
      '<div class="w-skel-row" style="margin-top:18px">' + bar(132, 72, 'border-radius:14px') + bar(76, 76, 'border-radius:50%') + '</div>' +
      bar(150, 16, 'margin-top:16px') + bar(210, 12, 'margin-top:10px') +
      '<div class="w-skel-row" style="margin-top:22px">' + [0, 1, 2, 3, 4, 5].map(function () { return bar(38, 62, 'border-radius:10px'); }).join('') + '</div>' +
      '<div class="w-skel-note">' + I.gl('refresh', 'spin') + 'Getting the latest forecast…</div></div>';
  }

  function errorView(s) {
    return '<div class="w-state" data-open="overview">' +
      '<span class="w-state-ring">' + I.gl('offline') + '</span>' +
      '<h2>Weather unavailable</h2>' +
      '<p>' + esc(s.error || 'Something went wrong while getting the forecast.') + '</p>' +
      '<div class="w-btns"><button class="w-btn is-primary" data-action="refresh">' + (refreshing ? 'Trying…' : 'Try again') + '</button><button class="w-btn" data-open="settings">Settings</button></div></div>';
  }

  function minimal(s) {
    var c = s.current, t = s.today || {}, a = s.alerts && s.alerts[0];
    return '<div class="w-min" data-open="overview">' +
      '<div class="w-icon">' + I.wx(c.icon || 'cloudy', c.isDay !== false, { animated: true }) + '</div>' +
      '<div class="w-temp">' + (c.temp == null ? '–' : Math.round(c.temp) + '<sup>°</sup>') + '</div>' +
      '<div class="w-min-hl"><span class="w-hl" title="Today\u2019s high">' + UP + U.deg(t.high) + '</span><span class="w-hl" title="Today\u2019s low">' + DOWN + U.deg(t.low) + '</span></div>' +
      '<div class="w-min-side">' + (a ? '<span class="w-min-alert sev-' + U.sev(a) + '" data-open="alerts" role="button" title="' + esc(a.event) + '" aria-label="' + esc(a.event) + '">' + I.gl('alert') + '</span>' : '') + refreshBtn(s) + '</div></div>';
  }

  function render() {
    var s = state;
    if (!s) return;
    var lay = layout(), html;
    if (s.status === 'error' && !s.current) html = errorView(s);
    else if (!s.current) html = loadingView(s);
    else if (lay === 'minimal') html = minimal(s);
    else if (lay === 'compact') html = alertBanner(s) + topRow(s, true) + hero(s) + incoming(s, true);
    else if (lay === 'wide') {
      html = alertBanner(s) + '<div class="w-wide"><div class="w-col">' + topRow(s, true) + hero(s) + '</div>' +
        '<div class="w-col w-col-mid">' + incoming(s) + hours(s) + '</div><div class="w-col w-col-days">' + days(s) + '</div></div>';
    } else html = alertBanner(s) + topRow(s) + hero(s) + incoming(s) + hours(s) + days(s);
    var sig = lay + '|' + html;
    /* skip DOM churn when nothing visible changed (keeps icon animation phase too) */
    if (sig !== lastSig) {
      lastSig = sig;
      root.className = 'w lay-' + lay + (first ? '' : ' no-anim');
      root.innerHTML = '<div class="w-panel">' + html + '</div>';
      first = !s.current;
    }
    reportSize();
    drawTray(s);
  }

  function reportSize() {
    var r = root.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) window.sky.setContentSize(Math.ceil(r.width), Math.ceil(r.height));
  }

  /* 64x64 tray icon: temperature digits, soft dark outline, alert dot */
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
      /* digits only: at 16-24 px a degree sign costs more legibility than it adds */
      var text = temp, F = '"Segoe UI Variable Display","Segoe UI",system-ui,sans-serif';
      g.font = '700 60px ' + F;
      var size = Math.min(64, Math.floor(60 * 58 / g.measureText(text).width));
      g.font = '700 ' + size + 'px ' + F;
      g.textAlign = 'center'; g.textBaseline = 'alphabetic';
      var m = g.measureText(text);
      var asc = m.actualBoundingBoxAscent, desc = m.actualBoundingBoxDescent;
      /* stretch vertically (max 1.35x) so the number fills the square */
      var sy = Math.max(1, Math.min(1.35, 54 / (asc + desc)));
      g.save();
      g.translate(32, 32); g.scale(1, sy);
      var y = (asc - desc) / 2;
      g.strokeStyle = 'rgba(0,0,0,0.62)'; g.lineWidth = 7; g.strokeText(text, 0, y);
      g.fillStyle = '#fff'; g.fillText(text, 0, y);
      g.restore();
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
  });

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
