/* WeathaDock "Customize widget" editor (detail window).
   WDCustomize.init(sectionEl, { commit(nextSettings, patch) })   builds the editor once
   WDCustomize.update(state, settings)                            syncs controls + live preview
   The preview and the style thumbnails are drawn by the same renderer as the desktop widget. */
(function () {
  'use strict';
  var I = window.SkyIcons, W = window.WDWidget, PRESETS = window.WD_PRESETS || [];
  var el = null, api = null, state = null, settings = null;
  var backdrop = 'dark', basePreset = null, opTimer = 0, built = false;

  var GROUPS = [
    ['Header', [['location', 'Location'], ['updated', 'Updated time'], ['refresh', 'Refresh button']]],
    ['Right now', [['icon', 'Icon'], ['temp', 'Temperature'], ['condition', 'Condition'], ['highLow', 'High / low'], ['feelsLike', 'Feels like']]],
    ['Details', [['wind', 'Wind'], ['humidity', 'Humidity'], ['uv', 'UV index'], ['sun', 'Sunrise & sunset']]],
    ['Coming up', [['incoming', 'Rain & snow heads-up'], ['timeline', 'Rain timeline'], ['hourly', 'Hourly'], ['daily', 'Daily']]],
    ['Alerts', [['alerts', 'Weather alerts']]]
  ];

  function $(sel) { return el.querySelector(sel); }
  function $$(sel) { return Array.prototype.slice.call(el.querySelectorAll(sel)); }
  function clone(o) { return JSON.parse(JSON.stringify(o)); }
  function preset(id) { for (var i = 0; i < PRESETS.length; i++) if (PRESETS[i].id === id) return PRESETS[i]; return null; }
  function config() { var s = settings || {}; return W.normalize(s.widget || W.presetConfig(s.layout) || W.presetConfig('full')); }

  /* the preset a custom layout is closest to: what "Reset" goes back to after a restart */
  function nearestPreset(c) {
    var best = PRESETS[0], bestScore = -1;
    PRESETS.forEach(function (p) {
      var n = W.normalize(p.widget), score = (n.orientation === c.orientation ? 6 : 0);
      W.KEYS.forEach(function (k) { if (n.show[k] === c.show[k]) score++; });
      if (score > bestScore) { bestScore = score; best = p; }
    });
    return best;
  }

  function seg(key, label, opts) {
    return '<div class="seg" role="radiogroup" aria-label="' + label + '" data-cz="' + key + '">' + opts.map(function (o) {
      return '<button role="radio" data-val="' + o[0] + '"' + (o[2] ? ' title="' + o[1] + '" aria-label="' + o[1] + '"' : '') + '>' + (o[2] ? I.gl(o[2]) : o[1]) + '</button>';
    }).join('') + '</div>';
  }
  function row(name, ctl, sub) {
    return '<div class="cz-row"><div class="cz-row-tx"><div class="cz-row-n">' + name + '</div>' + (sub ? '<div class="cz-row-s">' + sub + '</div>' : '') + '</div>' + ctl + '</div>';
  }

  function buildDom() {
    var styles = PRESETS.map(function (p) {
      return '<button class="cz-style" role="radio" data-preset="' + p.id + '" title="' + p.description + '">' +
        '<span class="cz-thumb"><span class="cz-scale"><span class="wv" data-thumb="' + p.id + '"></span></span></span>' +
        '<span class="cz-style-n">' + p.name + '</span><span class="cz-style-d">' + p.description + '</span></button>';
    }).join('');

    var show = GROUPS.map(function (g) {
      var chips = g[1].map(function (it) {
        return '<button class="cz-chip" role="switch" data-k="' + it[0] + '">' + I.gl('check') + '<span>' + it[1] + '</span></button>';
      }).join('');
      var extra = '';
      if (g[0] === 'Coming up') {
        extra = '<div class="cz-counts">' +
          '<label class="cz-count" data-for="hourly"><span>Hours</span>' + seg('hours', 'Hours shown', [[4, '4'], [6, '6'], [8, '8'], [12, '12']]) + '</label>' +
          '<label class="cz-count" data-for="daily"><span>Days</span>' + seg('days', 'Days shown', [[3, '3'], [5, '5'], [7, '7']]) + '</label></div>';
      }
      return '<div class="cz-group"><div class="cz-group-n">' + g[0] + '</div><div class="cz-group-b"><div class="cz-chips">' + chips + '</div>' + extra + '</div></div>';
    }).join('');

    el.innerHTML =
      '<div class="cz">' +
        '<aside class="cz-preview" aria-label="Live preview">' +
          '<div class="cz-stage" data-bg="dark"><span class="cz-scale"><span class="wv" id="cz-wv"></span></span></div>' +
          '<div class="cz-stage-bar"><span class="cz-live"><i></i>Live preview</span>' +
            '<div class="cz-bgs" role="radiogroup" aria-label="Preview backdrop">' +
              '<button role="radio" data-bg="dark" title="Dark wallpaper" aria-label="Dark wallpaper"></button>' +
              '<button role="radio" data-bg="bright" title="Bright wallpaper" aria-label="Bright wallpaper"></button>' +
              '<button role="radio" data-bg="photo" title="Photo wallpaper" aria-label="Photo wallpaper"></button>' +
            '</div></div>' +
        '</aside>' +
        '<div class="cz-controls">' +
          '<div class="cz-h"><h2 class="group-title">Style</h2><span class="cz-status" id="cz-status"></span>' +
            '<button class="btn cz-reset" id="cz-reset">' + I.gl('restart') + '<span id="cz-reset-tx">Reset to preset</span></button></div>' +
          '<div class="cz-styles" role="radiogroup" aria-label="Widget style">' + styles + '</div>' +

          '<h2 class="group-title">Look</h2>' +
          '<div class="card cz-card">' +
            row('Background', seg('glass', 'Background', [['none', 'Transparent'], ['clear', 'Tinted glass'], ['frosted', 'Frosted']])) +
            row('Text color', seg('ink', 'Text color', [['light', 'Light'], ['dark', 'Dark']]), 'Dark suits bright wallpapers.') +
            row('Opacity', '<div class="cz-range"><input type="range" id="cz-op" min="30" max="100" step="5" aria-label="Widget opacity"><output id="cz-op-v"></output></div>') +
          '</div>' +

          '<h2 class="group-title">Arrangement</h2>' +
          '<div class="card cz-card">' +
            row('Direction', seg('orientation', 'Direction', [['vertical', 'Vertical'], ['horizontal', 'Horizontal']])) +
            row('Size', seg('size', 'Size', [['small', 'Small'], ['medium', 'Medium'], ['large', 'Large'], ['xlarge', 'XL']])) +
            row('Alignment', seg('align', 'Alignment', [['left', 'Left', 'al-left'], ['center', 'Center', 'al-center'], ['right', 'Right', 'al-right']])) +
          '</div>' +

          '<h2 class="group-title">What to show</h2>' +
          '<div class="card cz-card cz-show">' + show + '</div>' +
        '</div>' +
      '</div>';

    /* ---- events ---- */
    el.addEventListener('click', function (e) {
      var b;
      if ((b = e.target.closest('.cz-style'))) { applyPreset(b.dataset.preset); return; }
      if ((b = e.target.closest('.cz-chip'))) { var sh = {}; sh[b.dataset.k] = b.getAttribute('aria-checked') !== 'true'; changeWidget({ show: sh }); return; }
      if ((b = e.target.closest('.cz-bgs button'))) { backdrop = b.dataset.bg; syncPreview(); return; }
      if ((b = e.target.closest('[data-cz] button'))) { pickSeg(b.parentNode.dataset.cz, b.dataset.val); return; }
      if (e.target.closest('#cz-reset') && basePreset) applyPreset(basePreset);
    });
    el.addEventListener('keydown', function (e) {
      var g = e.target.closest && e.target.closest('[role="radiogroup"]');
      if (!g || ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].indexOf(e.key) < 0) return;
      var bs = Array.prototype.slice.call(g.querySelectorAll('button')), i = bs.indexOf(e.target);
      if (i < 0) return;
      e.preventDefault();
      var n = bs[(i + (e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 1) + bs.length) % bs.length];
      n.focus(); n.click();
    });
    var op = $('#cz-op');
    op.addEventListener('input', function () {
      paintOpacity(Number(op.value));
      clearTimeout(opTimer);
      opTimer = setTimeout(function () { commitOpacity(Number(op.value)); }, 90);
    });
    op.addEventListener('change', function () { clearTimeout(opTimer); commitOpacity(Number(op.value)); });
    if (window.ResizeObserver) new ResizeObserver(function () { if (!el.hidden) { fitPreview(); fitThumbs(); } }).observe($('.cz-stage'));
    built = true;
  }

  /* ---- changes ---- */
  function send(next, patch) { settings = next; sync(); api.commit(next, patch); }
  function applyPreset(id) {
    var p = preset(id);
    if (!p) return;
    basePreset = id;
    var next = clone(settings || {});
    next.widget = clone(p.widget); next.layout = id;
    send(next, { widget: clone(p.widget), layout: id });
  }
  function changeWidget(part) {
    var c = config(), next = clone(settings || {});
    var w = Object.assign({}, c, part);
    w.show = Object.assign({}, c.show, part.show || {});
    next.widget = w; next.layout = 'custom';
    send(next, { widget: part, layout: 'custom' });
  }
  function pickSeg(key, val) {
    if (key === 'glass') {
      if ((settings || {}).glass === val) return;
      var next = clone(settings || {}); next.glass = val;
      send(next, { glass: val });
      return;
    }
    var c = config(), part = {};
    part[key] = key === 'hours' || key === 'days' ? Number(val) : val;
    if (c[key] === part[key]) return;
    if (key === 'ink') { /* show the new ink on a wallpaper it is meant for */
      if (val === 'dark' && backdrop === 'dark') backdrop = 'bright';
      if (val === 'light' && backdrop === 'bright') backdrop = 'dark';
    }
    changeWidget(part);
  }
  function opacity() { var v = Number((settings || {}).widgetOpacity); return v >= 30 && v <= 100 ? Math.round(v) : 100; }
  function commitOpacity(v) {
    if (opacity() === v) return;
    var next = clone(settings || {}); next.widgetOpacity = v;
    send(next, { widgetOpacity: v });
  }
  function paintOpacity(v) {
    var op = $('#cz-op');
    op.style.setProperty('--p', ((v - 30) / 70 * 100).toFixed(1) + '%');
    $('#cz-op-v').textContent = v + '%';
    $('.cz-stage .cz-scale').style.opacity = v / 100;
  }

  /* ---- sync controls + preview with settings ---- */
  function setSeg(key, val) {
    $$('[data-cz="' + key + '"] button').forEach(function (b) {
      var on = b.dataset.val === String(val);
      b.setAttribute('aria-checked', on ? 'true' : 'false'); b.tabIndex = on ? 0 : -1;
    });
  }
  function sync() {
    if (!built || !settings) return;
    var c = config(), s = settings, lay = s.layout, p = preset(lay);
    if (p) basePreset = p.id; else if (!basePreset) basePreset = nearestPreset(c).id;
    var base = preset(basePreset);

    $$('.cz-style').forEach(function (b) { var on = b.dataset.preset === lay; b.setAttribute('aria-checked', on ? 'true' : 'false'); b.tabIndex = on || (!p && b.dataset.preset === basePreset) ? 0 : -1; });
    $('#cz-status').textContent = p ? p.name : 'Custom';
    $('#cz-status').classList.toggle('is-custom', !p);
    $('#cz-reset').disabled = !!p;
    $('#cz-reset-tx').textContent = 'Reset to ' + (base ? base.name : 'preset');

    setSeg('glass', s.glass === 'clear' || s.glass === 'frosted' ? s.glass : 'none');
    ['ink', 'orientation', 'size', 'align', 'hours', 'days'].forEach(function (k) { setSeg(k, c[k]); });
    $$('.cz-chip').forEach(function (b) { b.setAttribute('aria-checked', c.show[b.dataset.k] ? 'true' : 'false'); });
    $$('.cz-count').forEach(function (l) { l.classList.toggle('is-off', !c.show[l.dataset.for]); });
    var op = $('#cz-op');
    if (document.activeElement !== op || Number(op.value) !== opacity()) { if (!op.matches(':active')) op.value = opacity(); }
    paintOpacity(Number(op.value));
    syncPreview();
  }

  function fitInto(scaleEl, box, pad, max) {
    var w = scaleEl.offsetWidth, h = scaleEl.offsetHeight;
    if (!w || !h || !box.clientWidth) return;
    var k = Math.min(max, (box.clientWidth - pad * 2) / w, (box.clientHeight - pad * 2) / h);
    scaleEl.style.transform = 'translate(-50%, -50%) scale(' + k.toFixed(3) + ')';
  }
  function fitPreview() { fitInto($('.cz-stage .cz-scale'), $('.cz-stage'), 14, 1); }
  function fitThumbs() { $$('.cz-thumb').forEach(function (t) { fitInto(t.firstChild, t, 6, 0.42); }); }

  function syncPreview() {
    if (!built || !settings || !state || el.hidden) return;
    var stage = $('.cz-stage');
    stage.dataset.bg = backdrop;
    $$('.cz-bgs button').forEach(function (b) { var on = b.dataset.bg === backdrop; b.setAttribute('aria-checked', on ? 'true' : 'false'); b.tabIndex = on ? 0 : -1; });
    var root = $('#cz-wv');
    var glass = settings.glass === 'clear' || settings.glass === 'frosted' ? settings.glass : 'none';
    if (W.mount(root, state, config(), { glass: glass, places: null })) W.fit(root, 900, 900);
    fitPreview();
    /* thumbnails: every style drawn with today's weather */
    var changed = false;
    $$('[data-thumb]').forEach(function (t) {
      var c = clone(preset(t.dataset.thumb).widget); c.size = 'medium';
      if (W.mount(t, state, c, { glass: 'none', places: null, animate: false })) changed = true;
    });
    if (changed || !el.__czFit) { fitThumbs(); el.__czFit = true; }
  }

  window.WDCustomize = {
    init: function (sectionEl, a) { el = sectionEl; api = a; buildDom(); },
    update: function (st, se) { state = st || state; settings = se || settings; sync(); }
  };
})();
