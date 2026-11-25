/* WeathaDock icon set: hand-built inline SVG, no fonts, no external assets.
   SkyIcons.wx(key, isDay, {animated}) -> HTML string for a weather icon sized 1em (set font-size).
   SkyIcons.gl(name) -> HTML string for a small monochrome UI glyph (currentColor). */
(function () {
  'use strict';

  var DEFS =
    '<svg id="sk-defs" width="0" height="0" style="position:absolute;width:0;height:0;overflow:hidden" aria-hidden="true"><defs>' +
    '<radialGradient id="sk-sun" cx="38%" cy="34%" r="75%"><stop offset="0" stop-color="#FFF0A3"/><stop offset="0.55" stop-color="#FFCB45"/><stop offset="1" stop-color="#FF9F1C"/></radialGradient>' +
    '<linearGradient id="sk-ray" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#FFD866"/><stop offset="1" stop-color="#FFAE2E"/></linearGradient>' +
    '<linearGradient id="sk-moon" x1="0.2" y1="0" x2="0.8" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#B4C0F2"/></linearGradient>' +
    '<linearGradient id="sk-cloud" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFFFFF"/><stop offset="1" stop-color="#C9D5E8"/></linearGradient>' +
    '<linearGradient id="sk-cloud-back" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#C5CEDD"/><stop offset="1" stop-color="#93A0B6"/></linearGradient>' +
    '<linearGradient id="sk-cloud-dark" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#D9E0EC"/><stop offset="1" stop-color="#8793AA"/></linearGradient>' +
    '<linearGradient id="sk-rain" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#9BD6FF"/><stop offset="1" stop-color="#3F9BFF"/></linearGradient>' +
    '<linearGradient id="sk-bolt" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFE873"/><stop offset="1" stop-color="#FFA21A"/></linearGradient>' +
    '<linearGradient id="sk-wind" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.55"/><stop offset="1" stop-color="#FFFFFF"/></linearGradient>' +
    '</defs></svg>';

  function ensureDefs() {
    if (document.getElementById('sk-defs') || !document.body) return;
    var d = document.createElement('div');
    d.innerHTML = DEFS;
    document.body.insertBefore(d.firstChild, document.body.firstChild);
  }

  var CLOUD = 'M19 48H44A10 10 0 0 0 46.6 28.3 14.6 14.6 0 0 0 17.4 30.1 9 9 0 0 0 19 48Z';

  function cloud(dx, dy, s, fill) {
    return '<path d="' + CLOUD + '" transform="translate(' + dx + ' ' + dy + ') scale(' + s + ')" fill="url(#' + (fill || 'sk-cloud') + ')"/>';
  }
  function sun(cx, cy, r) {
    var out = '<g stroke="#FFC340" stroke-width="' + (r * 0.24).toFixed(2) + '" stroke-linecap="round">';
    for (var i = 0; i < 8; i++) {
      var a = (i * Math.PI) / 4, c = Math.cos(a), s = Math.sin(a);
      var r1 = r * 1.42, r2 = r * 1.85;
      out += '<line x1="' + (cx + c * r1).toFixed(2) + '" y1="' + (cy + s * r1).toFixed(2) + '" x2="' + (cx + c * r2).toFixed(2) + '" y2="' + (cy + s * r2).toFixed(2) + '"/>';
    }
    return out + '</g><circle cx="' + cx + '" cy="' + cy + '" r="' + r + '" fill="url(#sk-sun)"/>';
  }
  function moon(cx, cy, s) {
    return '<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z" transform="translate(' + (cx - 12 * s).toFixed(2) + ' ' + (cy - 12 * s).toFixed(2) + ') scale(' + s + ')" fill="url(#sk-moon)"/>';
  }
  function star(x, y, s) {
    var k = s * 0.26;
    return '<path fill="#FFFFFF" d="M' + x + ' ' + (y - s) + 'L' + (x + k) + ' ' + (y - k) + 'L' + (x + s) + ' ' + y + 'L' + (x + k) + ' ' + (y + k) + 'L' + x + ' ' + (y + s) + 'L' + (x - k) + ' ' + (y + k) + 'L' + (x - s) + ' ' + y + 'L' + (x - k) + ' ' + (y - k) + 'Z"/>';
  }
  function drop(x, y, len) {
    return '<line x1="' + x + '" y1="' + y + '" x2="' + (x - len * 0.34).toFixed(2) + '" y2="' + (y + len) + '" stroke="url(#sk-rain)" stroke-width="3.2" stroke-linecap="round"/>';
  }
  function flake(x, y, r) {
    return '<circle cx="' + x + '" cy="' + y + '" r="' + (r || 2.5) + '" fill="#FFFFFF"/>';
  }

  /* each layer: [animClass, svgBody, transformOrigin] */
  function layers(key, day) {
    var body = function (cx, cy, r) {
      return day
        ? ['wx-a-spin', sun(cx, cy, r), (cx / 0.64).toFixed(1) + '% ' + (cy / 0.64).toFixed(1) + '%']
        : ['', moon(cx, cy, r / 6.4), ''];
    };
    switch (key) {
      case 'clear':
        return day
          ? [['wx-a-spin', sun(32, 32, 13.5), '50% 50%']]
          : [['', moon(30, 34, 2.05), ''], ['wx-a-twinkle', star(47, 17, 5) + star(53, 33, 3), '']];
      case 'mostly-clear':
        return [body(day ? 28 : 27, day ? 27 : 28, day ? 11.5 : 11.5), ['wx-a-drift', cloud(27.5, 22.5, 0.58), '']];
      case 'partly-cloudy':
        return [body(day ? 23 : 22, day ? 22 : 21, day ? 9.5 : 10.5), ['wx-a-drift', cloud(7, 8, 0.9), '']];
      case 'cloudy':
        return [['wx-a-drift', cloud(22.5, 5.5, 0.66, 'sk-cloud-back'), ''], ['', cloud(1.5, 9, 0.92), '']];
      case 'fog':
        return [['', cloud(3.2, -3, 0.9), ''],
          ['wx-a-gust', '<g stroke="#FFFFFF" stroke-opacity="0.78" stroke-width="3.2" stroke-linecap="round"><line x1="13" y1="48" x2="51" y2="48"/><line x1="20" y1="56.5" x2="44" y2="56.5"/></g>', '']];
      case 'drizzle':
        return [['wx-a-fall-slow', drop(27, 46, 6.5) + drop(40, 46, 6.5), ''], ['', cloud(3.2, -5, 0.9), '']];
      case 'rain':
        return [['wx-a-fall', drop(23, 45, 10) + drop(34, 45, 10) + drop(45, 45, 10), ''], ['', cloud(3.2, -6, 0.9, 'sk-cloud-dark'), '']];
      case 'heavy-rain':
        return [['wx-a-fall', drop(19, 44, 13) + drop(29, 44, 13) + drop(39, 44, 13) + drop(49, 44, 13), ''], ['', cloud(3.2, -7, 0.9, 'sk-cloud-dark'), '']];
      case 'thunderstorm':
        return [['wx-a-fall', drop(20, 45, 10) + drop(48, 45, 10), ''], ['', cloud(3.2, -7, 0.9, 'sk-cloud-dark'), ''],
          ['wx-a-flash', '<path d="M35.5 33 26 47.5h7.2L30 60l12-16h-7.4l4.4-11Z" fill="url(#sk-bolt)" stroke="#8a5a00" stroke-opacity="0.18" stroke-width="0.6"/>', '']];
      case 'snow':
        return [['wx-a-snow', flake(21, 48) + flake(32, 55) + flake(43, 48) + flake(27, 59.5, 1.7) + flake(38.5, 45.5, 1.7), ''], ['', cloud(3.2, -7, 0.9), '']];
      case 'sleet':
        return [['wx-a-fall-slow', drop(23, 45, 9) + flake(33, 52) + drop(46, 45, 9) + flake(41, 58.5, 1.8), ''], ['', cloud(3.2, -6, 0.9, 'sk-cloud-dark'), '']];
      case 'wind':
        return [['wx-a-gust', '<g fill="none" stroke="url(#sk-wind)" stroke-width="3.4" stroke-linecap="round"><path d="M9 24H36a6.2 6.2 0 1 0-6.2-6.2"/><path d="M9 34H47a7 7 0 1 1-7 7"/><path d="M15 44H27a5 5 0 1 1-5 5"/></g>', '']];
      default:
        return [['', cloud(1.5, 2, 0.92), '']];
    }
  }

  /* Visual extent of each icon's artwork inside its 64-unit box: [top, bottom] for day, night.
     Used to size an icon optically against text (see widget-view.js). */
  var BOUNDS = {
    clear: [[7, 57], [12, 52.5]], 'mostly-clear': [[5.7, 50.3], [11.8, 50.3]], 'partly-cloudy': [[4.4, 51.2], [6.2, 51.2]],
    cloudy: [[15.1, 53.2]], fog: [[10.1, 58.1]], drizzle: [[8.1, 54.1]], rain: [[7.1, 56.6]], 'heavy-rain': [[6.1, 58.6]],
    thunderstorm: [[6.1, 60]], snow: [[6.1, 61.2]], sleet: [[7.1, 60.3]], wind: [[9.9, 55.7]]
  };
  function bounds(key, isDay) {
    var b = BOUNDS[key] || BOUNDS.cloudy;
    return b[isDay === false && b[1] ? 1 : 0];
  }

  var SVG_OPEN = '<svg viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">';

  function wx(key, isDay, opts) {
    ensureDefs();
    var ls = layers(key, isDay !== false);
    if (opts && opts.animated) {
      var html = '<span class="wx wx-anim" data-icon="' + key + '">';
      for (var i = 0; i < ls.length; i++) {
        html += '<span class="wx-layer ' + ls[i][0] + '"' + (ls[i][2] ? ' style="transform-origin:' + ls[i][2] + '"' : '') + '>' + SVG_OPEN + ls[i][1] + '</svg></span>';
      }
      return html + '</span>';
    }
    return '<span class="wx" data-icon="' + key + '">' + SVG_OPEN + ls.map(function (l) { return l[1]; }).join('') + '</svg></span>';
  }

  /* ---- UI glyphs: 20x20, 1.5 stroke, currentColor ---- */
  var G = {
    'chevron-right': '<path d="M7.5 4.5 13 10l-5.5 5.5"/>',
    'chevron-down': '<path d="M4.5 7.5 10 13l5.5-5.5"/>',
    alert: '<path d="M10 3.2 17.4 16H2.6L10 3.2Z"/><path d="M10 8.2v3.6"/><circle cx="10" cy="13.9" r="0.6" fill="currentColor" stroke="none"/>',
    refresh: '<path d="M16 10a6 6 0 1 1-1.9-4.4"/><path d="M16.2 3.4v3.4h-3.4"/>',
    drop: '<path d="M10 3.2c2.6 3.1 4.6 5.6 4.6 8.1a4.6 4.6 0 0 1-9.2 0c0-2.5 2-5 4.6-8.1Z"/>',
    flake: '<path d="M10 3v14M3.9 6.5l12.2 7M16.1 6.5l-12.2 7"/>',
    bolt: '<path d="M11 2.8 5 11h4.2L8.4 17.2 15 8.8h-4.4L11 2.8Z"/>',
    mix: '<path d="M6.2 4.2c1.5 1.8 2.7 3.3 2.7 4.8a2.7 2.7 0 0 1-5.4 0c0-1.5 1.2-3 2.7-4.8Z"/><path d="M13.6 9.6v7M10.6 11.3l6 3.5M16.6 11.3l-6 3.5"/>',
    sunny: '<circle cx="10" cy="10" r="3.2"/><path d="M10 2.6v1.8M10 15.6v1.8M2.6 10h1.8M15.6 10h1.8M4.8 4.8 6 6M14 14l1.2 1.2M15.2 4.8 14 6M6 14l-1.2 1.2"/>',
    wind: '<path d="M3 7.5h8.2a2.2 2.2 0 1 0-2.2-2.2M3 10.5h11.6a2.4 2.4 0 1 1-2.4 2.4M3 13.5h5.2a1.9 1.9 0 1 1-1.9 1.9"/>',
    humidity: '<path d="M10 3.2c2.6 3.1 4.6 5.6 4.6 8.1a4.6 4.6 0 0 1-9.2 0c0-2.5 2-5 4.6-8.1Z"/><path d="M7.8 11.6a2.3 2.3 0 0 0 2.2 2.1"/>',
    dew: '<path d="M6.5 3.5c1.7 2 3 3.7 3 5.3a3 3 0 0 1-6 0c0-1.6 1.3-3.3 3-5.300Z"/><path d="M13.5 9c1.3 1.6 2.3 2.9 2.3 4.200a2.3 2.3 0 0 1-4.6 0c0-1.3 1-2.6 2.3-4.200Z"/>',
    gauge: '<path d="M3.6 14.500a7 7 0 1 1 12.8 0"/><path d="m10 11 3-4"/><circle cx="10" cy="11" r="1" fill="currentColor" stroke="none"/>',
    eye: '<path d="M2.4 10S5.2 5 10 5s7.6 5 7.6 5-2.8 5-7.6 5-7.6-5-7.6-5Z"/><circle cx="10" cy="10" r="2.3"/>',
    thermo: '<path d="M8.2 11.800V4.600a1.8 1.8 0 0 1 3.6 0v7.200a3.3 3.3 0 1 1-3.6 0Z"/><path d="M10 8v6"/>',
    sunrise: '<path d="M3 15.500h14M6.2 12.500a3.8 3.8 0 0 1 7.6 0M10 3.500v3.200M7.8 5.4 10 3.200l2.2 2.200M3.8 9.300l1.2 1M16.2 9.300l-1.2 1"/>',
    sunset: '<path d="M3 15.500h14M6.2 12.500a3.8 3.8 0 0 1 7.6 0M10 3.200v3.300M7.8 4.5 10 6.700l2.2-2.200M3.8 9.300l1.2 1M16.2 9.300l-1.2 1"/>',
    home: '<path d="M3.5 9.2 10 3.600l6.5 5.600V16a.8.8 0 0 1-.8.8H12v-4.600H8v4.600H4.300a.8.8 0 0 1-.8-.8V9.200Z"/>',
    clock: '<circle cx="10" cy="10" r="6.8"/><path d="M10 6v4.200l2.8 1.7"/>',
    calendar: '<rect x="3.2" y="4.4" width="13.6" height="12.4" rx="2"/><path d="M3.2 8.400h13.600M7 2.800v3M13 2.800v3"/>',
    gear: '<circle cx="10" cy="10" r="2.4"/><path d="M10 2.800l1.1 2 2.2-.5.7 2.1 2.1.9-.9 2 .9 2-2.1.9-.700 2.1-2.2-.500-1.1 2-1.1-2-2.2.5-.700-2.1-2.1-.900.9-2-.900-2 2.1-.900.7-2.1 2.2.5 1.1-2Z"/>',
    check: '<path d="m4.5 10.5 3.6 3.6 7.4-8"/>',
    'check-circle': '<circle cx="10" cy="10" r="7"/><path d="m6.8 10.2 2.3 2.3 4.2-4.6"/>',
    x: '<path d="m5 5 10 10M15 5 5 15"/>',
    mouse: '<rect x="6.2" y="2.8" width="7.600" height="14.4" rx="3.8"/><path d="M10 5.800v2.800"/>',
    pin: '<path d="M10 17.200s5-4.6 5-8.600a5 5 0 0 0-10 0c0 4 5 8.6 5 8.600Z"/><circle cx="10" cy="8.6" r="1.8"/>',
    umbrella: '<path d="M10 3.200a7 7 0 0 1 7 7H3a7 7 0 0 1 7-7ZM10 10.200v5a1.7 1.7 0 0 1-3.4 0"/>',
    power: '<path d="M10 3v6.500M6 5.300a6.2 6.2 0 1 0 8 0"/>',
    offline: '<path d="M5.5 15.500h8.300a3.3 3.3 0 0 0 1.2-6.400A4.8 4.8 0 0 0 6 8.200a3.7 3.7 0 0 0-.5 7.300Z"/><path d="m3.5 3.5 13 13"/>',
    cloud: '<path d="M5.5 15.500h8.300a3.3 3.3 0 0 0 1.2-6.400A4.8 4.8 0 0 0 6 8.200a3.7 3.7 0 0 0-.5 7.300Z"/>',
    info: '<circle cx="10" cy="10" r="7"/><path d="M10 9.200v4.3"/><circle cx="10" cy="6.7" r="0.6" fill="currentColor" stroke="none"/>',
    bell: '<path d="M5.2 13.500V9a4.8 4.8 0 0 1 9.6 0v4.500l1.2 1.500H4l1.2-1.500ZM8.4 17a1.7 1.7 0 0 0 3.2 0"/>',
    lock: '<rect x="4.2" y="9" width="11.6" height="8" rx="2"/><path d="M6.8 9V6.6a3.2 3.2 0 0 1 6.4 0V9"/>',
    snap: '<path d="M3.4 3v14M16.6 10H7.4M10.6 6.8 7.4 10l3.2 3.2"/>',
    layers: '<path d="m10 3.2 7 3.8-7 3.800L3 7l7-3.800ZM3.4 10.4 10 14l6.6-3.600M3.4 13.4 10 17l6.6-3.6"/>',
    rocket: '<path d="M10 2.800c2.6 1.8 3.6 4.6 3.6 7.200l-1.4 3H7.800l-1.4-3c0-2.6 1-5.4 3.6-7.200Z"/><circle cx="10" cy="8" r="1.3"/><path d="M8.6 15.2 10 17.400l1.4-2.2"/>',
    brush: '<path d="M15.6 3.2a1.7 1.7 0 0 1 1.2 2.9l-6.5 6.5-2.9.6.6-2.9 6.5-6.5a1.7 1.7 0 0 1 1.1-.6Z"/><path d="M6.9 12.400c-1.8.200-2.5 1.3-2.7 2.8-.1.8-.5 1.2-1.2 1.5 2.4.700 5 .100 5.3-2.6"/>',
    rows: '<rect x="3.5" y="3.5" width="13" height="5" rx="1.5"/><rect x="3.5" y="11.5" width="13" height="5" rx="1.5"/>',
    cols: '<rect x="3.5" y="3.5" width="5" height="13" rx="1.5"/><rect x="11.5" y="3.5" width="5" height="13" rx="1.5"/>',
    'al-left': '<path d="M4 5h12M4 10h7M4 15h10"/>',
    'al-center': '<path d="M4 5h12M6.5 10h7M5 15h10"/>',
    'al-right': '<path d="M4 5h12M9 10h7M6 15h10"/>',
    plus: '<path d="M10 4.5v11M4.5 10h11"/>',
    download: '<path d="M10 3.5v9M6.4 9.2 10 12.8l3.6-3.6M4 16h12"/>',
    restart: '<path d="M4 10a6 6 0 1 0 1.9-4.4"/><path d="M3.8 3.4v3.4h3.4"/>',
    trash: '<path d="M4.5 6h11M8 6V4.2h4V6M6 6l.7 9.6h6.6L14 6"/>',
    'win-min': '<path d="M5 10h10" stroke-width="1"/>',
    'win-max': '<rect x="5.5" y="5.5" width="9" height="9" rx="1.2" stroke-width="1"/>',
    'win-close': '<path d="m5.5 5.5 9 9M14.5 5.500l-9 9" stroke-width="1"/>'
  };

  function gl(name, cls) {
    return '<svg class="gl' + (cls ? ' ' + cls : '') + '" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + (G[name] || '') + '</svg>';
  }

  window.SkyIcons = { wx: wx, gl: gl, bounds: bounds, ensureDefs: ensureDefs, KEYS: ['clear', 'mostly-clear', 'partly-cloudy', 'cloudy', 'fog', 'drizzle', 'rain', 'heavy-rain', 'thunderstorm', 'snow', 'sleet', 'wind'] };
})();
