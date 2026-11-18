/* WeathaDock detail window */
(function () {
  'use strict';
  var U = window.SkyUI, I = window.SkyIcons, esc = U.esc;
  var $ = function (id) { return document.getElementById(id); };
  var SECTIONS = ['overview', 'hourly', 'daily', 'alerts', 'widget', 'settings'];
  var state = null, settings = null, current = 'overview';
  var places = { locations: [], activeZip: null }, update = null, refreshing = false;
  var openDays = null;            // Set of expanded daily dates
  var main = $('main'), tip = $('tip');

  /* ---------- static chrome ---------- */
  document.querySelectorAll('[data-gl]').forEach(function (el) { el.innerHTML = I.gl(el.dataset.gl); });
  document.querySelectorAll('[data-wx]').forEach(function (el) { el.outerHTML = I.wx(el.dataset.wx, true); });
  $('tb-logo').innerHTML = I.wx('partly-cloudy', true);
  document.querySelectorAll('[data-win]').forEach(function (b) {
    b.innerHTML = I.gl('win-' + (b.dataset.win === 'minimize' ? 'min' : b.dataset.win === 'maximize' ? 'max' : 'close'));
    b.addEventListener('click', function () { window.sky.windowControl(b.dataset.win); });
  });

  function tz() { return state && state.location && state.location.timeZone; }
  function nowMs() { return window.sky.__mock && state && state.updatedAt ? new Date(state.updatedAt).getTime() + 120000 : Date.now(); }
  function hasData() { return !!(state && state.current); }

  /* ---------- navigation ---------- */
  function go(sec, focus) {
    if (SECTIONS.indexOf(sec) < 0) sec = 'overview';
    current = sec;
    document.querySelectorAll('.nav-i').forEach(function (b) {
      var on = b.dataset.sec === sec;
      b.setAttribute('aria-selected', on ? 'true' : 'false');
      b.tabIndex = 0;
    });
    SECTIONS.forEach(function (s) { $('sec-' + s).hidden = s !== sec; });
    hideTip();
    main.scrollTop = 0;
    renderSection();
  }
  document.querySelectorAll('.nav-i').forEach(function (b) { b.addEventListener('click', function () { go(b.dataset.sec); }); });
  main.addEventListener('click', function (e) {
    var g = e.target.closest('[data-go]');
    if (g) { go(g.dataset.go); return; }
    var a = e.target.closest('[data-action]');
    if (a && a.dataset.action === 'refresh') doRefresh();
  });

  function syncRefresh() {
    var busy = refreshing || (state && state.status === 'loading');
    [$('btn-refresh'), $('set-refresh')].forEach(function (b) { b.disabled = !!busy; b.classList.toggle('is-busy', !!busy); });
    $('btn-refresh-tx').textContent = busy ? 'Refreshing…' : 'Refresh';
  }
  function doRefresh() {
    if (refreshing) return;
    refreshing = true; syncRefresh();
    Promise.resolve(window.sky.refresh()).catch(function () {}).then(function () { refreshing = false; syncRefresh(); });
  }
  $('btn-refresh').addEventListener('click', doRefresh);

  /* keyboard: Ctrl+R refresh, Ctrl+1..5 sections, Esc closes an open menu */
  var KEY_SECTIONS = ['overview', 'hourly', 'daily', 'alerts', 'settings'];
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      if (!locMenu.hidden) { closeMenu(); locBtn.focus(); e.preventDefault(); }
      return;
    }
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    if (e.key === 'r' || e.key === 'R') { e.preventDefault(); doRefresh(); }
    else if (e.key >= '1' && e.key <= '5') { e.preventDefault(); go(KEY_SECTIONS[Number(e.key) - 1]); }
  });

  /* ---------- location switcher (header menu) ---------- */
  var locBtn = $('head-loc'), locMenu = $('loc-menu');
  function closeMenu() { locMenu.hidden = true; locBtn.setAttribute('aria-expanded', 'false'); }
  function openMenu() {
    var ls = places.locations || [];
    locMenu.innerHTML = '<div class="menu-h">Locations</div>' + ls.map(function (p) {
      return '<button class="menu-i" role="menuitemradio" aria-checked="' + (p.zip === places.activeZip) + '" data-zip="' + esc(p.zip) + '">' + I.gl('check', 'gl-check') + '<span>' + esc(p.city) + ', ' + esc(p.state) + '</span><small>' + esc(p.zip) + '</small></button>';
    }).join('') + '<div class="menu-sep"></div><button class="menu-i" role="menuitem" data-manage="1">' + I.gl('plus') + '<span>Add or manage locations</span></button>';
    locMenu.hidden = false; locBtn.setAttribute('aria-expanded', 'true');
    var f = locMenu.querySelector('[aria-checked="true"]') || locMenu.querySelector('.menu-i');
    if (f) f.focus();
  }
  locBtn.addEventListener('click', function () { if (locMenu.hidden) openMenu(); else closeMenu(); });
  locMenu.addEventListener('click', function (e) {
    var b = e.target.closest('.menu-i'); if (!b) return;
    closeMenu();
    if (b.dataset.manage) { go('settings'); setTimeout(function () { $('zip').focus(); }, 60); }
    else if (b.dataset.zip !== places.activeZip) pickPlace(b.dataset.zip);
  });
  locMenu.addEventListener('keydown', function (e) {
    var items = [].slice.call(locMenu.querySelectorAll('.menu-i')), i = items.indexOf(document.activeElement);
    if (e.key === 'ArrowDown') { e.preventDefault(); items[(i + 1) % items.length].focus(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); items[(i + items.length - 1) % items.length].focus(); }
    else if (e.key === 'Escape') { closeMenu(); locBtn.focus(); }
  });
  document.addEventListener('pointerdown', function (e) { if (!locMenu.hidden && !e.target.closest('.head-place')) closeMenu(); });
  function pickPlace(zip) {
    places = { locations: places.locations, activeZip: zip }; /* optimistic; main confirms via onLocations */
    renderPlaces(); renderHead();
    Promise.resolve(window.sky.setActiveLocation(zip)).catch(function () {});
  }

  /* ---------- header ---------- */
  function renderHead() {
    var s = state;
    var loc = s && s.location;
    var act = (places.locations || []).filter(function (p) { return p.zip === places.activeZip; })[0];
    if (!loc && act) loc = act;
    $('head-loc').innerHTML = I.gl('pin') + (loc ? '<span>' + esc(loc.city) + ', ' + esc(loc.state) + '</span><small>' + esc(loc.zip) + '</small>' : '<span>Choose a location</span>') + I.gl('chevron-down', 'head-chev');
    var upd = '';
    if (s) {
      if (s.status === 'loading' || refreshing) upd = 'Updating…';
      else if (s.stale) upd = '<span class="is-stale">Offline · as of ' + esc(U.when(s.updatedAt, tz())) + '</span>';
      else if (s.updatedAt) upd = 'Updated ' + esc(U.time(s.updatedAt, tz()));
    }
    $('head-upd').innerHTML = upd;
    syncRefresh();
    var n = $('notice');
    if (s && s.stale && s.current) { n.hidden = false; n.innerHTML = I.gl('offline') + '<span>' + esc(s.error || 'Couldn\u2019t refresh the forecast.') + ' Last updated ' + esc(U.when(s.updatedAt, tz())) + '.</span>'; }
    else n.hidden = true;
    var badge = $('nav-badge'), al = (s && s.alerts) || [];
    badge.hidden = !al.length;
    if (al.length) { badge.textContent = al.length; badge.className = 'nav-badge sev-' + U.sev(al[0]); }
  }

  /* ---------- shared fragments ---------- */
  function placeholder() {
    var s = state;
    if (s && s.status === 'error') {
      return '<div class="empty"><span class="empty-ic is-muted">' + I.gl('offline') + '</span><h2>Weather unavailable</h2><p>' + esc(s.error || 'Something went wrong while getting the forecast.') + '</p>' +
        '<div style="display:flex;gap:8px"><button class="btn is-primary" data-action="refresh">' + I.gl('refresh') + 'Try again</button><button class="btn" data-go="settings">Check settings</button></div></div>';
    }
    var b = function (w, h, st) { return '<span class="skel" style="width:' + w + ';height:' + h + 'px;' + (st || '') + '"></span>'; };
    return '<div class="pg-skel" aria-busy="true" aria-label="Loading forecast">' +
      '<div style="display:flex;justify-content:space-between;align-items:center;padding:10px 4px 26px">' +
      '<div>' + b('190px', 96, 'border-radius:18px') + b('170px', 20, 'margin-top:18px') + b('250px', 14, 'margin-top:10px') + '</div>' + b('128px', 128, 'border-radius:50%') + '</div>' +
      '<div class="grid-2">' + b('100%', 132, 'border-radius:14px') + b('100%', 132, 'border-radius:14px') + '</div>' +
      '<div class="grid-3">' + b('100%', 96, 'border-radius:14px') + b('100%', 96, 'border-radius:14px') + b('100%', 96, 'border-radius:14px') + '</div></div>';
  }

  function alertWhen(a) {
    var t = tz(), now = new Date(nowMs()).toISOString();
    var end = a.ends || a.expires;
    var future = a.onset && new Date(a.onset).getTime() > nowMs();
    if (future) return 'From <b>' + esc(U.when(a.onset, t, now)) + '</b>' + (end ? ' until <b>' + esc(U.when(end, t, now)) + '</b>' : '');
    return end ? 'In effect until <b>' + esc(U.when(end, t, now)) + '</b>' : 'In effect now';
  }

  function rangeBar(d, lo, hi, cur) {
    if (d.low == null || d.high == null) return '<span class="range"></span>';
    var span = Math.max(1, hi - lo);
    var html = '<i style="left:' + (((d.low - lo) / span) * 100).toFixed(1) + '%;right:' + (((hi - d.high) / span) * 100).toFixed(1) + '%;background:linear-gradient(90deg,' + U.tempColor(d.low) + ',' + U.tempColor(d.high) + ')"></i>';
    if (cur != null) html += '<b style="left:' + (Math.max(0, Math.min(1, (cur - lo) / span)) * 100).toFixed(1) + '%"></b>';
    return '<span class="range">' + html + '</span>';
  }
  function weekRange(ds, cur) {
    var lo = Infinity, hi = -Infinity;
    ds.forEach(function (d) { if (d.low != null) lo = Math.min(lo, d.low); if (d.high != null) hi = Math.max(hi, d.high); });
    if (cur != null) { lo = Math.min(lo, cur); hi = Math.max(hi, cur); }
    return [lo, hi];
  }

  /* ---------- overview ---------- */
  var DIRS = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW'];
  function compass(dir) {
    var i = DIRS.indexOf(String(dir || '').toUpperCase());
    if (i < 0) return '';
    /* arrow points the way the wind is blowing (from `dir` toward the opposite side) */
    return '<svg class="compass" viewBox="0 0 34 34" aria-hidden="true"><circle cx="17" cy="17" r="15.5" fill="none" stroke="rgba(255,255,255,.16)"/>' +
      '<path d="M17 1.500v3M17 29.500v3M1.5 17h3M29.5 17h3" stroke="rgba(255,255,255,.3)" stroke-linecap="round"/>' +
      '<g transform="rotate(' + (i * 22.5 + 180) + ' 17 17)"><path d="M17 7.500l4.2 8.500h-3v10.500h-2.400V16h-3z" fill="#7cc0ff"/></g></svg>';
  }
  function windWord(v) {
    if (v == null) return '';
    return v < 1 ? 'Calm' : v < 8 ? 'Light breeze' : v < 13 ? 'Gentle breeze' : v < 19 ? 'Moderate breeze' : v < 25 ? 'Fresh breeze' : v < 32 ? 'Strong breeze' : v < 39 ? 'Near gale' : 'Gale';
  }
  function uvWord(v) { return v == null ? '' : v < 3 ? 'Low' : v < 6 ? 'Moderate' : v < 8 ? 'High' : v < 11 ? 'Very high' : 'Extreme'; }
  function visWord(v) { return v == null ? '' : v >= 10 ? 'Clear view' : v >= 6 ? 'Good' : v >= 3 ? 'Moderate' : v >= 1 ? 'Poor' : 'Very poor'; }
  function tile(gl, name, value, unit, sub, extra) {
    return '<div class="card tile"><div class="tile-h">' + I.gl(gl) + name + '</div><div class="tile-v">' + value + (unit ? '<small>' + unit + '</small>' : '') + (extra || '') + '</div><div class="tile-s">' + (sub || '&nbsp;') + '</div></div>';
  }

  function sunCard(s) {
    var t = s.today || {};
    if (!t.sunrise || !t.sunset) return '';
    var r = new Date(t.sunrise).getTime(), e = new Date(t.sunset).getTime(), n = nowMs();
    var f = (n - r) / (e - r);
    var pt = function (k) { return [(120 - 100 * Math.cos(Math.PI * k)), (66 - 54 * Math.sin(Math.PI * k))]; };
    var path = function (a, b) {
      var d = '';
      for (var i = 0; i <= 40; i++) { var p = pt(a + ((b - a) * i) / 40); d += (i ? 'L' : 'M') + p[0].toFixed(1) + ' ' + p[1].toFixed(1); }
      return d;
    };
    var up = f > 0 && f < 1;
    var dot = '';
    if (up) { var p = pt(f); dot = '<circle cx="' + p[0].toFixed(1) + '" cy="' + p[1].toFixed(1) + '" r="11" fill="#ffc94a" opacity=".2"/><circle cx="' + p[0].toFixed(1) + '" cy="' + p[1].toFixed(1) + '" r="6" fill="#ffcb52" stroke="rgba(16,20,28,.8)" stroke-width="2"/>'; }
    var mins = Math.round((e - r) / 60000);
    var left = up ? Math.round((e - n) / 60000) : 0;
    var note = up ? (Math.floor(left / 60) ? Math.floor(left / 60) + ' h ' : '') + (left % 60) + ' min of daylight left' : (n < r ? 'Sun rises at ' + U.time(t.sunrise, tz()) : 'The sun has set');
    return '<div class="card card-pad sun"><div class="card-h"><span>' + I.gl('sunrise') + 'Sun</span><span style="font-weight:400;color:var(--t3)">' + Math.floor(mins / 60) + ' h ' + (mins % 60) + ' min day</span></div>' +
      '<svg viewBox="0 0 240 76" role="img" aria-label="' + esc(note) + '">' +
      '<path d="' + path(0, 1) + '" fill="none" stroke="rgba(255,255,255,.2)" stroke-width="1.5" stroke-dasharray="1 5" stroke-linecap="round"/>' +
      (up ? '<path d="' + path(0, f) + '" fill="none" stroke="#ffc94a" stroke-width="2" stroke-linecap="round"/>' : '') +
      '<line x1="6" y1="66" x2="234" y2="66" stroke="rgba(255,255,255,.16)"/>' + dot + '</svg>' +
      '<div class="sun-row"><div>Sunrise<b>' + esc(U.time(t.sunrise, tz())) + '</b></div><div class="sun-len">' + esc(note) + '</div><div>Sunset<b>' + esc(U.time(t.sunset, tz())) + '</b></div></div></div>';
  }

  function incomingCard(s) {
    var np = s.nextPrecip || {};
    var t = tz();
    var tl = np.kind ? U.precipTimeline(s.minutely, t, { height: 46 }) : '';
    var sub = '';
    if (np.kind) {
      var parts = [];
      if (!np.isNow && np.startsAt) parts.push('Starts around ' + U.time(np.startsAt, t));
      if (np.endsAt) parts.push((np.isNow ? 'Expected to end around ' : 'ends around ') + U.time(np.endsAt, t));
      sub = parts.join(', ');
      var total = (s.minutely || []).reduce(function (a, m) { return a + (m.precip || 0); }, 0);
      if (total > 0.005) sub += (sub ? ' · ' : '') + '≈ ' + total.toFixed(2) + '″ ' + (np.kind === 'snow' ? 'liquid equivalent ' : '') + 'in the next 6 h';
    } else sub = 'No rain or snow expected.';
    var body = tl || (np.kind ? '' : '<div class="inc-dry"></div><div class="inc-dry-ticks"><span>Now</span><span>+6 h</span><span>+12 h</span></div>');
    return '<div class="card card-pad inc ' + (np.kind ? 'k-' + np.kind : 'is-dry') + '"><div class="card-h"><span>' + I.gl('umbrella') + 'Incoming weather</span>' +
      '<button class="link" data-go="hourly">Hourly' + I.gl('chevron-right') + '</button></div>' +
      '<div class="inc-head"><span class="inc-ic">' + I.gl(np.kind ? U.kindGlyph(np.kind) : 'check') + '</span><span>' + esc(np.headline || 'No precipitation data') + '</span></div>' +
      '<div class="inc-sub">' + esc(sub) + '</div>' + body + '</div>';
  }

  function renderOverview() {
    var s = state, c = s.current, t = s.today || {}, zone = tz();
    var html = '';
    if (s.alerts && s.alerts.length) {
      html += '<div class="ov-alerts">' + s.alerts.map(function (a) {
        return '<button class="ov-alert sev-' + U.sev(a) + '" data-go="alerts"><span class="ov-alert-ic">' + I.gl('alert') + '</span><span class="ov-alert-tx"><div class="ov-alert-ev">' + esc(a.event) + '</div><div class="ov-alert-sub">' + alertWhen(a).replace(/<\/?b>/g, '') + ' · ' + esc(a.areaDesc || a.sender || '') + '</div></span>' + I.gl('chevron-right') + '</button>';
      }).join('') + '</div>';
    }
    html += '<div class="ov-hero"><div><div class="ov-temp">' + (c.temp == null ? '–' : Math.round(c.temp) + '<sup>°</sup>') + '</div>' +
      '<div class="ov-cond">' + esc(c.condition || '') + '</div>' +
      '<div class="ov-meta"><span>High <b>' + U.deg(t.high) + '</b></span><span>Low <b>' + U.deg(t.low) + '</b></span>' + (c.feelsLike != null ? '<i></i><span>Feels like <b>' + U.deg(c.feelsLike) + '</b></span>' : '') + '</div></div>' +
      '<div class="ov-icon">' + I.wx(c.icon, c.isDay, { animated: true }) + '</div>' +
      (t.summary ? '<p class="ov-sum">' + esc(t.summary) + '</p>' : '') + '</div>';

    html += '<div class="grid-2">' + incomingCard(s) + sunCard(s) + '</div>';

    var gust = c.windGust != null && c.windGust > (c.windSpeed || 0) ? 'Gusts to ' + c.windGust + ' mph' : windWord(c.windSpeed);
    var uvPos = c.uvIndex == null ? 0 : Math.min(100, (c.uvIndex / 11) * 100);
    html += '<div class="grid-3">' +
      tile('wind', 'Wind', U.num(c.windSpeed), 'mph ' + esc(c.windDir || ''), esc(gust), compass(c.windDir)) +
      tile('humidity', 'Humidity', U.num(c.humidity), '%', c.dewPoint != null ? 'Dew point ' + U.deg(c.dewPoint) : '') +
      tile('sunny', 'UV index', U.num(c.uvIndex), esc(uvWord(c.uvIndex)), '<div class="uv-bar"><i style="left:' + uvPos.toFixed(0) + '%"></i></div>') +
      tile('gauge', 'Pressure', c.pressure == null ? '–' : Number(c.pressure).toFixed(2), 'inHg', c.pressure == null ? '' : c.pressure >= 30.2 ? 'High pressure' : c.pressure <= 29.7 ? 'Low pressure' : 'Near normal') +
      tile('eye', 'Visibility', U.num(c.visibility), 'mi', esc(visWord(c.visibility))) +
      tile('thermo', 'Feels like', U.deg(c.feelsLike), '', c.feelsLike == null || c.temp == null ? '' : c.feelsLike < c.temp - 2 ? 'Wind makes it feel colder' : c.feelsLike > c.temp + 2 ? 'Humidity makes it feel warmer' : 'Close to the actual temperature') +
      '</div>';

    var hs = (s.hourly || []).slice(0, 12);
    if (hs.length) {
      html += '<div class="card card-pad stack is-link" data-go="hourly" style="cursor:pointer"><div class="card-h"><span>' + I.gl('clock') + 'Next 12 hours</span><span class="link">All 48 hours' + I.gl('chevron-right') + '</span></div><div class="strip">' +
        hs.map(function (h, i) {
          return '<div class="strip-h"><span class="strip-t">' + (i === 0 ? 'Now' : esc(U.hour(h.time, zone))) + '</span>' + I.wx(h.icon, h.isDay) + '<span class="strip-v">' + U.deg(h.temp) + '</span><span class="strip-p">' + (h.precipProb >= 15 ? h.precipProb + '%' : '') + '</span></div>';
        }).join('') + '</div></div>';
    }
    var ds = s.daily || [];
    if (ds.length) {
      html += '<div class="card card-pad stack is-link" data-go="daily" style="cursor:pointer"><div class="card-h"><span>' + I.gl('calendar') + 'This week</span><span class="link">7-day forecast' + I.gl('chevron-right') + '</span></div><div class="strip">' +
        ds.map(function (d) {
          return '<div class="week-d"><span class="week-n">' + esc(d.name) + '</span>' + I.wx(d.icon, true) + '<span class="week-hl">' + U.deg(d.high) + '<span>' + U.deg(d.low) + '</span></span><span class="strip-p">' + (d.precipProb >= 20 ? d.precipProb + '%' : '') + '</span></div>';
        }).join('') + '</div></div>';
    }
    $('sec-overview').innerHTML = html;
  }

  /* ---------- hourly ---------- */
  function monotone(pts) {
    var n = pts.length;
    if (n < 2) return '';
    var dx = [], m = [], t = [], i;
    for (i = 0; i < n - 1; i++) { dx[i] = pts[i + 1][0] - pts[i][0]; m[i] = (pts[i + 1][1] - pts[i][1]) / dx[i]; }
    t[0] = m[0]; t[n - 1] = m[n - 2];
    for (i = 1; i < n - 1; i++) t[i] = m[i - 1] * m[i] <= 0 ? 0 : (3 * (dx[i - 1] + dx[i])) / ((2 * dx[i] + dx[i - 1]) / m[i - 1] + (dx[i] + 2 * dx[i - 1]) / m[i]);
    var d = 'M' + pts[0][0].toFixed(1) + ' ' + pts[0][1].toFixed(1);
    for (i = 0; i < n - 1; i++) {
      d += 'C' + (pts[i][0] + dx[i] / 3).toFixed(1) + ' ' + (pts[i][1] + (t[i] * dx[i]) / 3).toFixed(1) + ' ' + (pts[i + 1][0] - dx[i] / 3).toFixed(1) + ' ' + (pts[i + 1][1] - (t[i + 1] * dx[i]) / 3).toFixed(1) + ' ' + pts[i + 1][0].toFixed(1) + ' ' + pts[i + 1][1].toFixed(1);
    }
    return d;
  }

  var chartGeom = null;
  function chartSvg(hs, W) {
    var zone = tz(), n = hs.length;
    var L = 38, R = 14, top = 26, tH = 168, gap = 40, pH = 62, axis = 24;
    var H = top + tH + gap + pH + axis;
    var step = (W - L - R) / n;
    var x = function (i) { return L + (i + 0.5) * step; };
    var temps = hs.map(function (h) { return h.temp; });
    var mn = Math.min.apply(null, temps), mx = Math.max.apply(null, temps);
    var tickStep = mx - mn > 28 ? 10 : 5;
    var y0 = Math.floor((mn - 2) / tickStep) * tickStep, y1 = Math.ceil((mx + 2) / tickStep) * tickStep;
    if (y1 - y0 < tickStep * 2) y1 += tickStep;
    /* keep headroom above the curve for direct labels */
    while ((y1 - mx) / (y1 - y0) * tH < 24) y1 += tickStep;
    var ty = function (v) { return top + tH - ((v - y0) / (y1 - y0)) * tH; };
    var pTop = top + tH + gap, pBase = pTop + pH;
    var s = '<svg viewBox="0 0 ' + W + ' ' + H + '" height="' + H + '" role="img" aria-label="Temperature and chance of precipitation for the next ' + n + ' hours">';

    /* gradient that maps the y position to the temperature colour scale */
    s += '<defs><linearGradient id="c-temp" gradientUnits="userSpaceOnUse" x1="0" y1="' + ty(y1) + '" x2="0" y2="' + ty(y0) + '">';
    for (var k = 0; k <= 6; k++) s += '<stop offset="' + (k / 6).toFixed(3) + '" stop-color="' + U.tempColor(y1 - ((y1 - y0) * k) / 6) + '"/>';
    s += '</linearGradient><linearGradient id="c-area" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".13"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>' +
      '<linearGradient id="c-pop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#8fd0ff"/><stop offset="1" stop-color="#3f97f5"/></linearGradient></defs>';

    /* night shading + day boundaries */
    var i, runStart = -1;
    for (i = 0; i <= n; i++) {
      var night = i < n && !hs[i].isDay;
      if (night && runStart < 0) runStart = i;
      if (!night && runStart >= 0) {
        s += '<rect class="c-night" x="' + (L + runStart * step).toFixed(1) + '" y="' + top + '" width="' + ((i - runStart) * step).toFixed(1) + '" height="' + tH + '" rx="3"/>' +
          '<rect class="c-night" x="' + (L + runStart * step).toFixed(1) + '" y="' + pTop + '" width="' + ((i - runStart) * step).toFixed(1) + '" height="' + pH + '" rx="3"/>';
        runStart = -1;
      }
    }
    var starts = [], lastDay = '';
    for (i = 0; i < n; i++) { var dk = U.dayKey(hs[i].time, zone); if (dk !== lastDay) { starts.push(i); lastDay = dk; } }
    starts.forEach(function (st, k) {
      var len = (k + 1 < starts.length ? starts[k + 1] : n) - st, bx = L + st * step;
      if (st > 0) s += '<line class="c-mid" x1="' + bx.toFixed(1) + '" y1="' + (top - 16) + '" x2="' + bx.toFixed(1) + '" y2="' + pBase + '"/>';
      var name = st === 0 ? 'Today' : U.weekdayLong(hs[st].time, zone);
      if (len * step >= name.length * 6.6 + 12) s += '<text class="c-day" x="' + (bx + (st > 0 ? 7 : 0)).toFixed(1) + '" y="' + (top - 10) + '">' + esc(name) + '</text>';
    });

    /* temperature grid */
    for (var v = y0; v <= y1 + 0.01; v += tickStep) {
      s += '<line class="' + (v === y0 ? 'c-base' : 'c-grid') + '" x1="' + L + '" x2="' + (W - R) + '" y1="' + ty(v).toFixed(1) + '" y2="' + ty(v).toFixed(1) + '"/>' +
        '<text x="' + (L - 8) + '" y="' + (ty(v) + 3.5).toFixed(1) + '" text-anchor="end">' + v + '°</text>';
    }
    var pts = hs.map(function (h, j) { return [x(j), ty(h.temp)]; });
    var line = monotone(pts);
    s += '<path d="' + line + 'L' + pts[n - 1][0].toFixed(1) + ' ' + ty(y0).toFixed(1) + 'L' + pts[0][0].toFixed(1) + ' ' + ty(y0).toFixed(1) + 'Z" fill="url(#c-area)"/>' +
      '<path d="' + line + '" fill="none" stroke="url(#c-temp)" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>';

    /* direct labels: now, highest, lowest */
    var iMax = temps.indexOf(mx), iMin = temps.indexOf(mn);
    var marks = [[0, 'Now ' + temps[0] + '°', -1]];
    if (iMax > 2) marks.push([iMax, temps[iMax] + '°', -1]);
    if (iMin > 2 && Math.abs(iMin - iMax) > 2) marks.push([iMin, temps[iMin] + '°', 1]);
    marks.forEach(function (mk) {
      var px = pts[mk[0]][0], py = pts[mk[0]][1];
      var anchor = mk[0] === 0 ? 'start' : (px > W - R - 24 ? 'end' : 'middle');
      s += '<circle cx="' + px.toFixed(1) + '" cy="' + py.toFixed(1) + '" r="4" fill="' + U.tempColor(temps[mk[0]]) + '" stroke="rgba(18,22,30,.9)" stroke-width="2"/>' +
        '<text class="c-val" x="' + (mk[0] === 0 ? px - 4 : px).toFixed(1) + '" y="' + (py + (mk[2] < 0 ? -11 : 19)).toFixed(1) + '" text-anchor="' + anchor + '">' + mk[1] + '</text>';
    });

    /* precipitation probability panel (own axis, shared x) */
    s += '<text class="c-title" x="' + L + '" y="' + (pTop - 12) + '">Chance of precipitation</text>';
    [0, 50, 100].forEach(function (pv) {
      var py = pBase - (pv / 100) * pH;
      s += '<line class="' + (pv === 0 ? 'c-base' : 'c-grid') + '" x1="' + L + '" x2="' + (W - R) + '" y1="' + py.toFixed(1) + '" y2="' + py.toFixed(1) + '"/>' +
        '<text x="' + (L - 8) + '" y="' + (py + 3.5).toFixed(1) + '" text-anchor="end">' + pv + '%</text>';
    });
    var bw = Math.max(2, step - 2);
    hs.forEach(function (h, j) {
      var p = h.precipProb || 0;
      if (p <= 0) return;
      var bh = Math.max(2, (p / 100) * pH), r = Math.min(3, bw / 2, bh);
      var bx2 = x(j) - bw / 2, by = pBase - bh;
      s += '<path d="M' + bx2.toFixed(1) + ' ' + pBase + 'V' + (by + r).toFixed(1) + 'q0-' + r + ' ' + r + '-' + r + 'h' + (bw - 2 * r).toFixed(1) + 'q' + r + ' 0 ' + r + ' ' + r + 'V' + pBase + 'Z" fill="url(#c-pop)" opacity="' + (0.45 + 0.55 * (p / 100)).toFixed(2) + '"/>';
    });

    /* x axis: clock hours */
    var every = step >= 13 ? 3 : 6;
    for (i = 0; i < n; i++) {
      var hn = U.hourNum(hs[i].time, zone);
      if (hn % every !== 0) continue;
      if (x(i) < L + 14 || x(i) > W - R - 10) continue;
      s += '<line class="c-base" x1="' + x(i).toFixed(1) + '" x2="' + x(i).toFixed(1) + '" y1="' + pBase + '" y2="' + (pBase + 4) + '"/>' +
        '<text x="' + x(i).toFixed(1) + '" y="' + (pBase + 17) + '" text-anchor="middle">' + esc(U.hour(hs[i].time, zone)) + '</text>';
    }
    s += '<g id="c-hover" visibility="hidden"><line class="c-cross" id="c-line" y1="' + top + '" y2="' + pBase + '"/><circle id="c-dot" r="5" fill="#fff" stroke="rgba(18,22,30,.9)" stroke-width="2"/></g>';
    s += '</svg>';
    chartGeom = { L: L, step: step, n: n, pts: pts };
    return s;
  }

  function showTip(i, clientX) {
    var hs = state.hourly, h = hs[i], g = chartGeom, zone = tz();
    if (!h || !g) return;
    var svg = document.querySelector('#chart svg');
    var box = svg.getBoundingClientRect();
    document.getElementById('c-hover').setAttribute('visibility', 'visible');
    var line = document.getElementById('c-line'), dot = document.getElementById('c-dot');
    line.setAttribute('x1', g.pts[i][0]); line.setAttribute('x2', g.pts[i][0]);
    dot.setAttribute('cx', g.pts[i][0]); dot.setAttribute('cy', g.pts[i][1]);
    tip.innerHTML = '<div class="tip-h">' + I.wx(h.icon, h.isDay) + '<div><b>' + esc(U.weekday(h.time, zone) + ' ' + U.hour(h.time, zone)) + '</b>' + esc(h.condition) + '</div></div>' +
      '<div class="tip-big">' + U.deg(h.temp) + (h.feelsLike != null ? '<small>Feels like ' + U.deg(h.feelsLike) + '</small>' : '') + '</div>' +
      '<div class="tip-r"><span>Chance of precipitation</span><b>' + U.num(h.precipProb, '%') + '</b></div>' +
      (h.precipAmount ? '<div class="tip-r"><span>Amount</span><b>' + h.precipAmount.toFixed(2) + '″</b></div>' : '') +
      '<div class="tip-r"><span>Wind</span><b>' + U.num(h.windSpeed, ' mph') + ' ' + esc(h.windDir || '') + '</b></div>' +
      '<div class="tip-r"><span>Humidity</span><b>' + U.num(h.humidity, '%') + '</b></div>';
    tip.hidden = false;
    var px = box.left + g.pts[i][0], tw = tip.offsetWidth, th = tip.offsetHeight;
    var left = px + 14;
    if (left + tw > window.innerWidth - 12) left = px - tw - 14;
    tip.style.left = Math.max(8, left) + 'px';
    tip.style.top = Math.max(48, Math.min(window.innerHeight - th - 10, box.top + 34)) + 'px';
  }
  function hideTip() {
    tip.hidden = true;
    var g = document.getElementById('c-hover');
    if (g) g.setAttribute('visibility', 'hidden');
  }

  var chartW = 0, kbIndex = 0;
  function drawChart() {
    var el = $('chart');
    if (!el || !hasData()) return;
    var w = Math.floor(el.clientWidth);
    if (w < 200) return;
    chartW = w;
    el.innerHTML = chartSvg(state.hourly.slice(0, 48), w);
  }

  function renderHourly() {
    var s = state, zone = tz(), hs = (s.hourly || []).slice(0, 48);
    if (!hs.length) { $('sec-hourly').innerHTML = '<div class="empty"><span class="empty-ic is-muted">' + I.gl('clock') + '</span><h2>No hourly forecast</h2><p>Hourly data isn’t available right now.</p></div>'; return; }
    var html = '<h1 class="page-title">Next ' + hs.length + ' hours</h1><p class="page-sub">' + esc((s.nextPrecip && s.nextPrecip.headline) || '') + '</p>' +
      '<div class="card chart-card"><div class="card-h"><span>Temperature</span><span class="chart-hint">Hover or use ← → for details</span></div>' +
      '<div class="chart" id="chart" tabindex="0" aria-label="Hourly chart. Use the left and right arrow keys to step through hours."></div></div>';

    html += '<div class="card hr-card"><div class="hr-cols"><span>Time</span><span></span><span>Conditions</span><span class="r">Temp</span><span class="r">Feels</span><span style="padding-left:14px">Precip</span><span class="r">Amount</span><span class="r">Wind</span><span class="r">Humidity</span></div>';
    var last = '';
    hs.forEach(function (h, i) {
      var dk = U.dayKey(h.time, zone);
      if (dk !== last) {
        last = dk;
        html += '<div class="hr-day">' + esc(i === 0 ? 'Today' : U.weekdayLong(h.time, zone)) + '<small>' + esc(U.monthDay(h.time, zone)) + '</small></div>';
      }
      var p = h.precipProb || 0;
      html += '<div class="hr-row"><span class="hr-t">' + (i === 0 ? 'Now' : esc(U.hour(h.time, zone))) + '</span>' + I.wx(h.icon, h.isDay) +
        '<span class="hr-c">' + esc(h.condition) + '</span><span class="hr-v r">' + U.deg(h.temp) + '</span><span class="hr-n r">' + U.deg(h.feelsLike) + '</span>' +
        '<span class="hr-pop' + (p >= 30 ? ' is-on' : '') + '" style="padding-left:14px"><i><b style="width:' + p + '%"></b></i><span>' + (h.precipProb == null ? '–' : p + '%') + '</span></span>' +
        '<span class="hr-n r">' + (h.precipAmount ? h.precipAmount.toFixed(2) + '″' : '–') + '</span>' +
        '<span class="hr-n r">' + (h.windSpeed == null ? '–' : h.windSpeed + ' mph ' + esc(h.windDir || '')) + '</span><span class="hr-n r">' + U.num(h.humidity, '%') + '</span></div>';
    });
    html += '</div>';
    $('sec-hourly').innerHTML = html;
    drawChart();

    var el = $('chart');
    var idxAt = function (e) {
      var box = el.querySelector('svg').getBoundingClientRect();
      return Math.max(0, Math.min(chartGeom.n - 1, Math.floor((e.clientX - box.left - chartGeom.L) / chartGeom.step)));
    };
    el.addEventListener('pointermove', function (e) { if (chartGeom) { kbIndex = idxAt(e); showTip(kbIndex, e.clientX); } });
    el.addEventListener('pointerleave', hideTip);
    el.addEventListener('blur', hideTip);
    el.addEventListener('keydown', function (e) {
      if (!chartGeom) return;
      if (e.key === 'ArrowRight') kbIndex = Math.min(chartGeom.n - 1, kbIndex + 1);
      else if (e.key === 'ArrowLeft') kbIndex = Math.max(0, kbIndex - 1);
      else if (e.key === 'Escape') { hideTip(); return; }
      else return;
      e.preventDefault();
      showTip(kbIndex);
    });
  }
  new ResizeObserver(function () {
    var el = $('chart');
    if (current === 'hourly' && el && Math.floor(el.clientWidth) !== chartW) { hideTip(); drawChart(); }
  }).observe(main);
  main.addEventListener('scroll', hideTip, { passive: true });

  /* ---------- 7-day ---------- */
  function renderDaily() {
    var s = state, ds = s.daily || [], zone = tz();
    if (!ds.length) { $('sec-daily').innerHTML = '<div class="empty"><span class="empty-ic is-muted">' + I.gl('calendar') + '</span><h2>No daily forecast</h2><p>The 7-day forecast isn’t available right now.</p></div>'; return; }
    if (!openDays) openDays = new Set([ds[0].date]);
    var rg = weekRange(ds, s.current && s.current.temp);
    var html = '<h1 class="page-title">7-day forecast</h1><p class="page-sub">National Weather Service forecast for ' + esc(s.location ? s.location.city + ', ' + s.location.state : 'your area') + '. Select a day for the full text.</p><div class="card dy">';
    ds.forEach(function (d, i) {
      var open = openDays.has(d.date);
      var both = d.dayText && d.nightText;
      html += '<div class="dy-item' + (open ? ' is-open' : '') + '"><button class="dy-row" aria-expanded="' + open + '" data-day="' + esc(d.date) + '">' +
        '<span class="dy-n"><b>' + esc(i === 0 ? 'Today' : U.dateWeekdayLong(d.date) || d.name) + '</b><small>' + esc(U.dateLabel(d.date)) + '</small></span>' +
        I.wx(d.icon, true) + '<span class="dy-c">' + esc(d.condition) + '</span>' +
        '<span class="dy-p">' + (d.precipProb != null && d.precipProb >= 10 ? I.gl('drop') + d.precipProb + '%' : '') + '</span>' +
        '<span class="dy-lo">' + U.deg(d.low) + '</span>' + rangeBar(d, rg[0], rg[1], i === 0 && s.current ? s.current.temp : null) + '<span class="dy-hi">' + U.deg(d.high) + '</span>' +
        '<span class="dy-chev">' + I.gl('chevron-down') + '</span></button>' +
        '<div class="dy-more"><div>' +
        (d.dayText || d.nightText
          ? '<div class="dy-text' + (both ? '' : ' is-single') + '">' +
            (d.dayText ? '<div class="dy-part"><h3>' + I.wx(d.icon, true) + 'Day</h3><p>' + esc(d.dayText) + '</p></div>' : '') +
            (d.nightText ? '<div class="dy-part"><h3>' + I.wx(/clear|partly/.test(d.icon) ? d.icon : 'clear', false) + 'Night</h3><p>' + esc(d.nightText) + '</p></div>' : '') + '</div>'
          : '<div class="dy-text is-single"><div class="dy-part"><p style="color:var(--t3)">No detailed forecast text for this day yet.</p></div></div>') +
        (d.sunrise || d.sunset ? '<div class="dy-sun">' + (d.sunrise ? '<span>' + I.gl('sunrise') + 'Sunrise <b>' + esc(U.time(d.sunrise, zone)) + '</b></span>' : '') + (d.sunset ? '<span>' + I.gl('sunset') + 'Sunset <b>' + esc(U.time(d.sunset, zone)) + '</b></span>' : '') + '</div>' : '') +
        '</div></div></div>';
    });
    html += '</div>';
    $('sec-daily').innerHTML = html;
  }
  $('sec-daily').addEventListener('click', function (e) {
    var row = e.target.closest('.dy-row');
    if (!row) return;
    var key = row.dataset.day, open = !openDays.has(key);
    if (open) openDays.add(key); else openDays.delete(key);
    row.setAttribute('aria-expanded', open);
    row.parentNode.classList.toggle('is-open', open);
  });

  /* ---------- alerts ---------- */
  function renderAlerts() {
    var s = state, al = s.alerts || [], zone = tz();
    var where = s.location ? esc(s.location.city) + ', ' + esc(s.location.state) : 'your area';
    if (!al.length) {
      $('sec-alerts').innerHTML = '<h1 class="page-title">Alerts</h1><div class="empty"><span class="empty-ic">' + I.gl('check-circle') + '</span><h2>No active alerts</h2>' +
        '<p>The National Weather Service has no watches, warnings or advisories in effect for ' + where + '.' + (s.sources && s.sources.nws === false ? ' Alerts couldn’t be refreshed just now, so this may be out of date.' : '') + '</p></div>';
      return;
    }
    var now = new Date(nowMs()).toISOString();
    var html = '<h1 class="page-title">Alerts</h1><p class="page-sub">' + al.length + ' official ' + (al.length === 1 ? 'alert' : 'alerts') + ' in effect for ' + where + ', most severe first.</p>';
    al.forEach(function (a) {
      var sv = U.sev(a);
      html += '<article class="card al sev-' + sv + '"><div class="al-top"><span class="al-ic">' + I.gl('alert') + '</span><div><h2 class="al-ev">' + esc(a.event) + '</h2><div class="al-when">' + alertWhen(a) + '</div></div></div>' +
        '<div class="chips"><span class="chip is-sev">' + esc(sv === 'Unknown' ? 'Severity unknown' : sv) + '</span>' +
        (a.urgency ? '<span class="chip">Urgency <b>' + esc(a.urgency) + '</b></span>' : '') +
        (a.certainty ? '<span class="chip">Certainty <b>' + esc(a.certainty) + '</b></span>' : '') +
        (a.onset ? '<span class="chip">Began <b>' + esc(U.when(a.onset, zone, now)) + '</b></span>' : '') +
        (a.expires && a.expires !== a.ends ? '<span class="chip">Message expires <b>' + esc(U.when(a.expires, zone, now)) + '</b></span>' : '') + '</div>' +
        (a.headline ? '<p class="al-headline">' + esc(a.headline) + '</p>' : '') +
        (a.instruction ? '<div class="al-do"><h3>What to do</h3><p>' + esc(a.instruction) + '</p></div>' : '') +
        (a.description ? '<div class="al-body">' + esc(a.description) + '</div>' : '') +
        '<div class="al-foot">' + (a.areaDesc ? '<b>Area</b> ' + esc(a.areaDesc) + '<br>' : '') + (a.sender ? '<b>Issued by</b> ' + esc(a.sender) : '') + '</div></article>';
    });
    $('sec-alerts').innerHTML = html;
  }

  /* ---------- settings ---------- */
  var zipIn = $('zip'), zipStatus = $('zip-status'), zipAdd = $('zip-add');
  var ZIP_HINT = 'US ZIP code. The new place becomes the active one.';
  var zipToken = 0, zipTimer = null, zipResolved = null, adding = false;

  function setStatus(kind, html) { zipStatus.className = 'inline-status' + (kind ? ' is-' + kind : ''); zipStatus.innerHTML = html || ZIP_HINT; }

  function renderPlaces() {
    var ls = places.locations || [];
    $('loc-list').innerHTML = ls.map(function (p) {
      var on = p.zip === places.activeZip;
      return '<div class="loc-row"><button class="loc-pick" role="radio" aria-checked="' + on + '" data-zip="' + esc(p.zip) + '"><i class="loc-radio"></i><span><div class="loc-name">' + esc(p.city) + ', ' + esc(p.state) + '</div><div class="loc-zip">' + esc(p.zip) + '</div></span></button>' +
        (on ? '<span class="loc-tag">Showing now</span>' : '') +
        (ls.length > 1 ? '<button class="loc-del" data-del="' + esc(p.zip) + '" aria-label="Remove ' + esc(p.city) + ', ' + esc(p.state) + '" title="Remove">' + I.gl('trash') + '</button>' : '') + '</div>';
    }).join('');
  }
  $('loc-list').addEventListener('click', function (e) {
    var d = e.target.closest('[data-del]');
    if (d) { Promise.resolve(window.sky.removeLocation(d.dataset.del)).catch(function () {}); return; }
    var p = e.target.closest('.loc-pick');
    if (p && p.dataset.zip !== places.activeZip) pickPlace(p.dataset.zip);
  });

  function applySettings() {
    var s = settings;
    if (!s) return;
    document.querySelectorAll('[data-set]').forEach(function (b) { b.setAttribute('aria-checked', s[b.dataset.set] ? 'true' : 'false'); });
    document.querySelectorAll('[data-seg]').forEach(function (g) {
      g.querySelectorAll('button').forEach(function (b) { var on = String(s[g.dataset.seg]) === b.dataset.val; b.setAttribute('aria-checked', on ? 'true' : 'false'); b.tabIndex = on ? 0 : -1; });
    });
    var glass = s.glass === 'clear' || s.glass === 'frosted' ? s.glass : 'none';
    var lay = s.layout || 'full';
    var ps = (window.WD_PRESETS || []).filter(function (p) { return p.id === s.layout; })[0];
    $('cz-summary').textContent = (ps ? 'Using the \u201c' + ps.name + '\u201d style' : 'Using your own custom layout') + ' \u00b7 ' + ({ clear: 'tinted glass', frosted: 'frosted' }[s.glass] || 'transparent') + '.';
    if (current === 'widget') window.WDCustomize.update(state, settings);
    var docked = s.placement === 'taskbar';
    $('placement-sub').textContent = docked
      ? 'Docked at the left end of the taskbar. Best with centered taskbar icons.'
      : 'A widget on your desktop.';
    document.querySelectorAll('[data-desktop-only]').forEach(function (r) { r.hidden = docked; });
    $('covered-sub').textContent = s.coveredMode === 'taskbar'
      ? 'A small weather pill appears on the taskbar.'
      : 'The tray icon shows the current temperature.';
  }
  function patch(p) {
    settings = Object.assign({}, settings, p);
    applySettings();
    Promise.resolve(window.sky.setSettings(p)).then(function (s) { if (s) { settings = s; applySettings(); } }).catch(function () {});
  }
  document.querySelectorAll('[data-set]').forEach(function (b) { b.addEventListener('click', function () { var o = {}; o[b.dataset.set] = b.getAttribute('aria-checked') !== 'true'; patch(o); }); });
  document.querySelectorAll('[data-seg]').forEach(function (g) {
    g.addEventListener('click', function (e) {
      var b = e.target.closest('button'); if (!b) return;
      var o = {}; o[g.dataset.seg] = g.dataset.seg === 'refreshMinutes' ? Number(b.dataset.val) : b.dataset.val; patch(o);
    });
    g.addEventListener('keydown', function (e) {
      if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
      var bs = [].slice.call(g.querySelectorAll('button')), i = bs.indexOf(document.activeElement);
      var nx = bs[(i + (e.key === 'ArrowRight' ? 1 : bs.length - 1)) % bs.length];
      e.preventDefault(); nx.focus(); nx.click();
    });
  });

  zipIn.addEventListener('input', function () {
    var v = zipIn.value.replace(/\D/g, '').slice(0, 5);
    if (v !== zipIn.value) zipIn.value = v;
    clearTimeout(zipTimer); zipToken++; zipResolved = null; zipAdd.disabled = true;
    zipIn.classList.remove('is-bad');
    if (v.length < 5) { setStatus('', ''); return; }
    var have = (places.locations || []).filter(function (p) { return p.zip === v; })[0];
    if (have) { setStatus('', I.gl('info') + esc(have.city) + ', ' + esc(have.state) + ' is already in your list.'); return; }
    var tk = zipToken;
    setStatus('busy', I.gl('refresh', 'spin') + 'Looking up ' + esc(v) + '…');
    zipTimer = setTimeout(function () {
      Promise.resolve(window.sky.lookupZip(v)).then(function (r) {
        if (tk !== zipToken) return;
        if (r && r.ok) { zipResolved = r; setStatus('ok', I.gl('check-circle') + '<span>' + esc(r.city) + ', ' + esc(r.state) + '</span>'); zipAdd.disabled = false; }
        else { zipIn.classList.add('is-bad'); setStatus('bad', I.gl('info') + esc((r && r.error) || 'We couldn\u2019t find that ZIP code.')); }
      }).catch(function () { if (tk === zipToken) { zipIn.classList.add('is-bad'); setStatus('bad', I.gl('info') + 'Couldn\u2019t check that ZIP code. Try again.'); } });
    }, 250);
  });
  $('loc-form').addEventListener('submit', function (e) {
    e.preventDefault();
    if (!zipResolved || adding) return;
    var r = zipResolved; zipResolved = null; adding = true; zipAdd.disabled = true;
    setStatus('busy', I.gl('refresh', 'spin') + 'Adding ' + esc(r.city) + ', ' + esc(r.state) + '…');
    Promise.resolve(window.sky.addLocation(r.zip)).then(function (res) {
      adding = false;
      if (res && res.ok) { zipIn.value = ''; setStatus('', ''); }
      else { zipIn.classList.add('is-bad'); setStatus('bad', I.gl('info') + esc((res && res.error) || 'Couldn\u2019t add that location.')); }
    }).catch(function () { adding = false; setStatus('bad', I.gl('info') + 'Couldn\u2019t add that location.'); });
  });
  zipIn.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') { zipIn.value = ''; zipIn.dispatchEvent(new Event('input')); }
  });
  $('set-refresh').addEventListener('click', doRefresh);
  $('set-quit').addEventListener('click', function () { window.sky.quit(); });

  /* ---------- updates ---------- */
  function renderUpdate() {
    var u = update, sub = $('upd-sub'), check = $('upd-check'), inst = $('upd-install'), bar = $('upd-bar');
    if (!u) { $('upd-card').hidden = true; $('updates-title').hidden = true; $('tb-update').hidden = true; return; }
    $('upd-card').hidden = false; $('updates-title').hidden = false;
    $('upd-name').textContent = 'WeathaDock ' + (u.version || '');
    var text = '', cls = '', busy = u.status === 'checking' || u.status === 'downloading';
    var checked = u.checkedAt ? U.when(u.checkedAt, undefined) : '';
    if (u.status === 'unsupported') text = 'Updates are available in the installed version.';
    else if (u.status === 'checking') text = 'Checking for updates…';
    else if (u.status === 'downloading') text = 'Downloading version ' + (u.newVersion || '') + (u.percent != null ? ' · ' + Math.round(u.percent) + '%' : '…');
    else if (u.status === 'ready') { text = 'Version ' + (u.newVersion || '') + ' is ready. Restart WeathaDock to finish updating.'; cls = 'is-good'; }
    else if (u.status === 'up-to-date') text = 'You\u2019re up to date' + (checked ? ' · checked ' + checked : '') + '.';
    else if (u.status === 'error') { text = u.error || 'Couldn\u2019t check for updates.'; cls = 'is-bad'; }
    else text = checked ? 'Last checked ' + checked + '.' : 'WeathaDock checks for updates automatically.';
    sub.textContent = text; sub.className = 'set-sub ' + cls;
    bar.hidden = u.status !== 'downloading';
    $('upd-fill').style.width = Math.max(2, Math.min(100, u.percent || 0)) + '%';
    check.hidden = u.status === 'unsupported' || u.status === 'ready';
    check.disabled = busy; check.classList.toggle('is-busy', busy);
    $('upd-check-tx').textContent = u.status === 'checking' ? 'Checking…' : u.status === 'downloading' ? 'Downloading…' : u.status === 'error' ? 'Try again' : 'Check for updates';
    inst.hidden = u.status !== 'ready';
    $('tb-update').hidden = u.status !== 'ready';
  }
  $('upd-check').addEventListener('click', function () {
    Promise.resolve(window.sky.checkForUpdates()).then(function (u) { if (u) { update = u; renderUpdate(); } }).catch(function () {});
  });
  $('upd-install').addEventListener('click', function () { window.sky.installUpdate(); });
  $('tb-update').addEventListener('click', function () { go('settings'); $('updates-title').scrollIntoView({ block: 'start' }); });

  function renderSettingsState() {
    var s = state;
    var upd = 'Not updated yet';
    if (s) {
      if (s.status === 'loading') upd = 'Updating now…';
      else if (s.updatedAt) upd = 'Last updated ' + U.when(s.updatedAt, tz()) + (s.stale ? ' · last refresh failed' : '');
    }
    $('set-updated').textContent = upd;
    var src = (s && s.sources) || {};
    var known = !!(s && s.updatedAt);
    [['src-nws', src.nws], ['src-om', src.openMeteo]].forEach(function (p) {
      $(p[0]).className = 'src-dot' + (known ? (p[1] ? ' is-on' : ' is-off') : '');
      $(p[0] + '-tx').textContent = known ? (p[1] ? 'Connected' : 'No response') : '';
    });
  }

  /* ---------- render ---------- */
  function renderSection() {
    renderHead();
    if (!state) return;
    renderSettingsState();
    if (current === 'widget') { window.WDCustomize.update(state, settings); return; }
    if (current === 'settings') return;
    var el = $('sec-' + current);
    if (!hasData()) { el.innerHTML = placeholder(); return; }
    if (current === 'overview') renderOverview();
    else if (current === 'hourly') renderHourly();
    else if (current === 'daily') renderDaily();
    else if (current === 'alerts') renderAlerts();
  }

  U.watchVisibility();
  window.WDCustomize.init($('sec-widget'), {
    /* next: full settings object for instant UI; patch: what is sent to main */
    commit: function (next, patch) {
      settings = next; applySettings();
      Promise.resolve(window.sky.setSettings(patch)).then(function (s) { if (s) { settings = s; applySettings(); } }).catch(function () {});
    }
  });

  U.ready().then(function (sky) {
    sky.onState(function (s) { state = s; hideTip(); renderSection(); });
    sky.onSettings(function (s) { settings = s; applySettings(); });
    sky.onNavigate(function (sec) { go(sec); });
    sky.getSettings().then(function (s) { settings = s; applySettings(); });
    if (sky.onLocations) sky.onLocations(function (v) { if (v) { places = v; renderPlaces(); renderHead(); } });
    if (sky.getLocations) sky.getLocations().then(function (v) { if (v) { places = v; renderPlaces(); renderHead(); } }).catch(function () {});
    if (sky.onUpdateState) sky.onUpdateState(function (u) { update = u; renderUpdate(); });
    if (sky.getUpdateState) sky.getUpdateState().then(function (u) { update = u; renderUpdate(); }).catch(function () { renderUpdate(); });
    else renderUpdate();
    go(current);
    sky.getState().then(function (s) { state = s; renderSection(); });
  });
})();
