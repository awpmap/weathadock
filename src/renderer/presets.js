/* WeathaDock widget presets. One file, two consumers:
   - browser script: sets window.WD_PRESETS
   - CommonJS (main process): module.exports
   Shape: [{ id, name, description, widget: WidgetConfig }]. Every preset carries a COMPLETE
   WidgetConfig (all `show` keys), so applying one fully replaces the previous configuration. */
(function (root, factory) {
  var presets = factory();
  if (typeof module === 'object' && module && module.exports) module.exports = presets;
  if (root && typeof root === 'object') root.WD_PRESETS = presets;
})(typeof window !== 'undefined' ? window : (typeof globalThis !== 'undefined' ? globalThis : this), function () {
  'use strict';

  var KEYS = ['location', 'updated', 'refresh', 'icon', 'temp', 'condition', 'highLow', 'feelsLike',
    'incoming', 'timeline', 'hourly', 'daily', 'alerts', 'wind', 'humidity', 'uv', 'sun'];

  function show(on) {
    var s = {};
    KEYS.forEach(function (k) { s[k] = on.indexOf(k) >= 0; });
    return s;
  }
  function cfg(orientation, size, align, hours, days, on) {
    return { orientation: orientation, size: size, align: align, ink: 'auto', hours: hours, days: days, show: show(on) };
  }

  var NOW = ['icon', 'temp', 'condition', 'highLow', 'feelsLike'];
  var HEAD = ['location', 'updated', 'refresh'];

  return [
    /* ---- the four v1 layouts ---- */
    { id: 'full', name: 'Full', description: 'Now, what’s coming, hours and days.',
      widget: cfg('vertical', 'medium', 'left', 6, 5, HEAD.concat(NOW, ['incoming', 'timeline', 'hourly', 'daily', 'alerts'])) },
    { id: 'compact', name: 'Compact', description: 'Now, high/low and what’s coming.',
      widget: cfg('vertical', 'medium', 'left', 6, 5, HEAD.concat(NOW, ['incoming', 'alerts'])) },
    { id: 'wide', name: 'Wide', description: 'A horizontal strip with hours and days.',
      widget: cfg('horizontal', 'medium', 'left', 6, 5, HEAD.concat(NOW, ['incoming', 'timeline', 'hourly', 'daily', 'alerts'])) },
    { id: 'minimal', name: 'Minimal', description: 'Icon, temperature, high/low.',
      widget: cfg('horizontal', 'medium', 'left', 6, 5, ['icon', 'temp', 'highLow', 'refresh', 'alerts']) },

    /* ---- new in v1.1 ---- */
    { id: 'ticker', name: 'Ticker', description: 'One slim line: now and what’s next.',
      widget: cfg('horizontal', 'small', 'left', 6, 5, ['location', 'icon', 'temp', 'condition', 'highLow', 'incoming', 'alerts']) },
    { id: 'hero', name: 'Hero', description: 'A big centered temperature.',
      widget: cfg('vertical', 'large', 'center', 6, 5, ['location', 'icon', 'temp', 'condition', 'highLow', 'alerts']) },
    { id: 'number', name: 'Big number', description: 'Only the temperature, extra large.',
      widget: cfg('vertical', 'xlarge', 'left', 6, 5, ['temp']) },
    { id: 'today', name: 'Today', description: 'Now plus the next eight hours.',
      widget: cfg('horizontal', 'medium', 'left', 8, 5, ['icon', 'temp', 'condition', 'highLow', 'incoming', 'hourly', 'alerts']) },
    { id: 'forecast', name: 'Week ahead', description: 'Seven days, nothing else.',
      widget: cfg('vertical', 'medium', 'left', 6, 7, ['location', 'daily', 'alerts']) },
    { id: 'rain', name: 'Rain watch', description: 'Incoming rain or snow with the 6-hour timeline.',
      widget: cfg('vertical', 'medium', 'left', 6, 5, ['location', 'refresh', 'incoming', 'timeline', 'alerts']) },
    { id: 'dashboard', name: 'Dashboard', description: 'Everything, laid out wide.',
      widget: cfg('horizontal', 'medium', 'left', 6, 7, KEYS.slice()) },
    { id: 'conditions', name: 'Conditions', description: 'Now with wind, humidity, UV and sun times.',
      widget: cfg('vertical', 'medium', 'left', 6, 5, ['location', 'refresh'].concat(NOW, ['wind', 'humidity', 'uv', 'sun', 'alerts'])) }
  ];
});
