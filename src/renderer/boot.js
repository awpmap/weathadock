/* Runs synchronously in <head>: puts glass + platform on <html> before first paint. */
(function () {
  var p = new URLSearchParams(location.search);
  var h = document.documentElement;
  h.dataset.glass = p.get('glass') === 'frosted' ? 'frosted' : 'clear';
  var plat = p.get('platform');
  h.dataset.platform = plat === 'darwin' || plat === 'linux' ? plat : 'win32';
})();
