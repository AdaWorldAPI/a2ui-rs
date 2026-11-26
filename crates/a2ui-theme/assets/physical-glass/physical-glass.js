/* Physikalisches Glas: loads the optical engine's maps for each surface size and applies
   them as SVG backdrop filters. No optics are computed here: the page measures surfaces,
   builds map URLs and reads the response headers (x-a2ui-*), nothing else.
   Script attributes: data-map (map endpoint, default /optics/surface/map) and
   data-surfaces (selector of the glass surfaces, default the report-rs surfaces). */
(function () {
  'use strict';
  var root = document.documentElement;
  var script = document.currentScript;
  var base = (script && script.getAttribute('data-map')) || '/optics/surface/map';
  var SURF = (script && script.getAttribute('data-surfaces')) ||
    '.card, .wb-page, .wb-hint, .wb-inspector, .kpi, .a2-duo > section, .a2-duo > article, ' +
    '.shell[data-state="arrival"] nav.leftnav a, .shell[data-state="arrival"] nav.leftnav span';
  var MAX_BUCKETS = 24, NS = 'http://www.w3.org/2000/svg';
  var dpr = window.devicePixelRatio || 1, defs = null, buckets = {}, count = 0, timer = 0;

  function on() { return root.getAttribute('data-material') === 'physical-glass'; }
  function supported() {
    return !!(window.CSS && CSS.supports &&
      (CSS.supports('backdrop-filter', 'url(#a)') || CSS.supports('-webkit-backdrop-filter', 'url(#a)')));
  }
  function el(name, attrs) {
    var e = document.createElementNS(NS, name);
    Object.keys(attrs).forEach(function (k) { e.setAttribute(k, attrs[k]); });
    return e;
  }
  function ensureDefs() {
    if (defs) return defs;
    var svg = el('svg', { width: '0', height: '0', 'aria-hidden': 'true', style: 'position:absolute' });
    defs = el('defs', {}); svg.appendChild(defs); document.body.appendChild(svg);
    return defs;
  }
  function load(kind, q) {
    return fetch(base + '?kind=' + kind + '&' + q).then(function (r) {
      if (!r.ok) throw new Error(kind + ' ' + r.status);
      return r.blob().then(function (b) { return { url: URL.createObjectURL(b), h: r.headers }; });
    });
  }
  // one filter and one map set per (width, height, radius) bucket, 8 px steps
  function bucket(w, h, r) {
    var bw = Math.max(16, Math.round(w / 8) * 8), bh = Math.max(16, Math.round(h / 8) * 8);
    var br = Math.min(Math.round(r), Math.floor(Math.min(bw, bh) / 2));
    var key = bw + 'x' + bh + 'r' + br;
    if (buckets[key]) return buckets[key];
    if (count >= MAX_BUCKETS) return null;
    count++;
    var id = 'pg-' + key, b = buckets[key] = { id: id, ready: null, box: null, tint: '', spec: '', rim: '' };
    var f = el('filter', { id: id, x: '0', y: '0', width: '100%', height: '100%', 'color-interpolation-filters': 'sRGB' });
    var img = el('feImage', { x: '0', y: '0', preserveAspectRatio: 'none', result: 'map' });
    var dm = el('feDisplacementMap', { in: 'SourceGraphic', in2: 'map', xChannelSelector: 'R', yChannelSelector: 'G', result: 'direct' });
    var fine = el('feGaussianBlur', { in: 'direct', stdDeviation: '0', result: 'fine' });
    var wide = el('feGaussianBlur', { in: 'direct', stdDeviation: '0', result: 'wide' });
    var df = el('feComposite', { in: 'direct', in2: 'fine', operator: 'arithmetic', k1: '0', k2: '1', k3: '0', k4: '0', result: 'df' });
    var mw = el('feComposite', { in: 'df', in2: 'wide', operator: 'arithmetic', k1: '0', k2: '1', k3: '0', k4: '0' });
    [img, dm, fine, wide, df, mw].forEach(function (n) { f.appendChild(n); });
    ensureDefs().appendChild(f);
    var q = 'preset=clear-card&dpr=' + dpr + '&w=' + bw + '&h=' + bh + '&cornerpx=' + br;
    b.ready = Promise.all([load('disp', q), load('spec', q), load('rim', q)]).then(function (m) {
      var h = m[0].h, box = h.get('x-a2ui-box').split('x').map(Number);
      img.setAttribute('href', m[0].url);
      img.setAttribute('width', box[0] / dpr); img.setAttribute('height', box[1] / dpr);
      dm.setAttribute('scale', +h.get('x-a2ui-scale-g') / dpr);
      var wt = h.get('x-a2ui-weights').split(' ').map(Number), sg = h.get('x-a2ui-sigma').split(' ').map(Number);
      fine.setAttribute('stdDeviation', sg[0] / dpr); wide.setAttribute('stdDeviation', sg[1] / dpr);
      df.setAttribute('k2', wt[0]); df.setAttribute('k3', wt[1]); mw.setAttribute('k3', wt[2]);
      var t = h.get('x-a2ui-tint').split(' ').map(Number);
      b.box = [box[0] / dpr, box[1] / dpr];
      b.tint = 'rgba(' + t[0] + ',' + t[1] + ',' + t[2] + ',' + t[3] + ')';
      b.spec = 'url(' + m[1].url + ')'; b.rim = 'url(' + m[2].url + ')';
      return b;
    }).catch(function () { return null; });
    return b;
  }
  // The glass layer is its own element name, so no app rule for span/div/i reaches it
  // and the surface selector can never select the layer itself.
  function glassOf(s) {
    for (var i = 0; i < s.children.length; i++) if (s.children[i].localName === 'pg-glass') return s.children[i];
    var g = document.createElement('pg-glass');
    g.setAttribute('aria-hidden', 'true');
    g.appendChild(document.createElement('pg-layer'));
    g.appendChild(document.createElement('pg-layer'));
    s.insertBefore(g, s.firstChild);
    return g;
  }
  function apply() {
    if (!on() || !supported()) { root.removeAttribute('data-pg-optics'); return; }
    root.setAttribute('data-pg-optics', '');
    document.querySelectorAll(SURF).forEach(function (s) {
      if (s.closest('pg-glass')) return;
      var w = s.offsetWidth, h = s.offsetHeight;
      if (w < 16 || h < 16) return;
      var r = parseFloat(getComputedStyle(s).borderTopLeftRadius) || 0;
      var b = bucket(w, h, r);
      if (!b) return;
      var g = glassOf(s);
      b.ready.then(function (ok) {
        if (!ok) return;
        g.style.width = ok.box[0] + 'px'; g.style.height = ok.box[1] + 'px';
        g.style.background = ok.tint;
        g.style.backdropFilter = g.style.webkitBackdropFilter = 'url(#' + ok.id + ')';
        g.firstChild.style.backgroundImage = ok.rim; g.lastChild.style.backgroundImage = ok.spec;
        g.setAttribute('data-ready', '');
      });
    });
  }
  // The background behind the glass: one list, its labels here, the choice per browser.
  // Rosé is the default (no data-pg-bg); only a choice made in a picker is remembered, under
  // 'pg.background' ('pg.bg' was written on every visit and is ignored).
  var DEFAULT = 'rose';
  var BACKGROUNDS = [['rose', 'Rosé'], ['mint', 'Minze'], ['pearl', 'Perlmutt'], ['crystal', 'Kristall'],
    ['morning', 'Morgenlicht'], ['lagoon', 'Lagune'], ['night', 'Nacht'], ['deepsea', 'Tiefsee'], ['neon', 'Neon'],
    ['aurora', 'Polarlicht'], ['brushed', 'Gebürstetes Metall'], ['contour', 'Konturen']];
  function known(v) { return BACKGROUNDS.some(function (b) { return b[0] === v; }); }
  function setBackground(v, remember) {
    if (!known(v)) v = DEFAULT;
    if (v === DEFAULT) root.removeAttribute('data-pg-bg'); else root.setAttribute('data-pg-bg', v);
    if (remember) { try { localStorage.setItem('pg.background', v); } catch (e) {} }
    document.querySelectorAll('select[data-pg-bg-choice]').forEach(function (s) { s.value = v; });
  }
  function wireChoices() {
    document.querySelectorAll('select[data-pg-bg-choice]').forEach(function (s) {
      if (s.options.length) return;
      BACKGROUNDS.forEach(function (b) { var o = document.createElement('option'); o.value = b[0]; o.textContent = b[1]; s.appendChild(o); });
      s.addEventListener('change', function () { setBackground(s.value, true); });
    });
  }
  var saved = null;
  try { saved = localStorage.getItem('pg.background'); } catch (e) {}
  setBackground(saved || DEFAULT);
  window.physicalGlassBackground = function (v) { setBackground(v, true); };
  function later() { clearTimeout(timer); timer = setTimeout(apply, 120); }
  function start() {
    wireChoices(); setBackground(root.getAttribute('data-pg-bg') || DEFAULT);
    apply();
    new MutationObserver(later).observe(root, { attributes: true, attributeFilter: ['data-material'] });
    addEventListener('resize', later);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start); else start();
})();
