'use strict';
// Data layer: zip lookup, National Weather Service (forecast + official alerts) and
// Open-Meteo (current conditions + 15-minute precipitation timing), merged into the
// WeatherState shape described in docs/UI-CONTRACT.md. No Electron imports, so it can be
// unit tested with plain Node.

const USER_AGENT = 'WeathaDockDesktop/1.0 (desktop weather widget)';
const WET_THRESHOLD = 0.004; // inches per 15 minutes (about 0.1 mm)

const STATES = {
  Alabama: 'AL', Alaska: 'AK', Arizona: 'AZ', Arkansas: 'AR', California: 'CA', Colorado: 'CO',
  Connecticut: 'CT', Delaware: 'DE', 'District of Columbia': 'DC', Florida: 'FL', Georgia: 'GA',
  Hawaii: 'HI', Idaho: 'ID', Illinois: 'IL', Indiana: 'IN', Iowa: 'IA', Kansas: 'KS', Kentucky: 'KY',
  Louisiana: 'LA', Maine: 'ME', Maryland: 'MD', Massachusetts: 'MA', Michigan: 'MI', Minnesota: 'MN',
  Mississippi: 'MS', Missouri: 'MO', Montana: 'MT', Nebraska: 'NE', Nevada: 'NV', 'New Hampshire': 'NH',
  'New Jersey': 'NJ', 'New Mexico': 'NM', 'New York': 'NY', 'North Carolina': 'NC', 'North Dakota': 'ND',
  Ohio: 'OH', Oklahoma: 'OK', Oregon: 'OR', Pennsylvania: 'PA', 'Puerto Rico': 'PR', 'Rhode Island': 'RI',
  'South Carolina': 'SC', 'South Dakota': 'SD', Tennessee: 'TN', Texas: 'TX', Utah: 'UT', Vermont: 'VT',
  Virginia: 'VA', Washington: 'WA', 'West Virginia': 'WV', Wisconsin: 'WI', Wyoming: 'WY',
};

// ---------- small helpers ----------

const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const round = (v) => (num(v) === null ? null : Math.round(v));
const iso = (ms) => new Date(ms).toISOString();
const COMPASS = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW'];
const compass = (deg) => (num(deg) === null ? null : COMPASS[Math.round(deg / 22.5) % 16]);

async function getJson(fetchFn, url, { headers = {}, retries = 1, timeoutMs = 12000 } = {}) {
  let lastErr;
  for (let attempt = 0; attempt <= retries; attempt++) {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), timeoutMs);
    try {
      const res = await fetchFn(url, { headers, signal: ctl.signal });
      if (res.status === 404) {
        const e = new Error('Not found');
        e.notFound = true;
        throw e;
      }
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch (e) {
      lastErr = e;
      if (e.notFound) break;
      if (attempt < retries) await new Promise((r) => setTimeout(r, 800));
    } finally {
      clearTimeout(timer);
    }
  }
  throw lastErr;
}

// ---------- WMO weather codes (Open-Meteo) ----------

const WMO = {
  0: ['clear', 'Clear'], 1: ['mostly-clear', 'Mostly Clear'], 2: ['partly-cloudy', 'Partly Cloudy'],
  3: ['cloudy', 'Cloudy'], 45: ['fog', 'Fog'], 48: ['fog', 'Freezing Fog'],
  51: ['drizzle', 'Light Drizzle'], 53: ['drizzle', 'Drizzle'], 55: ['drizzle', 'Heavy Drizzle'],
  56: ['sleet', 'Freezing Drizzle'], 57: ['sleet', 'Freezing Drizzle'],
  61: ['rain', 'Light Rain'], 63: ['rain', 'Rain'], 65: ['heavy-rain', 'Heavy Rain'],
  66: ['sleet', 'Freezing Rain'], 67: ['sleet', 'Freezing Rain'],
  71: ['snow', 'Light Snow'], 73: ['snow', 'Snow'], 75: ['snow', 'Heavy Snow'], 77: ['snow', 'Snow Grains'],
  80: ['rain', 'Light Showers'], 81: ['rain', 'Showers'], 82: ['heavy-rain', 'Heavy Showers'],
  85: ['snow', 'Snow Showers'], 86: ['snow', 'Heavy Snow Showers'],
  95: ['thunderstorm', 'Thunderstorms'], 96: ['thunderstorm', 'Thunderstorms with Hail'],
  99: ['thunderstorm', 'Thunderstorms with Hail'],
};
const wmoIcon = (code) => (WMO[code] ? WMO[code][0] : 'cloudy');
const wmoText = (code) => (WMO[code] ? WMO[code][1] : '');

