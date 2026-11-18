'use strict';
// Logic tests against hand-built responses in the documented NWS / Open-Meteo shapes.
const test = require('node:test');
const assert = require('node:assert');
const { fetchWeather, lookupZip } = require('../src/main/weather');

const NOW = Date.parse('2026-11-18T20:07:00Z'); // 2:07 PM Central
const H = 3600;
const hour0 = Math.floor(NOW / 1000 / H) * H;
const loc = { zip: '60601', city: 'Chicago', state: 'IL', lat: 41.8858, lon: -87.6181 };

function nwsPeriods() {
  const mk = (name, start, isDaytime, temperature, pop, shortForecast) => ({
    name, startTime: start, isDaytime, temperature, temperatureUnit: 'F',
    probabilityOfPrecipitation: { value: pop }, windSpeed: '10 to 15 mph', windDirection: 'NW',
    shortForecast, detailedForecast: `${shortForecast}, with a ${isDaytime ? 'high' : 'low'} near ${temperature}.`,
  });
  return [
    mk('This Afternoon', '2026-11-18T14:00:00-06:00', true, 44, 20, 'Partly Sunny'),
    mk('Tonight', '2026-11-18T18:00:00-06:00', false, 31, 80, 'Rain And Snow'),
    mk('Thursday', '2026-11-19T06:00:00-06:00', true, 38, 40, 'Chance Snow Showers'),
    mk('Thursday Night', '2026-11-19T18:00:00-06:00', false, 27, null, 'Mostly Clear'),
  ];
}
function nwsHourly() {
  return Array.from({ length: 60 }, (_, i) => ({
    startTime: new Date((hour0 + i * H) * 1000).toISOString(), isDaytime: i < 3, temperature: 44 - i,
    probabilityOfPrecipitation: { value: i > 2 ? 70 : 10 }, windSpeed: '12 mph', windDirection: 'NW',
    shortForecast: i > 2 ? 'Rain Likely' : 'Partly Sunny', relativeHumidity: { value: 60 }, dewpoint: { value: 1 },
  }));
}
function openMeteo() {
  const q0 = Math.floor(NOW / 1000 / 900) * 900;
  const n = 52;
  const wet = (i) => i >= 2 && i < 10; // rain from +30 min for two hours
  const hours = Array.from({ length: 192 }, (_, i) => hour0 - 14 * H + i * H);
  const fill = (v) => hours.map(() => v);
  return {
    timezone: 'America/Chicago', utc_offset_seconds: -21600, hourly_units: { visibility: 'ft' },
    current: { temperature_2m: 43.6, relative_humidity_2m: 61, apparent_temperature: 38.2, is_day: 1, precipitation: 0,
      weather_code: 2, pressure_msl: 1016, wind_speed_10m: 11.6, wind_direction_10m: 315, wind_gusts_10m: 22 },
    minutely_15: { time: Array.from({ length: n }, (_, i) => q0 + i * 900),
      precipitation: Array.from({ length: n }, (_, i) => (wet(i) ? 0.02 : 0)),
      snowfall: Array.from({ length: n }, () => 0), weather_code: Array.from({ length: n }, (_, i) => (wet(i) ? 61 : 2)) },
    hourly: { time: hours, temperature_2m: fill(40), apparent_temperature: fill(35), relative_humidity_2m: fill(60),
      dew_point_2m: fill(30), precipitation_probability: fill(20), precipitation: fill(0.01), weather_code: fill(61),
      wind_speed_10m: fill(10), wind_direction_10m: fill(300), visibility: fill(52800), uv_index: fill(1.2), is_day: fill(1) },
    daily: { time: Array.from({ length: 8 }, (_, i) => Date.parse('2026-11-18T06:00:00Z') / 1000 + i * 86400),
      weather_code: Array(8).fill(63), temperature_2m_max: Array(8).fill(45.2), temperature_2m_min: Array(8).fill(29.9),
      sunrise: Array(8).fill(hour0 - 8 * H), sunset: Array(8).fill(hour0 + 2 * H), precipitation_probability_max: Array(8).fill(55) },
  };
}
function fakeFetch({ nws = true, om = true, alerts = [] } = {}) {
  const calls = [];
  const json = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
  const fn = async (url, init) => {
    calls.push({ url, init });
    if (url.includes('api.weather.gov')) {
      if (!nws) return json({}, 503);
      if (url.includes('/points/')) return json({ properties: { forecast: 'https://api.weather.gov/f', forecastHourly: 'https://api.weather.gov/fh', timeZone: 'America/Chicago' } });
      if (url.endsWith('/f')) return json({ properties: { periods: nwsPeriods() } });
      if (url.endsWith('/fh')) return json({ properties: { periods: nwsHourly() } });
      if (url.includes('/alerts/')) return json({ features: alerts });
    }
    if (url.includes('api.open-meteo.com')) return om ? json(openMeteo()) : json({}, 500);
    if (url.includes('zippopotam')) {
      return url.endsWith('/60601')
        ? json({ places: [{ 'place name': 'Chicago', 'state abbreviation': 'IL', latitude: '41.8858', longitude: '-87.6181' }] })
        : json({}, 404);
    }
    if (url.includes('geocoding-api')) return json({ results: [] });
    throw new Error('unexpected ' + url);
  };
  fn.calls = calls;
  return fn;
}

