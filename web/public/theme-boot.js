// Sets <html data-theme> before first paint (spec 5, D139): the choice remembered in this browser
// (dark, light or device; key kl.theme), else the device setting; no preference means dark.
// An external file so the production CSP needs no inline script. Mirrors src/theme/theme.ts.
(function () {
  var choice = 'device';
  try {
    var v = localStorage.getItem('kl.theme');
    if (v === 'dark' || v === 'light' || v === 'device') choice = v;
  } catch (e) {}
  var light = false;
  try {
    light = window.matchMedia('(prefers-color-scheme: light)').matches;
  } catch (e) {}
  document.documentElement.dataset.theme = choice === 'device' ? (light ? 'light' : 'dark') : choice;
})();