function precipKind(code, precip, snowfall) {
  if (code >= 95) return 'storm';
  if ([56, 57, 66, 67].includes(code)) return 'mix';
  if ([71, 73, 75, 77, 85, 86].includes(code)) return 'snow';
  if (num(snowfall) > 0) return num(precip) > 0 && code >= 51 && code <= 67 ? 'mix' : 'snow';
  return 'rain';
}

// Icon from an NWS short forecast, used only when Open-Meteo is unavailable.
function iconFromText(text) {
  const t = String(text || '').toLowerCase();
  if (/thunder|t-storm|tstorm/.test(t)) return 'thunderstorm';
  if (/sleet|freezing|ice|wintry|rain.*snow|snow.*rain/.test(t)) return 'sleet';
  if (/snow|flurr|blizzard/.test(t)) return 'snow';
  if (/heavy rain/.test(t)) return 'heavy-rain';
  if (/drizzle/.test(t)) return 'drizzle';
  if (/rain|shower/.test(t)) return 'rain';
  if (/fog|haze|mist|smoke/.test(t)) return 'fog';
  if (/wind|breezy|blustery/.test(t) && !/cloud|sun|clear/.test(t)) return 'wind';
  if (/partly (sunny|cloudy)|mostly cloudy/.test(t)) return /mostly cloudy/.test(t) ? 'cloudy' : 'partly-cloudy';
  if (/mostly (sunny|clear)/.test(t)) return 'mostly-clear';
  if (/cloud|overcast/.test(t)) return 'cloudy';
  return 'clear';
}

// ---------- zip lookup ----------

async function lookupZip(fetchFn, zipInput) {
  const zip = String(zipInput || '').trim();
  if (!/^\d{5}$/.test(zip)) return { ok: false, error: 'Enter a 5-digit US zip code.' };
  let networkError = false;
  try {
    const j = await getJson(fetchFn, `https://api.zippopotam.us/us/${zip}`, { retries: 0 });
    const p = j && j.places && j.places[0];
    if (p) {
      return {
        ok: true, zip, city: p['place name'], state: p['state abbreviation'],
        lat: Number(Number(p.latitude).toFixed(4)), lon: Number(Number(p.longitude).toFixed(4)),
      };
    }
  } catch (e) {
    if (!e.notFound) networkError = true;
  }
  // Fallback: Open-Meteo geocoding understands postal codes too.
  try {
    const j = await getJson(fetchFn,
      `https://geocoding-api.open-meteo.com/v1/search?name=${zip}&count=10&language=en&format=json`, { retries: 0 });
    const hit = (j.results || []).find((r) => r.country_code === 'US' && (r.postcodes || []).includes(zip));
    if (hit) {
      return {
        ok: true, zip, city: hit.name, state: STATES[hit.admin1] || hit.admin1 || '',
        lat: Number(hit.latitude.toFixed(4)), lon: Number(hit.longitude.toFixed(4)),
      };
    }
  } catch (e) {
    networkError = true;
  }
  return networkError
    ? { ok: false, error: 'Could not reach the zip lookup service. Check your connection and try again.' }
    : { ok: false, error: `No US location found for ${zip}.` };
}

// ---------- fetchers ----------

const nwsHeaders = { 'User-Agent': USER_AGENT, Accept: 'application/geo+json' };

