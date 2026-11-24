/* Design-time stand-in for the preload API. Loaded only when window.sky is undefined.
   ?scenario=clear-day|rain-soon|snow-night|severe-alert|loading|error
   Extras for design work: &stale=1 (mark data stale), &section=hourly (detail.html start section),
   &setup=1 (no locations yet), &places=1 (single location; default is two), &layout=<preset id> (see presets.js), &widget=<url-encoded JSON WidgetConfig, merged over the preset; layout becomes 'custom'>,
   &update=ready|downloading|unsupported|error|available (available: "Check for updates" finds and downloads one). */
(function () {
  'use strict';
  if (window.sky) return;

  var params = new URLSearchParams(location.search);
  var scenario = params.get('scenario') || 'clear-day';
  var TZ = 'America/Chicago';
  var LOC = { zip: '62701', city: 'Springfield', state: 'IL', lat: 39.8, lon: -89.65, timeZone: TZ };
  var ZIPS = {
    '62701': ['Springfield', 'IL', 39.8, -89.65, 'America/Chicago'], '10001': ['New York', 'NY', 40.75, -73.99, 'America/New_York'], '60601': ['Chicago', 'IL', 41.89, -87.62, 'America/Chicago'],
    '98101': ['Seattle', 'WA', 47.61, -122.33, 'America/Los_Angeles'], '80202': ['Denver', 'CO', 39.75, -104.99, 'America/Denver'], '33101': ['Miami', 'FL', 25.77, -80.19, 'America/New_York'],
    '78701': ['Austin', 'TX', 30.27, -97.74, 'America/Chicago'], '94103': ['San Francisco', 'CA', 37.77, -122.41, 'America/Los_Angeles'], '02108': ['Boston', 'MA', 42.36, -71.06, 'America/New_York'],
    '55401': ['Minneapolis', 'MN', 44.98, -93.27, 'America/Chicago'], '97201': ['Portland', 'OR', 45.5, -122.69, 'America/Los_Angeles'], '30303': ['Atlanta', 'GA', 33.75, -84.39, 'America/New_York']
  };
  function useZip(zip) {
    var z = ZIPS[zip] || ZIPS['62701'];
    TZ = z[4];
    LOC = { zip: ZIPS[zip] ? zip : '62701', city: z[0], state: z[1], lat: z[2], lon: z[3], timeZone: TZ };
  }

  /* ---------- time helpers (ISO strings with the real Chicago offset) ---------- */
  function tzParts(ms) {
    var p = {};
    new Intl.DateTimeFormat('en-US', { timeZone: TZ, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' })
      .formatToParts(new Date(ms)).forEach(function (x) { p[x.type] = x.value; });
    return p;
  }
  function offsetMin(ms) {
    var p = tzParts(ms);
    return Math.round((Date.UTC(+p.year, +p.month - 1, +p.day, +p.hour, +p.minute, +p.second) - Math.floor(ms / 1000) * 1000) / 60000);
  }
  function pad(n) { return (n < 10 ? '0' : '') + n; }
  function iso(ms) {
    var off = offsetMin(ms), p = tzParts(ms), a = Math.abs(off);
    return p.year + '-' + p.month + '-' + p.day + 'T' + p.hour + ':' + p.minute + ':' + p.second + (off < 0 ? '-' : '+') + pad(Math.floor(a / 60)) + ':' + pad(a % 60);
  }
  /* today (Chicago calendar day) at h:m local */
  function todayAt(h, m) {
    var p = tzParts(Date.now());
    var guess = Date.UTC(+p.year, +p.month - 1, +p.day, h, m || 0);
    return guess - offsetMin(guess) * 60000;
  }
  var HOUR = 3600000, MIN = 60000;

  var LABEL = {
    clear: ['Sunny', 'Clear'], 'mostly-clear': ['Mostly Sunny', 'Mostly Clear'], 'partly-cloudy': ['Partly Sunny', 'Partly Cloudy'],
    cloudy: ['Cloudy', 'Cloudy'], fog: ['Fog', 'Fog'], drizzle: ['Drizzle', 'Drizzle'], rain: ['Rain', 'Rain'],
    'heavy-rain': ['Heavy Rain', 'Heavy Rain'], thunderstorm: ['Thunderstorms', 'Thunderstorms'], snow: ['Snow', 'Snow'],
    sleet: ['Wintry Mix', 'Wintry Mix'], wind: ['Windy', 'Windy']
  };
  function label(icon, day) { return (LABEL[icon] || ['–', '–'])[day ? 0 : 1]; }

  function dayText(icon, hi, wind) {
    var w = ' ' + wind + '.';
    switch (icon) {
      case 'clear': return 'Sunny, with a high near ' + hi + '.' + w;
      case 'mostly-clear': return 'Mostly sunny, with a high near ' + hi + '.' + w;
      case 'partly-cloudy': return 'Partly sunny, with a high near ' + hi + '.' + w;
      case 'cloudy': return 'Cloudy, with a high near ' + hi + '.' + w;
      case 'fog': return 'Patchy fog before 9am. Otherwise, mostly cloudy, with a high near ' + hi + '.' + w;
      case 'drizzle': return 'Patchy drizzle. Cloudy, with a high near ' + hi + '.' + w + ' Chance of precipitation is 40%.';
      case 'rain': return 'Rain, mainly after 10am. High near ' + hi + '.' + w + ' Chance of precipitation is 80%. New rainfall amounts between a quarter and half of an inch possible.';
      case 'heavy-rain': return 'Rain. The rain could be heavy at times. High near ' + hi + '.' + w + ' Chance of precipitation is 100%. New rainfall amounts between 1 and 2 inches possible.';
      case 'thunderstorm': return 'Showers and thunderstorms. Some of the storms could be severe, with damaging winds and heavy rain. High near ' + hi + '.' + w + ' Chance of precipitation is 90%.';
      case 'snow': return 'Snow. High near ' + hi + '.' + w + ' Chance of precipitation is 90%. New snow accumulation of 2 to 4 inches possible.';
      case 'sleet': return 'Rain and snow, possibly mixed with sleet. High near ' + hi + '.' + w + ' Chance of precipitation is 70%. Little or no ice accumulation expected.';
      default: return 'Mostly sunny and breezy, with a high near ' + hi + '.' + w;
    }
  }
  function nightText(icon, lo, wind) {
    var w = ' ' + wind + '.';
    switch (icon) {
      case 'clear': return 'Clear, with a low around ' + lo + '.' + w;
      case 'mostly-clear': return 'Mostly clear, with a low around ' + lo + '.' + w;
      case 'partly-cloudy': return 'Partly cloudy, with a low around ' + lo + '.' + w;
      case 'cloudy': return 'Mostly cloudy, with a low around ' + lo + '.' + w;
      case 'fog': return 'Areas of fog after midnight. Otherwise, mostly cloudy, with a low around ' + lo + '.' + w;
      case 'drizzle': return 'A chance of drizzle before midnight. Mostly cloudy, with a low around ' + lo + '.' + w;
      case 'rain': return 'Rain likely, mainly before 1am. Cloudy, with a low around ' + lo + '.' + w + ' Chance of precipitation is 60%.';
      case 'heavy-rain': return 'Rain, tapering to showers after midnight. Low around ' + lo + '.' + w + ' Chance of precipitation is 90%.';
      case 'thunderstorm': return 'Showers and thunderstorms likely before 10pm, then a chance of showers. Low around ' + lo + '.' + w + ' Chance of precipitation is 70%.';
      case 'snow': return 'Snow, mainly before 3am. Low around ' + lo + '.' + w + ' Chance of precipitation is 80%. New snow accumulation of 1 to 3 inches possible.';
      case 'sleet': return 'A chance of rain and snow before midnight, then a chance of flurries. Low around ' + lo + '.' + w;
      default: return 'Mostly clear and breezy, with a low around ' + lo + '.' + w;
    }
  }

  /* ---------- scenarios ---------- */
  var SC = {
    'clear-day': {
      at: [13, 40],
      cur: { temp: 63, feelsLike: 63, icon: 'clear', humidity: 38, dewPoint: 37, windSpeed: 7, windGust: null, windDir: 'WSW', pressure: 30.21, visibility: 10, uvIndex: 3 },
      highs: [64, 66, 61, 53, 49, 55, 59], lows: [41, 44, 46, 38, 33, 35, 40],
      icons: ['clear', 'mostly-clear', 'partly-cloudy', 'rain', 'cloudy', 'clear', 'mostly-clear'], pops: [0, 5, 20, 70, 30, 5, 0],
      winds: ['West southwest wind 5 to 10 mph', 'Southwest wind around 10 mph', 'South wind 10 to 15 mph, with gusts as high as 25 mph', 'Northwest wind 10 to 15 mph', 'North wind around 10 mph', 'Light and variable wind', 'South wind 5 to 10 mph'],
      summary: 'Sunny, with a high near 64. West southwest wind 5 to 10 mph.',
      precip: { kind: null, isNow: false, start: null, end: null, headline: 'Dry for the next 12 hours' },
      minutely: function () { return 0; },
      hourly: function () { return null; },
      alerts: []
    },
    'rain-soon': {
      at: [15, 20],
      cur: { temp: 54, feelsLike: 51, icon: 'cloudy', humidity: 81, dewPoint: 48, windSpeed: 12, windGust: 21, windDir: 'SE', pressure: 29.74, visibility: 8, uvIndex: 1 },
      highs: [57, 52, 55, 60, 58, 47, 45], lows: [44, 38, 36, 41, 43, 34, 31],
      icons: ['rain', 'partly-cloudy', 'clear', 'mostly-clear', 'rain', 'cloudy', 'partly-cloudy'], pops: [90, 20, 0, 10, 60, 30, 10],
      winds: ['Southeast wind 10 to 15 mph, with gusts as high as 25 mph', 'West wind 10 to 15 mph', 'Light west wind', 'South wind 5 to 10 mph', 'South wind 10 to 15 mph', 'Northwest wind 15 to 20 mph', 'North wind around 10 mph'],
      summary: 'Rain, mainly after 4pm. Steady temperature around 54. Southeast wind 10 to 15 mph, with gusts as high as 25 mph. Chance of precipitation is 90%. New rainfall amounts between a half and three quarters of an inch possible.',
      precip: { kind: 'rain', isNow: false, start: 25, end: 330, headline: 'Rain starting in 25 min' },
      minutely: function (i) {
        var c = [0, 0.004, 0.012, 0.022, 0.034, 0.048, 0.062, 0.078, 0.09, 0.082, 0.07, 0.066, 0.05, 0.04, 0.036, 0.03, 0.022, 0.018, 0.014, 0.01, 0.006, 0.003, 0, 0];
        return c[i] || 0;
      },
      hourly: function (i) {
        var seq = [['cloudy', 55, 0], ['rain', 95, 0.07], ['heavy-rain', 100, 0.28], ['rain', 95, 0.21], ['rain', 85, 0.12], ['drizzle', 60, 0.03], ['cloudy', 30, 0], ['cloudy', 20, 0], ['partly-cloudy', 10, 0]];
        return seq[i] || null;
      },
      alerts: []
    },
    'snow-night': {
      at: [21, 10],
      cur: { temp: 27, feelsLike: 17, icon: 'snow', humidity: 88, dewPoint: 24, windSpeed: 14, windGust: 24, windDir: 'NNE', pressure: 29.92, visibility: 1.5, uvIndex: 0 },
      highs: [31, 29, 26, 33, 38, 41, 36], lows: [22, 14, 11, 19, 27, 30, 24],
      icons: ['snow', 'cloudy', 'clear', 'mostly-clear', 'partly-cloudy', 'sleet', 'cloudy'], pops: [90, 20, 0, 0, 10, 50, 20],
      winds: ['North northeast wind 10 to 15 mph, with gusts as high as 25 mph', 'North wind 10 to 15 mph', 'Northwest wind 5 to 10 mph', 'Light and variable wind', 'South wind 5 to 10 mph', 'East wind around 10 mph', 'Northwest wind 10 to 15 mph'],
      summary: 'Snow, mainly before 3am. Low around 22. Wind chill values as low as 12. North northeast wind 10 to 15 mph, with gusts as high as 25 mph. Chance of precipitation is 90%. New snow accumulation of 2 to 4 inches possible.',
      precip: { kind: 'snow', isNow: true, start: -95, end: 350, headline: 'Snow until 3 AM' },
      minutely: function (i) {
        var c = [0.014, 0.016, 0.02, 0.022, 0.026, 0.03, 0.028, 0.03, 0.026, 0.024, 0.02, 0.022, 0.018, 0.016, 0.014, 0.014, 0.012, 0.01, 0.008, 0.008, 0.006, 0.004, 0.002, 0];
        return c[i] || 0;
      },
      hourly: function (i) {
        var seq = [['snow', 90, 0.07], ['snow', 95, 0.1], ['snow', 95, 0.11], ['snow', 90, 0.08], ['snow', 80, 0.06], ['snow', 65, 0.03], ['cloudy', 30, 0.01], ['cloudy', 20, 0]];
        return seq[i] || null;
      },
      alerts: [{
        id: 'urn:oid:2.49.0.1.840.0.mock.wwa', event: 'Winter Weather Advisory', severity: 'Moderate', urgency: 'Expected', certainty: 'Likely',
        headline: 'Winter Weather Advisory issued by NWS Lincoln IL, in effect until 6 AM CST',
        description: '* WHAT...Snow. Additional snow accumulations of 2 to 4 inches.\n\n* WHERE...Portions of central and west central Illinois, including Sangamon, Menard and Christian Counties.\n\n* WHEN...Until 6 AM CST.\n\n* IMPACTS...Roads, and especially bridges and overpasses, will likely become slick and hazardous. The hazardous conditions could impact the morning commute.',
        instruction: 'Slow down and use caution while traveling.\n\nThe latest road conditions for Illinois can be obtained by calling 1-800-452-4368.',
        on: -240, end: 530, exp: 530, sender: 'NWS Lincoln IL', areaDesc: 'Sangamon; Menard; Christian; Logan'
      }]
    },
    'severe-alert': {
      at: [16, 5],
      cur: { temp: 67, feelsLike: 67, icon: 'thunderstorm', humidity: 78, dewPoint: 60, windSpeed: 22, windGust: 48, windDir: 'S', pressure: 29.41, visibility: 6, uvIndex: 1 },
      highs: [70, 51, 46, 49, 54, 57, 52], lows: [43, 34, 31, 33, 39, 42, 37],
      icons: ['thunderstorm', 'wind', 'mostly-clear', 'clear', 'partly-cloudy', 'rain', 'cloudy'], pops: [100, 10, 0, 0, 10, 60, 30],
      winds: ['South wind 20 to 25 mph, with gusts as high as 50 mph', 'West northwest wind 20 to 25 mph, with gusts as high as 40 mph', 'West wind 10 to 15 mph', 'Light southwest wind', 'South wind 10 mph', 'South wind 10 to 15 mph', 'Northwest wind 10 to 15 mph'],
      summary: 'Showers and thunderstorms. Some of the storms could be severe, with tornadoes, damaging winds and large hail. High near 70. South wind 20 to 25 mph, with gusts as high as 50 mph. Chance of precipitation is 100%.',
      precip: { kind: 'storm', isNow: false, start: 10, end: 215, headline: 'Thunderstorms starting in 10 min' },
      minutely: function (i) {
        var c = [0.002, 0.03, 0.09, 0.16, 0.19, 0.13, 0.1, 0.12, 0.08, 0.06, 0.05, 0.03, 0.02, 0.012, 0.006, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        return c[i] || 0;
      },
      hourly: function (i) {
        var seq = [['thunderstorm', 100, 0.3], ['thunderstorm', 100, 0.62], ['heavy-rain', 95, 0.41], ['rain', 70, 0.12], ['drizzle', 40, 0.02], ['cloudy', 20, 0], ['wind', 10, 0], ['wind', 5, 0], ['wind', 5, 0]];
        return seq[i] || null;
      },
      alerts: [{
        id: 'urn:oid:2.49.0.1.840.0.mock.tor', event: 'Tornado Warning', severity: 'Extreme', urgency: 'Immediate', certainty: 'Observed',
        headline: 'Tornado Warning issued by NWS Lincoln IL, in effect until 4:45 PM CST',
        description: 'The National Weather Service in Lincoln has issued a\n\n* Tornado Warning for...\n  Central Sangamon County in central Illinois...\n\n* Until 445 PM CST.\n\n* At 402 PM CST, a severe thunderstorm capable of producing a tornado was located near New Berlin, or 12 miles west of Springfield, moving northeast at 45 mph.\n\n  HAZARD...Tornado and quarter size hail.\n\n  SOURCE...Radar indicated rotation.\n\n  IMPACT...Flying debris will be dangerous to those caught without shelter. Mobile homes will be damaged or destroyed. Damage to roofs, windows, and vehicles will occur. Tree damage is likely.\n\n* This dangerous storm will be near...\n  Springfield and Jerome around 415 PM CST.\n  Sherman around 425 PM CST.',
        instruction: 'TAKE COVER NOW! Move to a basement or an interior room on the lowest floor of a sturdy building. Avoid windows. If you are outdoors, in a mobile home, or in a vehicle, move to the closest substantial shelter and protect yourself from flying debris.',
        on: -3, end: 40, exp: 40, sender: 'NWS Lincoln IL', areaDesc: 'Sangamon, IL'
      }, {
        id: 'urn:oid:2.49.0.1.840.0.mock.svr', event: 'Severe Thunderstorm Watch', severity: 'Severe', urgency: 'Future', certainty: 'Possible',
        headline: 'Severe Thunderstorm Watch issued by NWS Storm Prediction Center, in effect until 9 PM CST',
        description: 'THE NATIONAL WEATHER SERVICE HAS ISSUED SEVERE THUNDERSTORM WATCH 712 IN EFFECT UNTIL 9 PM CST THIS EVENING FOR THE FOLLOWING AREAS\n\nIN ILLINOIS THIS WATCH INCLUDES 14 COUNTIES\n\nIN CENTRAL ILLINOIS\n\nCHRISTIAN, LOGAN, MACON, MENARD, MORGAN, SANGAMON\n\nPRIMARY THREATS INCLUDE...\n  Scattered damaging wind gusts to 70 mph possible\n  Isolated large hail events to 1.5 inches in diameter possible\n  A tornado or two possible',
        instruction: 'Be prepared to move to shelter if a warning is issued for your area. Stay tuned to local media or NOAA Weather Radio for updates.',
        on: -125, end: 295, exp: 295, sender: 'NWS Storm Prediction Center', areaDesc: 'Christian; Logan; Macon; Menard; Morgan; Sangamon'
      }, {
        id: 'urn:oid:2.49.0.1.840.0.mock.fld', event: 'Flood Advisory', severity: 'Minor', urgency: 'Expected', certainty: 'Likely',
        headline: 'Flood Advisory issued by NWS Lincoln IL, in effect until 7:15 PM CST',
        description: '* WHAT...Urban and small stream flooding caused by excessive rainfall is expected.\n\n* WHERE...A portion of central Illinois, including Sangamon County.\n\n* WHEN...Until 715 PM CST.\n\n* IMPACTS...Minor flooding in low-lying and poor drainage areas. Water over roadways.',
        instruction: 'Turn around, don\'t drown when encountering flooded roads. Most flood deaths occur in vehicles.',
        on: 0, end: 190, exp: 190, sender: 'NWS Lincoln IL', areaDesc: 'Sangamon, IL'
      }]
    }
  };

  function emptyState(status, error) {
    return {
      status: status, error: error || null, stale: false, updatedAt: null, sources: { nws: false, openMeteo: false },
      location: status === 'error' ? LOC : null, current: null, today: null,
      nextPrecip: { kind: null, isNow: false, startsAt: null, endsAt: null, minutesUntil: null, headline: '' },
      minutely: [], hourly: [], daily: [], alerts: []
    };
  }

  function cosStep(a, b, k) { return a + (b - a) * (0.5 - 0.5 * Math.cos(Math.PI * Math.max(0, Math.min(1, k)))); }

  function build(name) {
    var sc = SC[name];
    if (!sc) return name === 'error'
      ? emptyState('error', 'Couldn’t reach the National Weather Service or Open-Meteo. Check your internet connection.')
      : emptyState('loading');

    var now = todayAt(sc.at[0], sc.at[1]);
    var midnight = todayAt(0, 0);
    var SUNRISE = 6 * 60 + 47, SUNSET = 16 * 60 + 40;
    function isDayAt(ms) { var m = ((ms - midnight) / MIN) % 1440; if (m < 0) m += 1440; return m >= SUNRISE && m < SUNSET; }

    /* temperature curve from daily highs/lows: low at 6am, high at 3pm */
    function tempAt(ms) {
      var hrs = (ms - midnight) / HOUR, dIdx = Math.floor(hrs / 24), h = hrs - dIdx * 24;
      var hi = function (k) { return sc.highs[Math.max(0, Math.min(6, k))]; };
      var lo = function (k) { return sc.lows[Math.max(0, Math.min(6, k))]; };
      /* lows[d] is the night following day d; the 6am minimum of day d is lows[d-1] */
      var morningLow = dIdx === 0 ? lo(0) - 2 : lo(dIdx - 1);
      if (h < 6) return cosStep(dIdx === 0 ? hi(0) - 3 : hi(dIdx - 1), morningLow, (h + 9) / 15);
      if (h < 15) return cosStep(morningLow, hi(dIdx), (h - 6) / 9);
      return cosStep(hi(dIdx), lo(dIdx), (h - 15) / 15);
    }

    var hour0 = Math.floor(now / HOUR) * HOUR;
    var hourly = [];
    var dirs = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW'];
    var baseOffset = sc.cur.temp - tempAt(hour0);
    for (var i = 0; i < 48; i++) {
      var t = hour0 + i * HOUR;
      var dIdx = Math.min(6, Math.floor((t - midnight) / (24 * HOUR)));
      var day = isDayAt(t + 30 * MIN);
      var ov = sc.hourly(i);
      var icon = ov ? ov[0] : sc.icons[dIdx];
      var wob = Math.sin(i * 1.7) * 0.5 + Math.sin(i * 0.6) * 0.5;
      var pop = ov ? ov[1] : Math.max(0, Math.min(100, Math.round((sc.pops[dIdx] * (0.75 + 0.25 * wob)) / 5) * 5));
      if (!ov && pop < 35 && /rain|snow|sleet|drizzle|thunder/.test(icon)) icon = 'cloudy';
      var amt = ov ? ov[2] : (pop >= 50 ? Math.round(pop * 0.12) / 100 : 0);
      var temp = Math.round(tempAt(t) + baseOffset * Math.exp(-i / 5));
      var wind = Math.max(2, Math.round(sc.cur.windSpeed * (0.55 + 0.45 * Math.exp(-i / 10)) + wob * 2));
      var chill = wind > 8 && temp < 50 ? Math.round(wind / 3) : (sc.cur.temp - sc.cur.feelsLike > 0 && i < 4 ? sc.cur.temp - sc.cur.feelsLike : 0);
      hourly.push({
        time: iso(t), temp: temp, feelsLike: temp - chill, icon: icon, isDay: day, condition: label(icon, day),
        precipProb: pop, precipAmount: amt, windSpeed: wind, windDir: dirs[(dirs.indexOf(sc.cur.windDir) + Math.floor(i / 5)) % dirs.length],
        humidity: Math.max(20, Math.min(98, Math.round(sc.cur.humidity + (day ? -4 : 6) + wob * 4)))
      });
    }
    hourly[0].temp = sc.cur.temp; hourly[0].feelsLike = sc.cur.feelsLike; hourly[0].icon = sc.cur.icon;
    hourly[0].condition = label(sc.cur.icon, isDayAt(now));

    var names = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    var daily = [];
    for (var d = 0; d < 7; d++) {
      var noon = midnight + d * 24 * HOUR + 12 * HOUR;
      var p = tzParts(noon);
      var wd = new Date(Date.UTC(+p.year, +p.month - 1, +p.day)).getUTCDay();
      daily.push({
        date: p.year + '-' + p.month + '-' + p.day, name: d === 0 ? 'Today' : names[wd],
        high: sc.highs[d], low: sc.lows[d], icon: sc.icons[d], condition: label(sc.icons[d], true), precipProb: sc.pops[d],
        dayText: d === 0 && !isDayAt(now) ? '' : (d === 0 ? sc.summary : dayText(sc.icons[d], sc.highs[d], sc.winds[d])),
        nightText: d === 0 && !isDayAt(now) ? sc.summary : nightText(sc.icons[Math.min(6, d + (sc.pops[d] > 50 ? 0 : 1))] === 'wind' ? 'clear' : sc.icons[d], sc.lows[d], sc.winds[d].replace(/, with gusts.*$/, '')),
        sunrise: iso(midnight + d * 24 * HOUR + (SUNRISE + d) * MIN), sunset: iso(midnight + d * 24 * HOUR + (SUNSET - d) * MIN)
      });
    }

    var q0 = Math.floor(now / (15 * MIN)) * 15 * MIN;
    var mk = sc.precip.kind === 'storm' ? 'rain' : sc.precip.kind;
    var minutely = [];
    for (var m = 0; m < 24; m++) {
      var amount = sc.minutely(m);
      minutely.push({ time: iso(q0 + m * 15 * MIN), precip: amount, kind: amount > 0 ? mk : null });
    }

    var day0 = isDayAt(now);
    /* a station's report is behind "now" unless the preview asks for the modelled conditions */
    var measured = params.get('station') !== '0';
    var STATION = { id: 'KSPI', name: params.get('long') === '1' ? 'Springfield, Abraham Lincoln Capital Airport and Air National Guard Base, Sangamon County' : 'Springfield, Abraham Lincoln Capital Airport', miles: 3.6 };
    return {
      status: 'ok', error: null, stale: false, updatedAt: iso(now - 2 * MIN), sources: { nws: true, openMeteo: true, alerts: true },
      location: LOC,
      current: Object.assign({ condition: label(sc.cur.icon, day0), isDay: day0, observedAt: measured ? iso(now - 11 * MIN) : null, station: measured ? STATION : null }, sc.cur),
      today: { high: sc.highs[0], low: sc.lows[0], sunrise: daily[0].sunrise, sunset: daily[0].sunset, summary: sc.summary },
      nextPrecip: {
        kind: sc.precip.kind, isNow: sc.precip.isNow,
        startsAt: sc.precip.start == null ? null : iso(now + sc.precip.start * MIN),
        endsAt: sc.precip.end == null ? null : iso(now + sc.precip.end * MIN),
        minutesUntil: sc.precip.start == null ? null : Math.max(0, sc.precip.start), headline: sc.precip.headline
      },
      minutely: minutely, hourly: hourly, daily: daily,
      alerts: sc.alerts.map(function (a) {
        return {
          id: a.id, event: a.event, severity: a.severity, urgency: a.urgency, certainty: a.certainty, headline: a.headline,
          description: a.description, instruction: a.instruction, onset: iso(now + a.on * MIN), ends: iso(now + a.end * MIN),
          expires: iso(now + a.exp * MIN), sender: a.sender, areaDesc: a.areaDesc
        };
      })
    };
  }

  var places = params.get('setup') === '1' ? [] : [{ zip: '62701', city: params.get('long') === '1' ? 'Rancho Santa Margarita Heights' : 'Springfield', state: 'IL' }];
  if (places.length && params.get('places') !== '1') places.push({ zip: '80202', city: 'Denver', state: 'CO' });
  var activeZip = places.length ? places[0].zip : null;
  /* the second, third... place shows a different scenario so switching is visible */
  var ALT = { 'clear-day': 'rain-soon', 'rain-soon': 'clear-day', 'snow-night': 'clear-day', 'severe-alert': 'rain-soon' };
  function scenarioFor(zip) { return places.length && zip !== places[0].zip && ALT[scenario] ? ALT[scenario] : scenario; }
  function withStale(st) {
    if (params.get('stale') === '1' && st.status === 'ok') {
      st.stale = true;
      st.error = 'Couldn\u2019t reach the weather service. Showing the last forecast we have.';
      st.updatedAt = iso(new Date(st.updatedAt).getTime() - 47 * MIN);
      if (st.current && st.current.observedAt) st.current.observedAt = iso(new Date(st.updatedAt).getTime() - 9 * MIN);
      st.sources = { nws: false, openMeteo: false, alerts: false };
      st.alertsAt = st.updatedAt;
    }
    return st;
  }
  function buildFor(zip) {
    useZip(zip || '62701');
    var st = withStale(build(scenarioFor(zip)));
    if (params.get('long') === '1' && st.location) {   /* design aid: stress truncation */
      st.location.city = 'Rancho Santa Margarita Heights';
      if (st.alerts.length) st.alerts[0].event = 'Particularly Dangerous Situation Severe Thunderstorm Warning';
    }
    return st;
  }

  var PRESETS = window.WD_PRESETS || [];
  function presetById(id) { for (var i = 0; i < PRESETS.length; i++) if (PRESETS[i].id === id) return PRESETS[i]; return null; }
  /* same merge rule as main: shallow over the config, `show` merged key by key */
  function mergeWidget(base, patch) {
    var out = Object.assign({}, base, patch || {});
    out.show = Object.assign({}, (base && base.show) || {}, (patch && patch.show) || {});
    return out;
  }
  var gp = params.get('glass');
  var startPreset = presetById(params.get('layout')) || presetById('full');
  var settings = {
    zip: activeZip,
    glass: gp === 'clear' || gp === 'frosted' ? gp : 'none',
    layout: startPreset ? startPreset.id : 'full',
    widget: startPreset ? clone(startPreset.widget) : null,
    taskbar: { condition: true, highLow: true, hint: true, hours: Number(params.get('hours') || 4) }, coveredMode: 'tray', placement: params.get('placement') === 'taskbar' ? 'taskbar' : 'desktop', showWidget: true, launchAtLogin: true, alertNotifications: true, refreshMinutes: 10,
    lockPosition: params.get('lock') === '1', snapToEdges: true, widgetOnTop: false, widgetOpacity: 100
  };
  if (params.get('widget')) {
    try { settings.widget = mergeWidget(settings.widget || {}, JSON.parse(params.get('widget'))); settings.layout = 'custom'; }
    catch (e) { console.warn('[mock sky] bad ?widget= JSON'); }
  }

  var state = buildFor(activeZip);
  var stateCbs = [], settingsCbs = [], navCbs = [], locCbs = [];
  function later(v, ms) { return new Promise(function (r) { setTimeout(function () { r(v); }, ms == null ? 60 : ms); }); }
  function clone(o) { return JSON.parse(JSON.stringify(o)); }
  function pushState() { var s = clone(state); stateCbs.forEach(function (cb) { cb(s); }); }
  function locs() { return { locations: clone(places), activeZip: activeZip }; }
  function pushLocs() { settings.zip = activeZip; var v = locs(); locCbs.forEach(function (cb) { cb(v); }); var st = clone(settings); settingsCbs.forEach(function (cb) { cb(st); }); }
  var up = params.get('update');
  var update = {
    version: '1.0.0',
    status: ['ready', 'downloading', 'unsupported', 'error'].indexOf(up) >= 0 ? up : 'idle',
    newVersion: up === 'ready' || up === 'downloading' ? '1.1.0' : null,
    percent: up === 'downloading' ? 42 : null,
    error: up === 'error' ? 'Couldn\u2019t reach the update server. Check your connection and try again.' : null,
    checkedAt: up === 'error' || up === 'ready' ? new Date(Date.now() - 5 * 60000).toISOString() : null
  };
  var updCbs = [];
  function pushUpdate(patch) { Object.assign(update, patch); var u = clone(update); updCbs.forEach(function (cb) { cb(u); }); return u; }
  var switchSeq = 0;
  function activate(zip) {
    activeZip = zip; pushLocs();
    var target = buildFor(zip), seq = ++switchSeq;
    if (target.status !== 'ok') { state = target; pushState(); return; }
    state = Object.assign(emptyState('loading'), { location: target.location }); pushState();
    setTimeout(function () { if (seq === switchSeq) { state = target; pushState(); } }, 650);
  }
  function log() { if (params.get('log') === '1') console.log.apply(console, ['[mock sky]'].concat([].slice.call(arguments))); }

  window.sky = {
    __mock: true,
    getState: function () { return later(clone(state), 30); },
    onState: function (cb) { stateCbs.push(cb); },
    getSettings: function () { return later(clone(settings), 20); },
    onSettings: function (cb) { settingsCbs.push(cb); },
    setSettings: function (patch) {
      var p = Object.assign({}, patch || {});
      delete p.zip; /* read-only: mirrors activeZip */
      var w = p.widget; delete p.widget;
      Object.assign(settings, p);
      if (w) settings.widget = mergeWidget(settings.widget || {}, w);
      else if (p.layout && presetById(p.layout)) settings.widget = clone(presetById(p.layout).widget);
      var s = clone(settings);
      settingsCbs.forEach(function (cb) { cb(s); });
      return later(s, 40);
    },
    getUpdateState: function () { return later(clone(update), 20); },
    onUpdateState: function (cb) { updCbs.push(cb); },
    checkForUpdates: function () {
      log('checkForUpdates');
      if (update.status === 'unsupported') return later(clone(update), 20);
      pushUpdate({ status: 'checking', error: null });
      return later(null, 900).then(function () {
        var now = new Date().toISOString();
        if (up !== 'available') return pushUpdate({ status: 'up-to-date', checkedAt: now, newVersion: null, percent: null });
        pushUpdate({ status: 'downloading', newVersion: '1.1.0', percent: 0, checkedAt: now });
        return new Promise(function (resolve) {
          var pct = 0;
          var t = setInterval(function () {
            pct += 20;
            if (pct >= 100) { clearInterval(t); resolve(pushUpdate({ status: 'ready', percent: 100 })); }
            else pushUpdate({ percent: pct });
          }, 250);
        });
      });
    },
    installUpdate: function () { log('installUpdate'); window.__installUpdate = true; },
    getLocations: function () { return later(locs(), 20); },
    onLocations: function (cb) { locCbs.push(cb); },
    addLocation: function (zip) {
      log('addLocation', zip);
      if (!/^\d{5}$/.test(zip || '')) return later({ ok: false, error: 'Enter a 5-digit ZIP code.' }, 150);
      var z = ZIPS[zip];
      if (!z) return later({ ok: false, error: 'We couldn\u2019t find that ZIP code.' }, 450);
      var place = { zip: zip, city: z[0], state: z[1] };
      return later(null, 450).then(function () {
        if (!places.some(function (p) { return p.zip === zip; })) places.push(place);
        activate(zip);
        return { ok: true, place: place };
      });
    },
    removeLocation: function (zip) {
      log('removeLocation', zip);
      if (places.length <= 1) return later(undefined, 30);
      places = places.filter(function (p) { return p.zip !== zip; });
      if (activeZip === zip) activate(places[0].zip); else pushLocs();
      return later(undefined, 30);
    },
    setActiveLocation: function (zip) {
      log('setActiveLocation', zip);
      if (places.some(function (p) { return p.zip === zip; }) && zip !== activeZip) activate(zip);
      return later(undefined, 30);
    },
    lookupZip: function (zip) {
      var z = ZIPS[zip];
      if (!/^\d{5}$/.test(zip || '')) return later({ ok: false, error: 'Enter a 5-digit ZIP code.' }, 150);
      return later(z ? { ok: true, zip: zip, city: z[0], state: z[1], lat: z[2], lon: z[3] } : { ok: false, error: 'We couldn’t find that ZIP code.' }, 420);
    },
    completeSetup: function (v) {
      log('completeSetup', v);
      if (v && v.zip === '33101') return later({ ok: false, error: 'Couldn’t save your settings. Please try again.' }, 500);
      return later({ ok: true }, 500);
    },
    refresh: function () {
      log('refresh');
      var prev = state;
      if (prev.status === 'ok') { state = Object.assign({}, prev, { status: 'loading' }); pushState(); }
      else { state = emptyState('loading'); pushState(); }
      return later(null, 1100).then(function () {
        state = prev.status === 'ok' ? Object.assign({}, prev, { status: 'ok' }) : buildFor(activeZip);
        pushState();
      });
    },
    openDetail: function (section) { log('openDetail', section || 'overview'); window.__lastOpenDetail = section || 'overview'; },
    onNavigate: function (cb) {
      navCbs.push(cb);
      var sec = params.get('section');
      if (sec) setTimeout(function () { cb(sec); }, 0);
    },
    windowControl: function (action) { log('windowControl', action); },
    moveBy: function (dx, dy) { log('moveBy', dx, dy); },
    dragEnd: function () { log('dragEnd'); },
    getBattery: function () { var b = params.get('battery'); var old = params.get('age'); var on = !old && params.get('charging') === '1'; var t = { good: 'Good', mid: 'Mid', low: 'Low', urgent: 'Charge now' }; return Promise.resolve(t[b] ? { tier: b, label: t[b], charging: on, low: b === 'low' && !on, urgent: b === 'urgent' && !on, stale: !!old, age: old || null } : null); },
    getBatteryLog: function () { return Promise.resolve('WeathaDock mouse battery details\n12:00:00 c547/01: mouse, feature 1004, bytes [2d, 04, 00, 00], 45% -> steadied 45%, shown Mid\n'); },
    onBattery: function () {},
    getDock: function () { return Promise.resolve({ room: Number(params.get('room') || 0), light: params.get('bar') === 'light' }); },
    onDock: function () {},
    getBackdrop: function () { return Promise.resolve({ dark: params.get('backdrop') === 'bright' }); },
    onBackdrop: function () {},
    showContextMenu: function () { window.__contextMenus = (window.__contextMenus || 0) + 1; log('showContextMenu'); },
    setContentSize: function (w, h) { window.__contentSize = { width: w, height: h }; log('setContentSize', w, h); },
    setTrayIcon: function (dataUrl) { window.__trayIcon = dataUrl; },
    quit: function () { log('quit'); }
  };
})();