test('merges both sources', async () => {
  const f = fakeFetch({ alerts: [
    { properties: { id: 'a', status: 'Actual', event: 'Wind Advisory', severity: 'Moderate', expires: '2026-11-19T06:00:00Z' } },
    { properties: { id: 'b', status: 'Actual', event: 'Winter Storm Warning', severity: 'Severe', expires: '2026-11-19T06:00:00Z' } },
    { properties: { id: 'c', status: 'Actual', event: 'Old', severity: 'Extreme', expires: '2026-11-18T06:00:00Z' } },
  ] });
  const s = await fetchWeather(f, loc, { now: NOW });
  assert.equal(s.status, 'ok');
  assert.equal(s.error, null);
  assert.deepEqual(s.sources, { nws: true, openMeteo: true });
  assert.equal(s.current.temp, 44);
  assert.equal(s.current.condition, 'Partly Sunny');
  assert.equal(s.current.windDir, 'NW');
  assert.equal(s.current.visibility, 10);
  assert.equal(s.current.pressure, 30);
  assert.equal(s.today.high, 44);
  assert.equal(s.today.low, 31);
  assert.equal(s.daily[0].name, 'Today');
  assert.equal(s.daily[1].name, 'Thu');
  assert.equal(s.daily[1].high, 38);
  assert.equal(s.daily[1].low, 27);
  assert.equal(s.daily[0].precipProb, 80);
  assert.equal(s.daily.length, 7); // later days filled from Open-Meteo
  assert.equal(s.hourly.length, 48);
  assert.equal(s.hourly[0].temp, 44);
  assert.equal(s.hourly[0].icon, 'rain');
  assert.equal(s.minutely.length, 24);
  assert.equal(s.nextPrecip.kind, 'rain');
  assert.equal(s.nextPrecip.isNow, false);
  assert.equal(s.nextPrecip.headline, 'Rain starting in 23 min');
  assert.deepEqual(s.alerts.map((a) => a.id), ['b', 'a']);
  const nwsCall = f.calls.find((c) => c.url.includes('weather.gov'));
  assert.match(nwsCall.init.headers['User-Agent'], /WeathaDock/);
});

test('falls back to Open-Meteo alone', async () => {
  const s = await fetchWeather(fakeFetch({ nws: false }), loc, { now: NOW, previousAlerts: [{ id: 'kept' }] });
  assert.equal(s.sources.nws, false);
  assert.match(s.error, /National Weather Service/);
  assert.equal(s.today.high, 45);
  assert.equal(s.hourly[0].condition, 'Light Rain');
  assert.equal(s.alerts[0].id, 'kept');
});

test('falls back to NWS alone', async () => {
  const s = await fetchWeather(fakeFetch({ om: false }), loc, { now: NOW });
  assert.equal(s.current.temp, 44);
  assert.equal(s.minutely.length, 0);
  assert.equal(s.nextPrecip.kind, 'rain');
  assert.match(s.nextPrecip.headline, /Rain likely around 5 PM \(70%\)/);
  assert.equal(s.daily.length, 2);
});

test('throws when both are down', async () => {
  await assert.rejects(fetchWeather(fakeFetch({ nws: false, om: false }), loc, { now: NOW }), /Could not reach/);
});

test('zip lookup', async () => {
  const f = fakeFetch();
  assert.deepEqual(await lookupZip(f, '60601'), { ok: true, zip: '60601', city: 'Chicago', state: 'IL', lat: 41.8858, lon: -87.6181 });
  assert.equal((await lookupZip(f, '00000')).ok, false);
  assert.match((await lookupZip(f, 'abc')).error, /5-digit/);
});