async function fetchNws(fetchFn, loc, pointCache) {
  const key = `${loc.lat},${loc.lon}`;
  let point = pointCache && pointCache[key];
  if (!point) {
    const j = await getJson(fetchFn, `https://api.weather.gov/points/${key}`, { headers: nwsHeaders });
    const p = j.properties;
    point = { forecast: p.forecast, forecastHourly: p.forecastHourly, timeZone: p.timeZone };
    if (pointCache) pointCache[key] = point;
  }
  const [daily, hourly, alerts] = await Promise.allSettled([
    getJson(fetchFn, point.forecast, { headers: nwsHeaders }),
    getJson(fetchFn, point.forecastHourly, { headers: nwsHeaders }),
    getJson(fetchFn, `https://api.weather.gov/alerts/active?point=${key}`, { headers: nwsHeaders }),
  ]);
  if (daily.status === 'rejected' && hourly.status === 'rejected' && alerts.status === 'rejected') {
    throw daily.reason;
  }
  return {
    timeZone: point.timeZone,
    daily: daily.status === 'fulfilled' ? daily.value.properties.periods || [] : [],
    hourly: hourly.status === 'fulfilled' ? hourly.value.properties.periods || [] : [],
    alerts: alerts.status === 'fulfilled' ? alerts.value.features || [] : null, // null = unknown
  };
}

function openMeteoUrl(loc) {
  const q = new URLSearchParams({
    latitude: String(loc.lat), longitude: String(loc.lon),
    current: 'temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,weather_code,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m',
    minutely_15: 'precipitation,snowfall,weather_code',
    hourly: 'temperature_2m,apparent_temperature,relative_humidity_2m,dew_point_2m,precipitation_probability,precipitation,weather_code,wind_speed_10m,wind_direction_10m,visibility,uv_index,is_day',
    daily: 'weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,precipitation_probability_max',
    temperature_unit: 'fahrenheit', wind_speed_unit: 'mph', precipitation_unit: 'inch',
    timezone: 'auto', timeformat: 'unixtime', forecast_days: '8', forecast_minutely_15: '52', past_minutely_15: '0',
  });
  return `https://api.open-meteo.com/v1/forecast?${q}`;
}

const fetchOpenMeteo = (fetchFn, loc) => getJson(fetchFn, openMeteoUrl(loc));

// ---------- normalisation ----------

function localDate(ms, timeZone) {
  // en-CA formats as YYYY-MM-DD
  return new Intl.DateTimeFormat('en-CA', { timeZone, year: 'numeric', month: '2-digit', day: '2-digit' }).format(ms);
}
function clock(ms, timeZone) {
  const s = new Intl.DateTimeFormat('en-US', { timeZone, hour: 'numeric', minute: '2-digit' }).format(ms);
  return s.replace(':00', '');
}
function weekday(dateStr) {
  return new Intl.DateTimeFormat('en-US', { timeZone: 'UTC', weekday: 'short' }).format(new Date(`${dateStr}T12:00:00Z`));
}
function addDays(dateStr, n) {
  return new Date(Date.parse(`${dateStr}T12:00:00Z`) + n * 86400000).toISOString().slice(0, 10);
}
const windMph = (s) => {
  const m = String(s || '').match(/(\d+)(?:\s*to\s*(\d+))?/);
  return m ? Number(m[2] || m[1]) : null;
};
const cToF = (c) => (num(c) === null ? null : Math.round(c * 9 / 5 + 32));

const KIND_WORD = { rain: 'Rain', snow: 'Snow', mix: 'Wintry mix', storm: 'Thunderstorms' };

function buildMinutely(om, now) {
  const m = om && om.minutely_15;
  if (!m || !m.time) return [];
  const out = [];
  for (let i = 0; i < m.time.length; i++) {
    const t = m.time[i] * 1000;
    if (t + 15 * 60000 <= now) continue;
    const precip = num(m.precipitation[i]) || 0;
    const wet = precip >= WET_THRESHOLD;
    out.push({
      time: iso(t), ms: t, precip: Math.round(precip * 1000) / 1000,
      kind: wet ? precipKind(m.weather_code ? m.weather_code[i] : 61, precip, m.snowfall ? m.snowfall[i] : 0) : null,
    });
  }
  return out.slice(0, 48);
}

