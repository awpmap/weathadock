/* WeathaDock shared renderer helpers (formatting, mock bootstrap, drag-vs-click). */
(function () {
  'use strict';

  var params = new URLSearchParams(location.search);

  /* Resolves once window.sky exists. Inside the app the bridge defines it before any page script
     runs. The sample weather in mock.js is for design previews in a plain browser only, so the
     app itself can never show it. */
  function ready() {
    return new Promise(function (resolve) {
      if (window.sky) return resolve(window.sky);
      if (window.__TAURI_INTERNALS__ || /^tauri/i.test(location.hostname) || /^tauri/i.test(location.protocol)) {
        var tries = 0, wait = setInterval(function () {
          if (window.sky) { clearInterval(wait); resolve(window.sky); }
          else if (++tries > 200) clearInterval(wait);
        }, 50);
        return;
      }
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

  /* whole degrees with a true minus sign (U+2212), which sits level with the digits */
  function whole(v) { return String(Math.round(v)).replace('-', '\u2212'); }
  function deg(v) { return v == null || isNaN(v) ? '–' : whole(v) + '°'; }
  /* the present moment; design previews pin it just after the sample data's own timestamp */
  function now(s) { return window.sky && window.sky.__mock && s && s.updatedAt ? new Date(s.updatedAt).getTime() + 120000 : Date.now(); }
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
    /* from the hourly chance, not from minute-by-minute timing: no minute counts */
    if (np.chance != null) return np.startsAt && np.minutesUntil > 0 ? w + ' around ' + hour(np.startsAt, tz) : w + ' likely';
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
      if (st.locked) { if (Math.abs(e.screenX - st.x) > 4 || Math.abs(e.screenY - st.y) > 4) st.moved = true; return; }
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
      else if (!cancelled && !s.moved) onClick(s.target);
    }
    root.addEventListener('pointerup', function (e) { end(e, false); });
    root.addEventListener('pointercancel', function (e) { end(e, true); });
    root.addEventListener('dragstart', function (e) { e.preventDefault(); });
  }

  /* 64x64 tray icon: temperature on a sky-blue tile, alert dot. The widget keeps it current; docked
     on the taskbar there is no widget, so the strip does, for when the taskbar has no room for it. */
  var lastTray = '';
  function tray(s) {
    var temp = s.current && s.current.temp != null ? String(Math.round(s.current.temp)) : null;
    var a = s.alerts && s.alerts[0];
    var sig = temp + '|' + (a ? sev(a) : '');
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
      g.fillStyle = sevColor(sev(a)); g.fill();
      g.strokeStyle = 'rgba(0,0,0,0.65)'; g.lineWidth = 3; g.stroke();
    }
    try { window.sky.setTrayIcon(c.toDataURL('image/png')); } catch (e) {}
  }

  function watchVisibility() {
    var f = function () { document.documentElement.classList.toggle('is-hidden', document.visibilityState === 'hidden'); };
    document.addEventListener('visibilitychange', f); f();
  }

  window.SkyUI = {
    params: params, ready: ready, esc: esc,
    time: time, hour: hour, hourNum: hourNum, weekday: weekday, weekdayLong: weekdayLong, monthDay: monthDay, dayKey: dayKey,
    dateLabel: dateLabel, dateWeekdayLong: dateWeekdayLong, when: when,
    deg: deg, whole: whole, now: now, num: num, tempColor: tempColor, sev: sev, sevColor: sevColor,
    kindGlyph: kindGlyph, kindWord: kindWord, shortHint: shortHint, precipTimeline: precipTimeline,
    dragOrClick: dragOrClick, watchVisibility: watchVisibility, tray: tray
  };
})();
