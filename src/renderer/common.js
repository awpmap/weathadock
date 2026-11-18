/* WeathaDock shared renderer helpers (formatting, mock bootstrap, drag-vs-click). */
(function () {
  'use strict';

  var params = new URLSearchParams(location.search);

  /* Resolves once window.sky exists. Loads mock.js when the preload API is absent. */
  function ready() {
    return new Promise(function (resolve) {
      if (window.sky) return resolve(window.sky);
      var s = document.createElement('script');
      s.src = 'mock.js';
      s.onload = function () { resolve(window.sky); };
      s.onerror = function () { resolve(window.sky); };
      document.head.appendChild(s);
    });
  }

  function esc(v) {
    return String(v == null ? '' : v).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  /* ---- time formatting in the location's time zone ---- */
  var fmtCache = {};
  function fmt(tz, key, opts) {
    var k = (tz || '') + '|' + key;
    if (!fmtCache[k]) {
      var o = Object.assign({}, opts);
      if (tz) o.timeZone = tz;
      try { fmtCache[k] = new Intl.DateTimeFormat('en-US', o); }
      catch (e) { delete o.timeZone; fmtCache[k] = new Intl.DateTimeFormat('en-US', o); }
    }
    return fmtCache[k];
  }
  function d(iso) { var t = new Date(iso); return isNaN(t) ? null : t; }
  function time(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'time', { hour: 'numeric', minute: '2-digit' }).format(t) : '–'; }
  function hour(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'hour', { hour: 'numeric' }).format(t) : '–'; }
  function hourNum(iso, tz) { var t = iso && d(iso); return t ? Number(fmt(tz, 'h24', { hour: 'numeric', hourCycle: 'h23' }).format(t)) : 0; }
  function weekday(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'wd', { weekday: 'short' }).format(t) : ''; }
  function weekdayLong(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'wdl', { weekday: 'long' }).format(t) : ''; }
  function monthDay(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'md', { month: 'short', day: 'numeric' }).format(t) : ''; }
  function dayKey(iso, tz) { var t = iso && d(iso); return t ? fmt(tz, 'dk', { year: 'numeric', month: '2-digit', day: '2-digit' }).format(t) : ''; }
  /* "2026-11-18" (a calendar date, no zone) -> Date at local noon, safe to format without tz */
  function dateOnly(s) { var m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s || ''); return m ? new Date(+m[1], +m[2] - 1, +m[3], 12) : null; }
  function dateLabel(s) { var t = dateOnly(s); return t ? fmt('', 'dl', { month: 'short', day: 'numeric' }).format(t) : ''; }
  function dateWeekdayLong(s) { var t = dateOnly(s); return t ? fmt('', 'dwl', { weekday: 'long' }).format(t) : ''; }
  /* "Thu 6:00 PM" when not today, else "6:00 PM" */
  function when(iso, tz, nowIso) {
    if (!iso) return '';
    var same = dayKey(iso, tz) === dayKey(nowIso || new Date().toISOString(), tz);
    return (same ? '' : weekday(iso, tz) + ' ') + time(iso, tz);
  }

  function deg(v) { return v == null || isNaN(v) ? '–' : Math.round(v) + '°'; }
  function num(v, suffix) { return v == null || isNaN(v) ? '–' : v + (suffix || ''); }

  /* temperature -> color (cool blue .. teal .. gold .. orange .. red) for range bars */
  var STOPS = [[0, [150, 170, 255]], [25, [116, 176, 255]], [42, [92, 205, 240]], [56, [108, 224, 186]], [68, [246, 219, 96]], [80, [255, 176, 72]], [95, [255, 110, 78]]];
  function tempColor(t) {
    if (t == null) return 'rgb(180,190,205)';
    if (t <= STOPS[0][0]) return 'rgb(' + STOPS[0][1].join(',') + ')';
    for (var i = 1; i < STOPS.length; i++) {
      if (t <= STOPS[i][0]) {
        var a = STOPS[i - 1], b = STOPS[i], k = (t - a[0]) / (b[0] - a[0]);
        return 'rgb(' + a[1].map(function (c, j) { return Math.round(c + (b[1][j] - c) * k); }).join(',') + ')';
      }
    }
    return 'rgb(' + STOPS[STOPS.length - 1][1].join(',') + ')';
  }

  var SEV_RANK = { Extreme: 4, Severe: 3, Moderate: 2, Minor: 1, Unknown: 0 };
  function sev(a) { return a && SEV_RANK.hasOwnProperty(a.severity) ? a.severity : 'Unknown'; }
  function sevColor(s) {
    return { Extreme: '#ff4f6d', Severe: '#ff7a45', Moderate: '#ffb638', Minor: '#f0d74e', Unknown: '#9db0c9' }[s] || '#9db0c9';
  }

  function kindGlyph(kind) { return { rain: 'drop', snow: 'flake', mix: 'mix', storm: 'bolt' }[kind] || 'sunny'; }
  function kindWord(kind) { return { rain: 'Rain', snow: 'Snow', mix: 'Wintry mix', storm: 'Storms' }[kind] || ''; }

  /* Very short incoming-weather hint for the pill: "Rain in 25 min", "Snow until 3 AM" */
  function shortHint(np, tz) {
    if (!np || !np.kind) return '';
    var w = kindWord(np.kind);
    if (np.isNow) return np.endsAt ? w + ' until ' + hour(np.endsAt, tz) : w + ' now';
    if (np.minutesUntil != null && np.minutesUntil <= 90) return w + ' in ' + Math.max(1, Math.round(np.minutesUntil)) + ' min';
    if (np.startsAt) return w + ' at ' + hour(np.startsAt, tz);
    return w + ' soon';
  }

  /* 6-hour precipitation timeline (15-minute steps). Returns '' when nothing is coming. */
  function precipTimeline(minutely, tz, opts) {
    opts = opts || {};
    var m = (minutely || []).slice(0, 24);
    if (!m.length || !m.some(function (x) { return x.precip > 0; })) return '';
    var H = opts.height || 34;
    var bars = '';
    for (var i = 0; i < m.length; i++) {
      var p = Math.max(0, m[i].precip || 0);
      var k = p <= 0 ? 0 : Math.min(1, 0.16 + 0.84 * Math.sqrt(p / 0.1));
      var kind = m[i].kind || 'rain';
      bars += '<i class="ptl-bar' + (p > 0 ? ' k-' + kind : ' is-dry') + '" style="height:' + (p > 0 ? Math.max(3, Math.round(k * H)) : 2) + 'px"></i>';
    }
    /* tick labels sit at real whole hours (every 2 h), positioned by their step index */
    var ticks = '<span style="left:0">Now</span>';
    var n = m.length, lastLeft = 0;
    for (var j = 3; j < n; j++) {
      var tt = new Date(m[j].time);
      if (tt.getUTCMinutes() % 60 !== 0 && tt.getMinutes() !== 0) continue;
      if (hourNum(m[j].time, tz) % 2 !== 0) continue;
      var left = (j / n) * 100;
      if (left - lastLeft < 18) continue;
      lastLeft = left;
      ticks += '<span class="' + (left > 88 ? 'is-end' : 'is-mid') + '" style="left:' + left.toFixed(2) + '%">' + esc(hour(m[j].time, tz)) + '</span>';
    }
    return '<div class="ptl" role="img" aria-label="Precipitation over the next 6 hours"><div class="ptl-bars" style="height:' + H + 'px">' + bars + '</div><div class="ptl-ticks">' + ticks + '</div></div>';
  }

  /* Pointer handling for widget + pill: drag moves the window, a press without movement is a click.
     onClick(el) receives the element that was pressed. isLocked() -> true disables dragging.
     Right-click asks main for the native context menu. */
  function dragOrClick(root, onClick, isLocked) {
    var st = null;
    root.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      if (window.sky && window.sky.showContextMenu) window.sky.showContextMenu();
    });
    root.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      st = { id: e.pointerId, x: e.screenX, y: e.screenY, lx: e.screenX, ly: e.screenY, drag: false, locked: !!(isLocked && isLocked()), target: e.target };
      try { root.setPointerCapture(e.pointerId); } catch (err) {}
    });
    root.addEventListener('pointermove', function (e) {
      if (!st || e.pointerId !== st.id) return;
      if (st.locked) return;
      if (!st.drag) {
        if (Math.abs(e.screenX - st.x) <= 4 && Math.abs(e.screenY - st.y) <= 4) return;
        st.drag = true;
        root.classList.add('is-dragging');
      }
      var dx = e.screenX - st.lx, dy = e.screenY - st.ly;
      if (dx || dy) { st.lx = e.screenX; st.ly = e.screenY; window.sky && window.sky.moveBy(dx, dy); }
    });
    function end(e, cancelled) {
      if (!st || e.pointerId !== st.id) return;
      var s = st; st = null;
      try { root.releasePointerCapture(e.pointerId); } catch (err) {}
      root.classList.remove('is-dragging');
      if (s.drag) { window.sky && window.sky.dragEnd(); }
      else if (!cancelled) onClick(s.target);
    }
    root.addEventListener('pointerup', function (e) { end(e, false); });
    root.addEventListener('pointercancel', function (e) { end(e, true); });
    root.addEventListener('dragstart', function (e) { e.preventDefault(); });
  }

  function watchVisibility() {
    var f = function () { document.documentElement.classList.toggle('is-hidden', document.visibilityState === 'hidden'); };
    document.addEventListener('visibilitychange', f); f();
  }

  window.SkyUI = {
    params: params, ready: ready, esc: esc,
    time: time, hour: hour, hourNum: hourNum, weekday: weekday, weekdayLong: weekdayLong, monthDay: monthDay, dayKey: dayKey,
    dateLabel: dateLabel, dateWeekdayLong: dateWeekdayLong, when: when,
    deg: deg, num: num, tempColor: tempColor, sev: sev, sevColor: sevColor,
    kindGlyph: kindGlyph, kindWord: kindWord, shortHint: shortHint, precipTimeline: precipTimeline,
    dragOrClick: dragOrClick, watchVisibility: watchVisibility
  };
})();