function buildNextPrecip(steps, hourly, now, timeZone) {
  if (steps.length) {
    const start = steps.findIndex((s) => s.kind);
    if (start === -1) {
      const hours = Math.round((steps[steps.length - 1].ms + 15 * 60000 - now) / 3600000);
      return { kind: null, isNow: false, startsAt: null, endsAt: null, minutesUntil: null,
        headline: `Dry for the next ${Math.max(1, hours)} hours` };
    }
    // The spell ends at the first gap of two dry steps (30 minutes).
    let end = -1;
    for (let i = start + 1; i < steps.length; i++) {
      if (!steps[i].kind && (i + 1 >= steps.length || !steps[i + 1].kind)) { end = i; break; }
    }
    const spell = steps.slice(start, end === -1 ? steps.length : end);
    const kinds = new Set(spell.map((s) => s.kind));
    const kind = kinds.has('storm') ? 'storm' : kinds.has('mix') || (kinds.has('snow') && kinds.has('rain')) ? 'mix'
      : kinds.has('snow') ? 'snow' : 'rain';
    const word = KIND_WORD[kind];
    const startMs = steps[start].ms;
    const endMs = end === -1 ? null : steps[end].ms;
    const isNow = startMs <= now;
    const minutesUntil = isNow ? 0 : Math.max(1, Math.round((startMs - now) / 60000));
    let headline;
    if (isNow) headline = endMs ? `${word} until ${clock(endMs, timeZone)}` : `${word} continuing for hours`;
    else if (minutesUntil < 60) headline = `${word} starting in ${minutesUntil} min`;
    else headline = `${word} around ${clock(startMs, timeZone)}`;
    return { kind, isNow, startsAt: iso(startMs), endsAt: endMs ? iso(endMs) : null, minutesUntil, headline };
  }
  // No 15-minute data: fall back to hourly chance of precipitation.
  const next = hourly.slice(0, 12).find((h) => (h.precipProb || 0) >= 50);
  if (next) {
    const kind = { snow: 'snow', sleet: 'mix', thunderstorm: 'storm' }[next.icon] || 'rain';
    const ms = Date.parse(next.time);
    const isNow = ms <= now;
    return { kind, isNow, startsAt: next.time, endsAt: null,
      minutesUntil: isNow ? 0 : Math.round((ms - now) / 60000),
      headline: isNow ? `${KIND_WORD[kind]} likely now (${next.precipProb}%)`
        : `${KIND_WORD[kind]} likely around ${clock(ms, timeZone)} (${next.precipProb}%)` };
  }
  return { kind: null, isNow: false, startsAt: null, endsAt: null, minutesUntil: null,
    headline: hourly.length ? 'No precipitation expected in the next 12 hours' : 'Precipitation timing unavailable' };
}

function buildHourly(nws, om, now) {
  const omByHour = new Map();
  const h = om && om.hourly;
  if (h && h.time) {
    for (let i = 0; i < h.time.length; i++) omByHour.set(h.time[i] * 1000, i);
  }
  const hourStart = Math.floor(now / 3600000) * 3600000;
  const out = [];
  const fromOm = (i, base = {}) => ({
    feelsLike: round(h.apparent_temperature[i]),
    icon: wmoIcon(h.weather_code[i]),
    isDay: h.is_day[i] === 1,
    precipAmount: num(h.precipitation[i]),
    ...base,
  });
  if (nws && nws.hourly.length) {
    for (const p of nws.hourly) {
      const t = Date.parse(p.startTime);
      if (t < hourStart) continue;
      const i = omByHour.get(Math.floor(t / 3600000) * 3600000);
      const row = {
        time: iso(t), temp: round(p.temperature), feelsLike: null, icon: iconFromText(p.shortForecast),
        isDay: !!p.isDaytime, condition: p.shortForecast || '',
        precipProb: num(p.probabilityOfPrecipitation && p.probabilityOfPrecipitation.value),
        precipAmount: null, windSpeed: windMph(p.windSpeed), windDir: p.windDirection || null,
        humidity: num(p.relativeHumidity && p.relativeHumidity.value),
        dewPoint: cToF(p.dewpoint && p.dewpoint.value),
      };
      out.push(i === undefined ? row : { ...row, ...fromOm(i) });
      if (out.length >= 48) break;
    }
    return out;
  }
  if (h && h.time) {
    for (let i = 0; i < h.time.length && out.length < 48; i++) {
      const t = h.time[i] * 1000;
      if (t < hourStart) continue;
      out.push(fromOm(i, {
        time: iso(t), temp: round(h.temperature_2m[i]), condition: wmoText(h.weather_code[i]),
        precipProb: num(h.precipitation_probability[i]), windSpeed: round(h.wind_speed_10m[i]),
        windDir: compass(h.wind_direction_10m[i]), humidity: num(h.relative_humidity_2m[i]),
        dewPoint: round(h.dew_point_2m[i]),
      }));
    }
  }
  return out;
}

