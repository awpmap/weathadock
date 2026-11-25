// Part of the temporary desktop lab. Talks to the app's own pages through the web view's
// debugging port, which only the lab run that asks for it opens: reads what a page believes it
// is showing, and clicks the way a person would.
//   node drive.js state | open | light | close | shot NAME
const port = process.env.LAB_PORT || 9222;
const [cmd, arg] = process.argv.slice(2);
const say = (text) => console.log('LAB   page: ' + text);

async function pages() {
  const r = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(4000) });
  return (await r.json()).filter((t) => t.type === 'page' && t.webSocketDebuggerUrl);
}
const file = (t) => { try { return new URL(t.url).pathname.split('/').pop() || t.url; } catch { return t.url; } };
const pick = (list, ...names) => { for (const n of names) { const t = list.find((t) => file(t) === n); if (t) return t; } return null; };

function send(target, method, params, timeout = 8000) {
  return new Promise((resolve) => {
    let done = false;
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    const finish = (v) => { if (done) return; done = true; clearTimeout(timer); try { ws.close(); } catch {} resolve(v); };
    const timer = setTimeout(() => finish({ error: `no answer in ${timeout} ms` }), timeout);
    ws.onopen = () => ws.send(JSON.stringify({ id: 1, method, params }));
    ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id === 1) finish(d.error ? { error: d.error.message } : { result: d.result }); };
    ws.onerror = () => finish({ error: 'could not connect to the page' });
  });
}
async function run(target, expression, timeout) {
  const a = await send(target, 'Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, timeout);
  if (a.error) return 'FAILED: ' + a.error;
  const x = a.result.exceptionDetails;
  if (x) return 'FAILED: ' + ((x.exception && x.exception.description) || x.text);
  return a.result.result.value;
}

// What a page believes it is showing, and the settings the app gives it.
const STATE = `(async () => {
  const d = document, h = d.documentElement;
  const o = { visibility: d.visibilityState, focus: d.hasFocus(), size: innerWidth + 'x' + innerHeight, htmlOpacity: getComputedStyle(h).opacity, htmlData: Object.assign({}, h.dataset) };
  const root = d.getElementById('root') || d.body;
  const cs = getComputedStyle(root), r = root.getBoundingClientRect();
  o.root = { data: Object.assign({}, root.dataset), cls: root.className, box: Math.round(r.width) + 'x' + Math.round(r.height), color: cs.color, opacity: cs.opacity, display: cs.display, visibility: cs.visibility, parts: root.childElementCount, text: (root.innerText || '').replace(/\\s+/g, ' ').slice(0, 80) };
  try {
    const s = await Promise.race([window.sky.getSettings(), new Promise((done) => setTimeout(done, 3000, 'no answer in 3 s'))]);
    o.settings = s && typeof s === 'object' ? { version: s.version, ink: s.widget && s.widget.ink, layout: s.layout, widgetSet: s.widgetSet, glass: s.glass, opacity: s.widgetOpacity, placement: s.placement, coveredMode: s.coveredMode, showWidget: s.showWidget, onTop: s.widgetOnTop } : s;
  } catch (e) { o.settings = 'FAILED: ' + e; }
  return JSON.stringify(o);
})()`;

// The "Light" button of Text color in the widget settings, clicked once it is there.
const LIGHT = `(async () => {
  const wait = (ms) => new Promise((done) => setTimeout(done, ms));
  for (let i = 0; i < 40; i++) {
    const b = document.querySelector('[data-cz="ink"] button[data-val="light"]');
    if (b) { const was = b.getAttribute('aria-checked'); b.click(); await wait(400); return 'clicked (was chosen: ' + was + ', is chosen now: ' + b.getAttribute('aria-checked') + ')'; }
    await wait(250);
  }
  return 'no such button on ' + location.pathname;
})()`;

const WHAT = {
  async state(list) { for (const t of list) say(file(t) + ' ' + await run(t, STATE)); },
  async open(list) {
    const t = pick(list, 'widget.html', 'pill.html');
    say(t ? 'asked ' + file(t) + " to open the widget settings: " + await run(t, "window.sky.openDetail('widget'); 'ok'") : 'no widget or strip page to ask');
  },
  async light(list) {
    const t = pick(list, 'detail.html');
    say(t ? 'Text color, Light: ' + await run(t, LIGHT, 15000) : 'the settings window has no page');
  },
  async close(list) {
    const t = pick(list, 'detail.html');
    say(t ? 'settings window closed with its own button: ' + await run(t, "window.sky.windowControl('close'); 'ok'") : 'the settings window has no page');
  },
  // What the widget's (or the strip's) page draws, on grey so that light and dark text both show.
  async shot(list) {
    const t = pick(list, 'widget.html', 'pill.html');
    if (!t) return say('no widget or strip page to picture');
    await run(t, "document.documentElement.style.background = '#808080'; new Promise((done) => setTimeout(done, 300, 'ok'))");
    const a = await send(t, 'Page.captureScreenshot', { format: 'jpeg', quality: 70 }, 8000);
    await run(t, "document.documentElement.style.background = ''; 'ok'");
    if (a.error) return say('no picture of ' + file(t) + ': ' + a.error);
    console.log(arg + '-BEGIN');
    for (const line of a.result.data.match(/.{1,4000}/g)) console.log(line);
    console.log(arg + '-END');
  }
};

(async () => {
  try {
    const list = await pages();
    if (!list.length) say('nothing on the debugging port');
    else await (WHAT[cmd] || WHAT.state)(list);
  } catch (e) { say('the debugging port did not answer (' + (e && e.message) + ')'); }
  process.exit(0);
})();
