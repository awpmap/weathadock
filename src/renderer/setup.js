/* WeathaDock first-run setup */
(function () {
  'use strict';
  var U = window.SkyUI, I = window.SkyIcons, esc = U.esc;
  var $ = function (id) { return document.getElementById(id); };
  var zip = $('zip'), status = $('zip-status'), start = $('start'), err = $('error');
  var LOOKS = ['none', 'clear', 'frosted'];
  var glass = 'none'; /* Transparent is the default look */
  var resolved = null, token = 0, timer = null, busy = false;

  $('mark').innerHTML = I.wx('partly-cloudy', true, { animated: true });
  document.querySelectorAll('[data-wx]').forEach(function (el) { el.outerHTML = I.wx(el.dataset.wx, true); });
  var closeBtn = document.querySelector('[data-win="close"]');
  closeBtn.innerHTML = I.gl('win-close');
  closeBtn.addEventListener('click', function () { window.sky.windowControl('close'); });

  function setStatus(kind, html) { status.className = 'inline-status' + (kind ? ' is-' + kind : ''); status.innerHTML = html || ''; }
  function setError(msg) { err.hidden = !msg; $('note').hidden = !!msg; err.innerHTML = msg ? I.gl('info') + '<span>' + esc(msg) + '</span>' : ''; }
  function sync() {
    start.disabled = !resolved || busy;
    start.innerHTML = busy ? I.gl('refresh', 'spin') + 'Setting up…' : (resolved ? 'Start in ' + esc(resolved.city) : 'Start');
  }
  function pickLook(g) {
    glass = g;
    document.querySelectorAll('#looks .look').forEach(function (b) { var on = b.dataset.glass === g; b.setAttribute('aria-checked', on ? 'true' : 'false'); b.tabIndex = on ? 0 : -1; });
  }
  document.querySelectorAll('#looks .look').forEach(function (b) { b.addEventListener('click', function () { pickLook(b.dataset.glass); }); });
  $('looks').addEventListener('keydown', function (e) {
    if (['ArrowRight', 'ArrowLeft', 'ArrowUp', 'ArrowDown'].indexOf(e.key) < 0) return;
    e.preventDefault();
    var step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : LOOKS.length - 1;
    pickLook(LOOKS[(LOOKS.indexOf(glass) + step) % LOOKS.length]);
    document.querySelector('#looks .look[aria-checked="true"]').focus();
  });
  pickLook(glass);

  zip.addEventListener('input', function () {
    var v = zip.value.replace(/\D/g, '').slice(0, 5);
    if (v !== zip.value) zip.value = v;
    clearTimeout(timer); token++; resolved = null;
    zip.classList.remove('is-bad'); setError('');
    if (v.length < 5) { setStatus('', v.length ? '<span style="color:var(--t3)">' + (5 - v.length) + ' more digit' + (5 - v.length === 1 ? '' : 's') + '</span>' : ''); sync(); return; }
    var tk = token;
    setStatus('busy', I.gl('refresh', 'spin') + 'Finding ' + esc(v) + '…');
    sync();
    timer = setTimeout(function () {
      Promise.resolve(window.sky.lookupZip(v)).then(function (r) {
        if (tk !== token) return;
        if (r && r.ok) { resolved = r; setStatus('ok', I.gl('check-circle') + '<span>' + esc(r.city) + ', ' + esc(r.state) + '</span>'); }
        else { zip.classList.add('is-bad'); setStatus('bad', I.gl('info') + esc((r && r.error) || 'We couldn’t find that ZIP code.')); }
        sync();
      }).catch(function () {
        if (tk !== token) return;
        zip.classList.add('is-bad'); setStatus('bad', I.gl('info') + 'Couldn’t check that ZIP code. Check your connection and try again.'); sync();
      });
    }, 200);
  });

  $('form').addEventListener('submit', function (e) {
    e.preventDefault();
    if (!resolved || busy) return;
    busy = true; setError(''); sync();
    Promise.resolve(window.sky.completeSetup({ zip: resolved.zip, glass: glass })).then(function (r) {
      if (r && r.ok) { start.innerHTML = I.gl('check') + 'All set'; return; }
      busy = false; setError((r && r.error) || 'Something went wrong. Please try again.'); sync();
    }).catch(function () { busy = false; setError('Something went wrong. Please try again.'); sync(); });
  });

  U.ready().then(function (sky) {
    sync();
    sky.getSettings().then(function (s) {
      if (s && s.zip && !zip.value) { zip.value = s.zip; zip.dispatchEvent(new Event('input')); }
    }).catch(function () {});
    zip.focus();
  });
})();