function buildDaily(nws, om, now, timeZone) {
  const today = localDate(now, timeZone);
  const days = new Map();
  for (let i = 0; i < 7; i++) {
    const date = addDays(today, i);
    days.set(date, {
      date, name: i === 0 ? 'Today' : weekday(date), high: null, low: null, icon: null, condition: '',
      precipProb: null, dayText: '', nightText: '', sunrise: null, sunset: null,
    });
  }
  if (nws) {
    for (const p of nws.daily) {
      // NWS start times carry the local offset, so the first 10 characters are the local date.
      // An overnight period that started before midnight belongs to the previous day.
      let date = String(p.startTime).slice(0, 10);
      if (!p.isDaytime && Number(String(p.startTime).slice(11, 13)) < 6) date = addDays(date, -1);
      const d = days.get(date);
      if (!d) continue;
      const pop = num(p.probabilityOfPrecipitation && p.probabilityOfPrecipitation.value);
      if (pop !== null) d.precipProb = Math.max(d.precipProb || 0, pop);
      if (p.isDaytime) {
        d.high = round(p.temperature);
        d.dayText = p.detailedForecast || '';
        d.condition = p.shortForecast || d.condition;
        d.icon = iconFromText(p.shortForecast);
      } else {
        d.low = round(p.temperature);
        d.nightText = p.detailedForecast || '';
        if (!d.condition) { d.condition = p.shortForecast || ''; d.icon = iconFromText(p.shortForecast); }
      }
    }
  }
  const o = om && om.daily;
  if (o && o.time) {
    const offset = (om.utc_offset_seconds || 0) * 1000;
    for (let i = 0; i < o.time.length; i++) {
      const d = days.get(new Date(o.time[i] * 1000 + offset).toISOString().slice(0, 10));
      if (!d) continue;
      if (d.high === null) d.high = round(o.temperature_2m_max[i]);
      if (d.low === null) d.low = round(o.temperature_2m_min[i]);
      if (d.precipProb === null) d.precipProb = num(o.precipitation_probability_max[i]);
      if (!d.condition) d.condition = wmoText(o.weather_code[i]);
      d.icon = wmoIcon(o.weather_code[i]); // one consistent icon source when available
      d.sunrise = o.sunrise[i] ? iso(o.sunrise[i] * 1000) : null;
      d.sunset = o.sunset[i] ? iso(o.sunset[i] * 1000) : null;
    }
  }
  const list = [...days.values()].filter((d) => d.high !== null || d.low !== null);
  for (const d of list) if (!d.icon) d.icon = 'cloudy';
  return list;
}

const SEVERITY_RANK = { Extreme: 0, Severe: 1, Moderate: 2, Minor: 3, Unknown: 4 };

function buildAlerts(features, now) {
  return (features || [])
    .map((f) => f.properties || {})
    .filter((p) => p.status === 'Actual' && p.messageType !== 'Cancel')
    .filter((p) => {
      const end = Date.parse(p.ends || p.expires || '');
      return !Number.isFinite(end) || end > now;
    })
    .map((p) => ({
      id: p.id || p['@id'] || `${p.event}-${p.sent}`,
      event: p.event || 'Weather Alert',
      severity: SEVERITY_RANK[p.severity] === undefined ? 'Unknown' : p.severity,
      urgency: p.urgency || 'Unknown', certainty: p.certainty || 'Unknown',
      headline: p.headline || p.event || '', description: p.description || '', instruction: p.instruction || '',
      onset: p.onset || p.effective || null, ends: p.ends || null, expires: p.expires || null,
      sender: p.senderName || 'National Weather Service', areaDesc: p.areaDesc || '',
    }))
    .sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity]);
}

