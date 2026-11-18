/* WeathaDock taskbar pill */
(function () {
  'use strict';
  var U = window.SkyUI, I = window.SkyIcons, esc = U.esc;
  var root = document.getElementById('root');
  var state = null, last = '', dock = false, wantDock = false, room = 0;

  function shortEvent(ev) {
    /* "Severe Thunderstorm Warning" -> "Severe T-Storm Warning"; keep the official wording otherwise */
    return String(ev || 'Alert').replace(/Thunderstorm/i, 'T-Storm');
  }

  function render() {
    var s = state;
    if (!s) return;
    var html, cls = dock ? 'pill is-dock' : 'pill';
    if (!s.current) {
      html = s.status === 'error'
        ? '<span class="p-muted">' + I.gl('offline') + 'Weather unavailable</span>'
        : '<span class="p-muted">' + I.gl('refresh', 'spin') + 'Loading…</span>';
    } else {
      var c = s.current, t = s.today || {}, tz = s.location && s.location.timeZone;
      html = I.wx(c.icon, c.isDay) + '<span class="p-temp">' + U.deg(c.temp) + '</span>';
      if (dock) {
        /* docked on the taskbar: room for the conditions in words and the next few hours */
        html += '<span class="d-tx"><b>' + esc(c.condition || '') + '</b>' +
          (t.high != null || t.low != null ? '<span>H ' + U.deg(t.high) + ' \u00b7 L ' + U.deg(t.low) + '</span>' : '') + '</span>';
      } else if (t.high != null || t.low != null) html += '<span class="p-hl" title="High / low"><b>' + U.deg(t.high) + '</b><i>/</i><span>' + U.deg(t.low) + '</span></span>';
      var a = s.alerts && s.alerts[0];
      var hint = U.shortHint(s.nextPrecip, tz);
      if (a) {
        cls += ' has-alert sev-' + U.sev(a);
        html += '<span class="p-alert" data-open="alerts" title="' + esc(a.event) + '"><i class="p-alert-ic">' + I.gl('alert') + '</i><span>' + esc(shortEvent(a.event)) + '</span></span>';
      } else if (hint) {
        html += '<i class="p-sep"></i><span class="p-hint k-' + s.nextPrecip.kind + '">' + I.gl(U.kindGlyph(s.nextPrecip.kind)) + '<span>' + esc(hint) + '</span></span>';
      }
      if (dock) {
        var hs = (s.hourly || []).slice(1, 5);
        if (hs.length) html += '<i class="p-sep"></i><span class="d-hours" data-open="hourly">' + hs.map(function (h) {
          return '<span class="d-h"><span>' + esc(U.hour(h.time, tz)) + '</span><b>' + I.wx(h.icon, h.isDay) + U.deg(h.temp) + '</b></span>';
        }).join('') + '</span>';
      }
    }
    var sig = cls + html;
    if (sig !== last) { last = sig; root.className = cls; root.innerHTML = html; }
    fit();
    reportSize();
  }

  /* Docked: drop the least important parts until the strip fits the free taskbar space. */
  function fit() {
    root.removeAttribute('data-fit');
    if (!dock) return;
    for (var level = 1; level <= 4 && root.scrollWidth > room; level++) root.setAttribute('data-fit', level);
  }

  function reportSize() {
    var w = Math.ceil(root.getBoundingClientRect().width);
    if (w > 0) window.sky.setContentSize(w, window.innerHeight);
  }

  U.dragOrClick(root, function (target) {
    var el = target.closest && target.closest('[data-open]');
    window.sky.openDetail(el ? el.dataset.open : 'overview');
  });

  U.ready().then(function (sky) {
    var glass = document.documentElement.dataset.glass;
    function apply() {
      dock = wantDock && room > 0;
      /* the docked strip is always bare content, whatever the widget's glass style */
      document.documentElement.dataset.glass = dock ? 'none' : glass;
      last = ''; render();
    }
    function place(st) { wantDock = !!st && st.placement === 'taskbar'; apply(); }
    function bar(d) {
      room = (d && d.room) || 0;
      document.documentElement.dataset.bar = d && d.light ? 'light' : 'dark';
      apply();
    }
    sky.onSettings(place);
    sky.getSettings().then(place);
    if (sky.onDock) { sky.onDock(bar); sky.getDock().then(bar); }
    sky.onState(function (s) { state = s; render(); });
    sky.getState().then(function (s) { state = s; render(); });
    new ResizeObserver(reportSize).observe(root);
    window.addEventListener('resize', reportSize);
  });
})();
