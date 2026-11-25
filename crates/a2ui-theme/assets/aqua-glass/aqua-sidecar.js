// Ablation of the Aqua Glass mechanisms for visual review: ?aqua-off=glass,rear,beam turns the
// neutral glass, the rear illumination and the prismatic light off independently.
(function () {
  const off = new URLSearchParams(location.search).get('aqua-off');
  if (!off) return;
  const root = document.documentElement;
  for (const t of off.split(',')) {
    if (t === 'rear' || t === 'beam') root.style.setProperty('--aqua-l-' + t, '0');
    if (t === 'glass') root.setAttribute('data-aqua-glass', 'off');
  }
})();
// RPT-AQUA-GLASS-1: the appearance sidecar of the preferences menu. The page itself is the
// preview: every change sets the --aqua-* properties on <html> at once and is remembered in
// the cookies the server reads (<prefix>aqua and <prefix>material; the prefix is the sidecar's
// data-cookie-prefix, "report_" when absent). Hover opens it on pointer devices, a click on the
// eye pins it, Escape closes it; touch opens it as a sheet.
//
// Material slider (optional): an <input type="range" data-material-slider> whose position i
// selects the i-th key of data-keys (comma-separated, the default first), labelled by the i-th
// entry of data-labels ("|"-separated) in <output data-material-label>. data-css maps a key to
// the stylesheet it needs ("key=href;key=href"); a missing sheet is linked on first use. The
// material only switches: each one keeps its own stylesheet and look. Moving a light control
// while another material is chosen switches to aqua-glass, the material the light belongs to.
(function () {
  const entry = document.querySelector('[data-appearance]');
  if (!entry) return;
  const eye = entry.querySelector('.ap-eye');
  const side = entry.querySelector('.ap-sidecar');
  const menu = entry.closest('details');
  if (!eye || !side) return;
  // Fixed positioning must not be trapped by a header that blurs or transforms: the sidecar
  // lives at the end of <body> and is placed beside the menu.
  document.body.appendChild(side);
  const inside = (el) => !!el && (entry.contains(el) || side.contains(el));
  const root = document.documentElement;
  const KEYS = ['hue', 'sat', 'strength', 'transmission', 'glare', 'halo', 'depth'];
  const hover = window.matchMedia('(hover: hover) and (pointer: fine)');
  let pinned = false;
  let timer = 0;
  const prefix = side.dataset.cookiePrefix || 'report_';
  const mslider = side.querySelector('[data-material-slider]');
  const mkeys = mslider ? (mslider.dataset.keys || '').split(',') : [];
  const mlabels = mslider ? (mslider.dataset.labels || '').split('|') : [];
  const mcss = {};
  if (mslider) {
    for (const pair of (mslider.dataset.css || '').split(';')) {
      const i = pair.indexOf('=');
      if (i > 0) mcss[pair.slice(0, i)] = pair.slice(i + 1);
    }
  }

  const field = (k) => side.querySelector('[data-aqua="' + k + '"]');
  const value = (k) => {
    const v = Number(field(k) && field(k).value);
    return Number.isFinite(v) ? Math.round(v) : 0;
  };
  const sheet = window.matchMedia('(max-width: 700px), (hover: none)');
  function place() {
    side.style.left = side.style.top = '';
    if (sheet.matches) return; // a bottom sheet on touch and narrow screens (CSS)
    // Beside the menu panel: left of it when the menu sits at the right edge, else right.
    const panel = (entry.closest('.prefs-panel') || entry).getBoundingClientRect();
    const w = side.offsetWidth;
    const gap = 12;
    let left = panel.left - w - gap >= 8 ? panel.left - w - gap : panel.right + gap;
    left = Math.max(8, Math.min(left, window.innerWidth - w - 8));
    const top = Math.max(8, Math.min(panel.top, window.innerHeight - side.offsetHeight - 8));
    side.style.left = left + 'px';
    side.style.top = top + 'px';
  }
  function open() {
    clearTimeout(timer);
    if (!side.hidden) return;
    side.hidden = false;
    eye.setAttribute('aria-expanded', 'true');
    place();
  }
  function close(focusEye) {
    clearTimeout(timer);
    pinned = false;
    side.hidden = true;
    eye.setAttribute('aria-expanded', 'false');
    eye.classList.remove('ap-pinned');
    if (focusEye) eye.focus();
  }
  function later() {
    clearTimeout(timer);
    if (pinned) return;
    timer = setTimeout(() => {
      if (!pinned && !inside(document.activeElement)) close(false);
    }, 250);
  }
  function cookie(name, val) {
    document.cookie = name + '=' + val + '; Path=/; SameSite=Lax';
  }
  function linkSheet(href) {
    if (!href) return;
    for (const l of document.querySelectorAll('link[rel="stylesheet"]')) {
      if (l.getAttribute('href') === href) return;
    }
    const l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = href;
    document.head.appendChild(l);
  }
  function setMaterial(key) {
    if (!key) return;
    linkSheet(mcss[key]);
    root.setAttribute('data-material', key);
    cookie(prefix + 'material', key);
    side.dataset.aquaMaterial = key === 'aqua-glass' ? 'on' : 'off';
    const i = mkeys.indexOf(key);
    if (mslider && i >= 0) {
      mslider.value = String(i);
      const out = side.querySelector('[data-material-label]');
      if (out) out.textContent = mlabels[i] || key;
    }
  }
  function apply() {
    for (const k of KEYS) {
      const v = value(k);
      root.style.setProperty('--aqua-' + k, k === 'hue' ? String(v) : (v / 100).toFixed(2));
      side.style.setProperty('--aqua-' + k, k === 'hue' ? String(v) : (v / 100).toFixed(2));
    }
    for (const out of side.querySelectorAll('output[for]:not([data-material-label])')) {
      const input = document.getElementById(out.getAttribute('for'));
      if (input) out.textContent = input.value + (out.dataset.unit || '');
    }
    if (root.getAttribute('data-material') !== 'aqua-glass') setMaterial('aqua-glass');
    cookie(prefix + 'aqua', KEYS.map(value).join('.'));
    // the generated optics follow the new parameters
    window.dispatchEvent(new Event('aqua:change'));
  }

  const focusables = () => [...side.querySelectorAll('input, button, summary, [tabindex]:not([tabindex="-1"])')]
    .filter((el) => !el.disabled && el.offsetParent !== null);
  function focusFirst() { const f = focusables(); if (f.length) f[0].focus(); }
  // The element after the eye in the page's own tab order (the sidecar lives elsewhere).
  function afterEye() {
    const all = [...document.querySelectorAll('a[href], button, input, select, textarea, summary, [tabindex]:not([tabindex="-1"])')]
      .filter((el) => !side.contains(el) && !el.disabled && el.offsetParent !== null);
    return all[all.indexOf(eye) + 1];
  }
  function toggle(byKeyboard) {
    if (side.hidden) { open(); pinned = true; }
    else if (!pinned) { pinned = true; }
    else { close(false); return; }
    eye.classList.toggle('ap-pinned', pinned);
    if (byKeyboard || !hover.matches) focusFirst();
  }
  // e.detail is 0 for a click synthesised from Enter: keyboard use moves focus inside.
  eye.addEventListener('click', (e) => { e.preventDefault(); toggle(e.detail === 0); });
  eye.addEventListener('keydown', (e) => {
    if (e.key === ' ') { e.preventDefault(); toggle(true); }
    else if (e.key === 'Tab' && !e.shiftKey && !side.hidden) { e.preventDefault(); focusFirst(); }
  });
  // Tab order inside the sidecar: it continues from the eye and returns after it.
  side.addEventListener('keydown', (e) => {
    if (e.key !== 'Tab') return;
    const f = focusables();
    if (!f.length) return;
    if (e.shiftKey && document.activeElement === f[0]) { e.preventDefault(); eye.focus(); }
    else if (!e.shiftKey && document.activeElement === f[f.length - 1]) {
      e.preventDefault();
      const next = afterEye();
      if (!pinned) close(false);
      (next || eye).focus();
    }
  });
  for (const el of [entry, side]) {
    el.addEventListener('mouseenter', () => { if (hover.matches) open(); });
    el.addEventListener('mouseleave', () => { if (hover.matches) later(); });
    el.addEventListener('focusout', () => setTimeout(() => {
      if (!pinned && !inside(document.activeElement)) close(false);
    }, 0));
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !side.hidden) { e.preventDefault(); e.stopPropagation(); close(true); }
    });
  }
  side.querySelector('.ap-close').addEventListener('click', () => close(true));
  side.addEventListener('input', (e) => {
    const t = e.target;
    if (t === mslider) {
      const key = mkeys[Number(t.value)];
      setMaterial(key);
      // the viewer's light belongs to Aqua Glass: carry it over when switching to it
      if (key === 'aqua-glass') apply();
      return;
    }
    if (t.name === 'ap-amb') {
      field('hue').value = t.dataset.hue;
      field('sat').value = t.dataset.sat;
    } else if (t.dataset.aqua === 'hue') {
      for (const r of side.querySelectorAll('input[name="ap-amb"]')) r.checked = false;
      if (Number(field('sat').value) === 0) field('sat').value = '75';
    }
    apply();
  });
  side.querySelector('.ap-reset').addEventListener('click', (e) => {
    const d = e.currentTarget.dataset.default.split('.');
    KEYS.forEach((k, i) => { if (field(k)) field(k).value = d[i]; });
    for (const r of side.querySelectorAll('input[name="ap-amb"]')) r.checked = r.dataset.hue === d[0] && r.dataset.sat === d[1];
    apply();
  });
  if (menu) menu.addEventListener('toggle', () => { if (!menu.open) close(false); });
  window.addEventListener('resize', () => { if (!side.hidden) place(); });
  side.addEventListener('toggle', () => place(), true); // Expert opens: keep it on screen
})();
