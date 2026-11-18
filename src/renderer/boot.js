/* Runs synchronously in <head>: puts glass + platform on <html> before first paint. */
(function () {
  var p = new URLSearchParams(location.search);
  var h = document.documentElement;
  var g = p.get('glass');
  h.dataset.glass = g === 'clear' || g === 'frosted' ? g : 'none';
  var plat = p.get('platform');
  h.dataset.platform = plat === 'darwin' || plat === 'linux' ? plat : 'win32';
})();