function buildCurrent(om, hourly, omHourIndex) {
  const c = om && om.current;
  const h0 = hourly[0];
  if (c) {
    const oh = om.hourly;
    const i = omHourIndex;
    let visibility = null;
    if (oh && i >= 0 && num(oh.visibility[i]) !== null) {
      const unit = (om.hourly_units && om.hourly_units.visibility) || 'm';
      visibility = Math.round((oh.visibility[i] / (unit === 'ft' ? 5280 : 1609.344)) * 10) / 10;
      if (visibility > 10) visibility = Math.round(visibility);
    }
    return {
      temp: round(c.temperature_2m), feelsLike: round(c.apparent_temperature),
      condition: (h0 && h0.condition) || wmoText(c.weather_code), icon: wmoIcon(c.weather_code),
      isDay: c.is_day === 1, humidity: num(c.relative_humidity_2m),
      dewPoint: oh && i >= 0 ? round(oh.dew_point_2m[i]) : (h0 ? h0.dewPoint : null),
      windSpeed: round(c.wind_speed_10m), windGust: round(c.wind_gusts_10m), windDir: compass(c.wind_direction_10m),
      pressure: num(c.pressure_msl) === null ? null : Math.round(c.pressure_msl * 0.02953 * 100) / 100,
      visibility, uvIndex: oh && i >= 0 ? round(oh.uv_index[i]) : null,
    };
  }
  if (h0) {
    return {
      temp: h0.temp, feelsLike: h0.feelsLike, condition: h0.condition, icon: h0.icon, isDay: h0.isDay,
      humidity: h0.humidity, dewPoint: h0.dewPoint, windSpeed: h0.windSpeed, windGust: null, windDir: h0.windDir,
      pressure: null, visibility: null, uvIndex: null,
    };
  }
  return null;
}

/**
 * Fetch everything for a location and return a WeatherState.
 * @param fetchFn  fetch implementation
 * @param loc      { zip, city, state, lat, lon }
 * @param opts     { pointCache, now, previousAlerts }
 */
async function fetchWeather(fetchFn, loc, opts = {}) {
  const now = opts.now || Date.now();
  const [nwsR, omR] = await Promise.allSettled([
    fetchNws(fetchFn, loc, opts.pointCache),
    fetchOpenMeteo(fetchFn, loc),
  ]);
  const nws = nwsR.status === 'fulfilled' ? nwsR.value : null;
  const om = omR.status === 'fulfilled' ? omR.value : null;
  if (!nws && !om) {
    const e = new Error('Could not reach the National Weather Service or Open-Meteo. Check your internet connection.');
    e.cause = nwsR.reason;
    throw e;
  }
  const timeZone = (nws && nws.timeZone) || (om && om.timezone) || 'America/Chicago';
  const hourly = buildHourly(nws, om, now);
  const daily = buildDaily(nws, om, now, timeZone);
  const steps = buildMinutely(om, now);
  const nextPrecip = buildNextPrecip(steps, hourly, now, timeZone);

  let omHourIndex = -1;
  if (om && om.hourly && om.hourly.time) {
    const hourStart = Math.floor(now / 3600000) * 3600;
    omHourIndex = om.hourly.time.indexOf(hourStart);
  }
  const current = buildCurrent(om, hourly, omHourIndex);
  const todayRow = daily[0] || null;
  const summary = nws && nws.daily[0] ? nws.daily[0].detailedForecast || '' : '';

  const missing = [];
  if (!nws) missing.push('National Weather Service');
  if (!om) missing.push('Open-Meteo');

  return {
    status: 'ok',
    error: missing.length ? `${missing.join(' and ')} did not respond. Showing what is available.` : null,
    stale: false,
    updatedAt: iso(now),
    sources: { nws: !!nws, openMeteo: !!om },
    location: { ...loc, timeZone },
    current,
    today: todayRow ? {
      high: todayRow.high, low: todayRow.low, sunrise: todayRow.sunrise, sunset: todayRow.sunset, summary,
    } : null,
    nextPrecip,
    minutely: steps.slice(0, 24).map(({ time, precip, kind }) => ({ time, precip, kind })),
    hourly: hourly.map(({ dewPoint, ...rest }) => rest),
    daily,
    // If the alerts call itself failed, keep showing the alerts we already knew about.
    alerts: nws && nws.alerts ? buildAlerts(nws.alerts, now) : (opts.previousAlerts || []),
  };
}

module.exports = { fetchWeather, lookupZip, USER_AGENT, _internal: { iconFromText, buildNextPrecip, buildMinutely } };
