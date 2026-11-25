/* WeathaDock widget renderer, shared by the desktop widget (widget.js) and the live preview in
   the detail window. Everything is driven by a WidgetConfig (see presets.js).
   WDWidget.normalize(cfg)                      -> complete, valid config
   WDWidget.mount(root, state, cfg, opts)       -> renders into `root` (class "wv"); returns true if the DOM changed
   WDWidget.fit(root, maxW, maxH)               -> scales down if the content exceeds the given box
   opts: { glass, places:{locations,activeZip}, refreshing, animate, pending } */
(function () {
  'use strict';
  var U = window.SkyUI, I = window.SkyIcons, esc = U.esc;

  var KEYS = ['location', 'updated', 'refresh', 'icon', 'temp', 'condition', 'highLow', 'feelsLike', 'incoming', 'timeline', 'hourly', 'daily', 'alerts', 'wind', 'humidity', 'uv', 'sun'];
  var DEFAULT_ON = ['location', 'updated', 'refresh', 'icon', 'temp', 'condition', 'highLow', 'feelsLike', 'incoming', 'timeline', 'hourly', 'daily', 'alerts'];
  var ZOOM = { small: 0.8, medium: 1, large: 1.2, xlarge: 1.45 };

  function pick(v, list, d) { return list.indexOf(v) >= 0 ? v : d; }
  function normalize(c) {
    c = c || {};
    var show = {}, src = c.show || {};
    KEYS.forEach(function (k) { show[k] = typeof src[k] === 'boolean' ? src[k] : DEFAULT_ON.indexOf(k) >= 0; });
    return {
      orientation: pick(c.orientation, ['vertical', 'horizontal'], 'vertical'),
      size: pick(c.size, ['small', 'medium', 'large', 'xlarge'], 'medium'),
      align: pick(c.align, ['left', 'center', 'right'], 'left'),
      ink: pick(c.ink, ['light', 'dark'], 'light'),
      hours: pick(Number(c.hours), [4, 6, 8, 12], 6),
      days: pick(Number(c.days), [3, 5, 7], 5),
      show: show
    };
  }
  function presetConfig(id) {
    var ps = window.WD_PRESETS || [];
    for (var i = 0; i < ps.length; i++) if (ps[i].id === id) return ps[i].widget;
    return null;
  }

  var UP = '<svg class="wv-arrow" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 9V1.5M1.8 4.5 5 1.3l3.2 3.2"/></svg>';
  var DOWN = '<svg class="wv-arrow" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 1v7.5M1.8 5.5 5 8.7l3.2-3.2"/></svg>';

  /* ---------- optical metrics ----------
     Digits are measured in the real font so an icon beside the big temperature can be sized to
     the digits' height and centred on their optical middle (not on the line box). */
  var metrics = null;
  function measure() {
    var cap = 0.715, shift = 0;
    try {
      var g = document.createElement('canvas').getContext('2d');
      var fam = getComputedStyle(document.documentElement).getPropertyValue('--font') || 'system-ui, sans-serif';
      g.font = '400 100px ' + fam;
      var m = g.measureText('0');
      if (m.actualBoundingBoxAscent > 40) {
        cap = m.actualBoundingBoxAscent / 100;
        if (m.fontBoundingBoxAscent != null) shift = ((m.fontBoundingBoxAscent - m.fontBoundingBoxDescent) / 100 - cap) / 2;
      }
    } catch (e) {}
    metrics = { cap: cap, shift: shift };
    return metrics;
  }
  function getMetrics() { return metrics || measure(); }
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(function () { var old = metrics; measure(); if (old && (Math.abs(old.cap - metrics.cap) > 0.004 || Math.abs(old.shift - metrics.shift) > 0.004) && window.WDWidget.onMetrics) window.WDWidget.onMetrics(); });

  function tzOf(s) { return s.location && s.location.timeZone; }
  function locName(s) { return s.location ? esc(s.location.city) + ', ' + esc(s.location.state) : 'WeathaDock'; }

  /* ---------- pieces ---------- */
  function alertBanner(s) {
    var a = s.alerts && s.alerts[0];
    if (!a) return '';
    var tz = tzOf(s), until = a.ends || a.expires;
    var sub = until ? 'Until ' + U.when(until, tz, s.updatedAt) : (a.headline || 'Active now');
    if (a.onset && s.updatedAt && new Date(a.onset) > new Date(s.updatedAt)) sub = 'From ' + U.when(a.onset, tz, s.updatedAt) + (until ? ' to ' + U.when(until, tz, s.updatedAt) : '');
    var more = s.alerts.length > 1 ? '<span class="wv-alert-more" title="' + (s.alerts.length - 1) + ' more">+' + (s.alerts.length - 1) + '</span>' : '';
    return '<div class="wv-alert sev-' + U.sev(a) + '" data-open="alerts" role="button" aria-label="' + esc(a.event) + ', open alerts">' +
      '<span class="wv-alert-ic">' + I.gl('alert') + '</span>' +
      '<span class="wv-alert-tx"><span class="wv-alert-ev" title="' + esc(a.event) + '">' + esc(a.event) + '</span><span class="wv-alert-sub">' + esc(sub) + '</span></span>' +
      more + '<span class="wv-alert-chev">' + I.gl('chevron-right') + '</span></div>';
  }
  function alertChip(s) {
    var a = s.alerts && s.alerts[0];
    if (!a) return '';
    return '<span class="wv-chip sev-' + U.sev(a) + '" data-open="alerts" role="button" title="' + esc(a.event) + '" aria-label="' + esc(a.event) + ', open alerts">' +
      '<i class="wv-chip-ic">' + I.gl('alert') + '</i><span>' + esc(a.event) + '</span>' + (s.alerts.length > 1 ? '<b>+' + (s.alerts.length - 1) + '</b>' : '') + '</span>';
  }

  function refreshBtn(s, o) {
    var busy = o.refreshing || s.status === 'loading';
    return '<span class="wv-ref' + (busy ? ' is-busy' : '') + '" data-action="refresh" role="button" aria-label="Refresh now" aria-disabled="' + busy + '" title="Refresh now">' + I.gl('refresh') + '</span>';
  }
  function placeDots(o) {
    var p = o.places || {}, ls = p.locations || [];
    if (ls.length < 2) return '';
    var idx = Math.max(0, ls.findIndex(function (x) { return x.zip === p.activeZip; }));
    if (ls.length > 6) return '<span class="wv-dots"><small>' + (idx + 1) + '/' + ls.length + '</small></span>';
    return '<span class="wv-dots">' + ls.map(function (x, i) { return '<i' + (i === idx ? ' class="on"' : '') + '></i>'; }).join('') + '</span>';
  }
  function locPart(s, o) {
    var dots = placeDots(o), name = locName(s);
    return '<span class="wv-loc"' + (dots ? ' data-action="next-place" role="button" title="' + name + ' – click to switch location" aria-label="' + name + ', switch location"' : ' title="' + name + '"') + '>' +
      I.gl('pin') + '<span class="wv-loc-n">' + name + '</span>' + dots + '</span>';
  }
  function updPart(s, o, short) {
    var tz = tzOf(s);
    if (s.stale) return '<span class="wv-upd is-stale" title="' + esc(s.error || '') + '"><i class="wv-dot"></i>Offline' + (short || !s.updatedAt ? '' : ' · ' + esc(U.time(s.updatedAt, tz))) + '</span>';
    if (o.refreshing || s.status === 'loading') return '<span class="wv-upd">Updating…</span>';
    return s.updatedAt ? '<span class="wv-upd" title="Last updated">' + (short ? '' : 'Updated ') + esc(U.time(s.updatedAt, tz)) + '</span>' : '';
  }
  function header(s, c, o) {
    var sh = c.show;
    if (!sh.location && !sh.updated && !sh.refresh) return '';
    var right = (sh.updated ? updPart(s, o, !sh.location ? false : true) : '') + (sh.refresh ? refreshBtn(s, o) : '');
    return '<div class="wv-sec wv-head" data-open="overview">' + (sh.location ? locPart(s, o) : '') + (right ? '<span class="wv-right">' + right + '</span>' : '') + '</div>';
  }

  function metaParts(s, c) {
    var cur = s.current || {}, t = s.today || {}, sh = c.show, out = [];
    if (sh.highLow) out.push('<span class="wv-hl" title="Today’s high">' + UP + '<b>' + U.deg(t.high) + '</b></span><span class="wv-hl" title="Today’s low">' + DOWN + '<b>' + U.deg(t.low) + '</b></span>');
    if (sh.feelsLike && cur.feelsLike != null) out.push('<span class="wv-feels">Feels like <b>' + U.deg(cur.feelsLike) + '</b></span>');
    return out.join('<i class="wv-vsep"></i>');
  }
  function tempPart(s) {
    var cur = s.current || {};
    if (cur.temp == null) return '<div class="wv-temp">–</div>';
    var t = Math.round(cur.temp);
    return '<div class="wv-temp" role="text" aria-label="' + t + ' degrees">' + t + '<i class="wv-deg" aria-hidden="true"></i></div>';
  }
  /* paired = sits beside (or above) the big temperature: size to the digits and share their centre line */
  function iconPart(s, o, paired) {
    var cur = s.current || {}, key = cur.icon || 'cloudy', day = cur.isDay !== false, style = '';
    if (paired) {
      var m = getMetrics(), b = I.bounds(key, day);
      var frac = Math.max(0.64, (b[1] - b[0]) / 64);            /* share of the box the artwork fills */
      var k = m.cap / frac;                                      /* icon box size, in temperature font-sizes */
      var off = ((b[0] + b[1]) / 2 - 32) / 64;                   /* artwork centre vs box centre, in box sizes */
      style = ' style="--k:' + k.toFixed(4) + ';--dy:' + (m.shift - off * k).toFixed(4) + '"';
    }
    return '<div class="wv-icon' + (paired ? ' is-paired' : '') + '"' + style + '>' + I.wx(key, day, { animated: o.animate !== false }) + '</div>';
  }
  function current(s, c, o) {
    var sh = c.show, cur = s.current || {};
    var meta = metaParts(s, c);
    if (!sh.icon && !sh.temp && !sh.condition && !meta) return '';
    var cls = 'wv-sec wv-now' + (sh.icon ? ' has-icon' : '') + (sh.temp ? ' has-temp' : '') + (!sh.temp && !sh.icon ? ' is-text' : '');
    return '<div class="' + cls + '" data-open="overview">' +
      (sh.icon || sh.temp ? '<div class="wv-now-top">' + (sh.temp ? tempPart(s) : '') + (sh.icon ? iconPart(s, o, sh.temp) : '') + '</div>' : '') +
      (sh.condition && cur.condition ? '<div class="wv-cond">' + esc(cur.condition) + '</div>' : '') +
      (meta ? '<div class="wv-meta">' + meta + '</div>' : '') + '</div>';
  }

  function statItems(s, c) {
    var cur = s.current || {}, t = s.today || {}, sh = c.show, tz = tzOf(s), out = [];
    if (sh.wind && cur.windSpeed != null) out.push('<span class="wv-stat" title="Wind' + (cur.windGust ? ', gusts to ' + cur.windGust + ' mph' : '') + '">' + I.gl('wind') + '<b>' + cur.windSpeed + '</b> mph' + (cur.windDir ? ' ' + esc(cur.windDir) : '') + '</span>');
    if (sh.humidity && cur.humidity != null) out.push('<span class="wv-stat" title="Humidity">' + I.gl('humidity') + '<b>' + cur.humidity + '%</b></span>');
    if (sh.uv && cur.uvIndex != null) out.push('<span class="wv-stat" title="UV index">' + I.gl('sunny') + 'UV <b>' + cur.uvIndex + '</b></span>');
    if (sh.sun && (t.sunrise || t.sunset)) {
      out.push('<span class="wv-stat" title="Sunrise and sunset">' + (t.sunrise ? I.gl('sunrise') + '<b>' + esc(U.time(t.sunrise, tz)) + '</b>' : '') + (t.sunset ? I.gl('sunset') + '<b>' + esc(U.time(t.sunset, tz)) + '</b>' : '') + '</span>');
    }
    return out;
  }
  function stats(s, c) {
    var it = statItems(s, c);
    return it.length ? '<div class="wv-sec wv-stats" data-open="overview">' + it.join('') + '</div>' : '';
  }

  function incomingLine(s) {
    var np = s.nextPrecip;
    if (!np || !np.headline) return '';
    return '<span class="wv-inc-row ' + (np.kind ? 'k-' + np.kind : 'is-dry') + '"><i class="wv-inc-ic">' + I.gl(np.kind ? U.kindGlyph(np.kind) : 'umbrella') + '</i><span class="wv-inc-tx" title="' + esc(np.headline) + '">' + esc(np.headline) + '</span></span>';
  }
  function incoming(s, c, tlHeight) {
    var sh = c.show, np = s.nextPrecip || {};
    var line = sh.incoming ? incomingLine(s) : '';
    var tl = sh.timeline && np.kind ? U.precipTimeline(s.minutely, tzOf(s), { height: tlHeight || 30 }) : '';
    if (!line && !tl) {
      /* timeline on, nothing coming: say so instead of leaving a hole */
      if (sh.timeline && !sh.incoming && np.headline) line = incomingLine(s);
      if (!line) return '';
    }
    return '<div class="wv-sec wv-inc" data-open="overview">' + line + tl + '</div>';
  }

  function hours(s, c, perRow) {
    var hs = (s.hourly || []).slice(0, c.hours);
    if (!hs.length) return '';
    var tz = tzOf(s);
    var anyPop = hs.some(function (h) { return h.precipProb != null && h.precipProb >= 15; });
    return '<div class="wv-sec wv-hours' + (anyPop ? '' : ' no-pop') + '" data-open="hourly" style="--cols:' + Math.min(perRow, hs.length) + '">' + hs.map(function (h, i) {
      var p = h.precipProb != null && h.precipProb >= 15 ? h.precipProb + '%' : '';
      return '<div class="wv-h"><span class="wv-h-t">' + (i === 0 ? 'Now' : esc(U.hour(h.time, tz))) + '</span>' + I.wx(h.icon, h.isDay) +
        '<span class="wv-h-v">' + U.deg(h.temp) + '</span><span class="wv-h-p">' + p + '</span></div>';
    }).join('') + '</div>';
  }

  function days(s, c) {
    var ds = (s.daily || []).slice(0, c.days);
    if (!ds.length) return '';
    var lo = Infinity, hi = -Infinity;
    ds.forEach(function (d) { if (d.low != null) lo = Math.min(lo, d.low); if (d.high != null) hi = Math.max(hi, d.high); });
    var cur = s.current && s.current.temp;
    if (cur != null) { lo = Math.min(lo, cur); hi = Math.max(hi, cur); }
    var span = Math.max(1, hi - lo);
    return '<div class="wv-sec wv-days" data-open="daily">' + ds.map(function (d, i) {
      var bar = '';
      if (d.low != null && d.high != null) {
        var l = ((d.low - lo) / span) * 100, r = ((hi - d.high) / span) * 100;
        bar = '<i style="left:' + l.toFixed(1) + '%;right:' + r.toFixed(1) + '%;background:linear-gradient(90deg,' + U.tempColor(d.low) + ',' + U.tempColor(d.high) + ')"></i>';
        if (i === 0 && cur != null) bar += '<b style="left:' + (((cur - lo) / span) * 100).toFixed(1) + '%"></b>';
      }
      var p = d.precipProb != null && d.precipProb >= 20 ? d.precipProb + '%' : '';
      return '<div class="wv-d"><span class="wv-d-n">' + esc(d.name) + '</span>' + I.wx(d.icon, true) + '<span class="wv-d-p">' + p + '</span>' +
        '<span class="wv-d-lo">' + U.deg(d.low) + '</span><span class="range">' + bar + '</span><span class="wv-d-hi">' + U.deg(d.high) + '</span></div>';
    }).join('') + '</div>';
  }

  /* ---------- states without data ---------- */
  function waitView(s) {
    return '<div class="wv-wait" data-open="overview">' + I.wx('partly-cloudy', true) + '<div><b>' + locName(s) + '</b><span>' + I.gl('refresh', 'spin') + 'Getting the latest forecast…</span></div></div>';
  }
  function errorView(s, o) {
    return '<div class="wv-state" data-open="overview"><span class="wv-state-ring">' + I.gl('offline') + '</span><h2>Weather unavailable</h2>' +
      '<p>' + esc(s.error || 'Something went wrong while getting the forecast.') + '</p>' +
      '<div class="wv-btns"><button class="wv-btn is-primary" data-action="refresh">' + (o.refreshing ? 'Trying…' : 'Try again') + '</button><button class="wv-btn" data-open="settings">Settings</button></div></div>';
  }
  function emptyView() {
    return '<div class="wv-empty" data-open="overview" title="Open WeathaDock">' + I.wx('partly-cloudy', true) + '<span>Open WeathaDock</span></div>';
  }

  /* ---------- composition ---------- */
  function divide(parts) {
    /* parts: [html, group]. A hairline goes between different groups. */
    var out = '', last = null;
    parts.forEach(function (p) {
      if (!p[0]) return;
      out += last !== null && last !== p[1] ? p[0].replace('class="wv-sec', 'class="wv-sec wv-div') : p[0];
      last = p[1];
    });
    return out;
  }

  function build(s, c, o) {
    var sh = c.show, res = { html: '', mode: 'block', minW: 0 };
    if (s.status === 'error' && !s.current) { res.html = errorView(s, o); res.mode = 'state'; return res; }
    if (!s.current) { res.html = waitView(s); res.mode = 'state'; return res; }

    var horizontal = c.orientation === 'horizontal';
    var np = s.nextPrecip || {};
    var hasTimeline = sh.timeline && np.kind && (s.minutely || []).some(function (m) { return m.precip > 0; });
    var hasHours = sh.hourly && (s.hourly || []).length > 0;
    var hasDays = sh.daily && (s.daily || []).length > 0;
    var alert = sh.alerts && s.alerts && s.alerts.length;

    /* inline: horizontal with no tall sections -> one slim row */
    if (horizontal && !hasTimeline && !hasHours && !hasDays) {
      var cur = s.current || {}, meta = metaParts(s, c), items = [];
      if (sh.icon || sh.temp) items.push('<span class="wv-i-now" data-open="overview">' + (sh.icon ? iconPart(s, o, sh.temp) : '') + (sh.temp ? tempPart(s) : '') + '</span>');
      var st1 = (sh.condition && cur.condition ? '<span class="wv-cond">' + esc(cur.condition) + '</span>' : '') + (meta ? '<span class="wv-meta">' + meta + '</span>' : '');
      if (st1) items.push('<span class="wv-i-stack" data-open="overview">' + st1 + '</span>');
      var line = (sh.incoming || sh.timeline) ? incomingLine(s) : '';
      if (line) items.push('<span class="wv-i-inc" data-open="overview">' + line + '</span>');
      var sts = statItems(s, c);
      if (sts.length) items.push('<span class="wv-i-stats" data-open="overview">' + sts.join('') + '</span>');
      var st2 = (sh.location ? locPart(s, o) : '') + (sh.updated ? updPart(s, o, false) : '');
      if (st2) items.push('<span class="wv-i-stack wv-i-place" data-open="overview">' + st2 + '</span>');
      if (alert) items.push(alertChip(s));
      if (sh.refresh) items.push(refreshBtn(s, o));
      if (!items.length) { res.html = emptyView(); res.mode = 'state'; return res; }
      res.mode = 'inline';
      res.html = '<div class="wv-inline">' + items.join('<i class="wv-i-sep"></i>') + '</div>';
      return res;
    }

    var hd = header(s, c, o), nw = current(s, c, o), stt = stats(s, c);
    var banner = alert ? alertBanner(s) : '';

    if (horizontal) {
      var colA = divide([[hd, 'a'], [nw, 'a'], [stt, 'a']]);
      var colB = divide([[incoming(s, c, 30), 'i'], [hasHours ? hours(s, c, c.hours) : '', 'h']]);
      var colC = hasDays ? days(s, c) : '';
      var cols = [];
      if (colA) cols.push('<div class="wv-col wv-col-a">' + colA + '</div>');
      if (colB) cols.push('<div class="wv-col wv-col-b' + (hasHours ? '' : ' no-hours') + '">' + colB + '</div>');
      if (colC) cols.push('<div class="wv-col wv-col-c">' + colC + '</div>');
      if (!cols.length && !banner) { res.html = emptyView(); res.mode = 'state'; return res; }
      res.html = banner + (cols.length ? '<div class="wv-cols">' + cols.join('') + '</div>' : '');
      res.minW = banner && !cols.length ? 260 : 0;
      return res;
    }

    var inc = incoming(s, c, 30);
    var perRow = c.hours > 8 ? 6 : c.hours;
    var body = divide([[hd, 'a'], [nw, 'a'], [stt, 'a'], [inc, 'i'], [hasHours ? hours(s, c, perRow) : '', 'h'], [hasDays ? days(s, c) : '', 'd']]);
    if (!body && !banner) { res.html = emptyView(); res.mode = 'state'; return res; }
    /* sections that shrink-wrap to the panel need a sensible floor */
    var minW = 0;
    if (banner) minW = Math.max(minW, 256);
    if (hd) minW = Math.max(minW, (sh.location ? 132 : 0) + (sh.updated ? (sh.location ? 62 : 112) : 0) + (sh.refresh ? 30 : 0) + 44);
    if (inc) minW = Math.max(minW, hasTimeline ? 284 : 272);
    if (hasDays) minW = Math.max(minW, 290);
    if (stt) minW = Math.max(minW, 236);
    res.minW = minW;
    res.html = banner + '<div class="wv-stack">' + body + '</div>';
    return res;
  }

  function mount(root, state, cfg, opts) {
    opts = opts || {};
    var c = normalize(cfg), r = build(state, c, opts);
    var glass = pick(opts.glass, ['none', 'clear', 'frosted'], 'none');
    var z = ZOOM[c.size] || 1;
    var sig = [glass, c.orientation, c.size, c.align, c.ink, r.mode, r.minW, opts.pending ? 1 : 0, r.html].join('|');
    var changed = root.__wvSig !== sig;
    if (changed) {
      root.__wvSig = sig;
      root.classList.add('wv');
      root.dataset.glass = glass; root.dataset.ink = c.ink; root.dataset.orient = c.orientation;
      root.dataset.align = c.align; root.dataset.size = c.size; root.dataset.mode = r.mode;
      var mt = getMetrics();
      root.style.setProperty('--wv-cap', mt.cap.toFixed(4)); root.style.setProperty('--wv-shift', mt.shift.toFixed(4));
      root.innerHTML = '<div class="wv-panel" style="zoom:' + z + ';' + (r.minW ? 'min-width:' + r.minW + 'px;' : '') + '">' + r.html + '</div>';
      root.__wvZoom = z;
      root.classList.toggle('is-pending', !!opts.pending);
    }
    return changed;
  }

  /* Scale the panel down when the rendered widget would exceed the window limits. */
  function fit(root, maxW, maxH) {
    var panel = root.firstElementChild;
    if (!panel) return;
    var z = root.__wvZoom || 1;
    panel.style.zoom = z;
    panel.style.maxWidth = Math.floor((maxW - 24) / z) + 'px';
    var r = root.getBoundingClientRect();
    if (r.width > maxW || r.height > maxH) {
      var k = Math.min(maxW / r.width, maxH / r.height) * 0.985;
      panel.style.zoom = (z * k).toFixed(3);
      panel.style.maxWidth = Math.floor((maxW - 24) / (z * k)) + 'px';
    }
  }

  window.WDWidget = { KEYS: KEYS, normalize: normalize, presetConfig: presetConfig, mount: mount, fit: fit };
})();
