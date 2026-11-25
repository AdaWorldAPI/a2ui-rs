// RPT-AQUA-GLASS-1: live preview for the Material Studio. Optional: without it the form still
// works (submit previews, "Übernehmen" applies). It only reads the studio's own inputs and
// writes numbers into CSS custom properties and the apply link; nothing is sent anywhere.
(function () {
  const form = document.querySelector('form.aqua-studio');
  if (!form) return;
  const frame = document.querySelector('iframe.ml-frame');
  const apply = form.querySelector('a[data-apply]');
  const sat = form.querySelector('input[name="pv_aqua_sat"]');
  const expertFlag = form.querySelector('input[name="pv_expert"]');
  const expert = form.querySelector('details.as-expert');
  const KEYS = ['hue', 'sat', 'strength', 'transmission', 'glare', 'halo', 'depth'];
  const num = (name) => {
    const el = name === 'sat' ? sat : form.querySelector('input[name="pv_aqua_' + name + '"]');
    const v = Number(el && el.value);
    return Number.isFinite(v) ? Math.round(v) : 0;
  };
  function vars(target) {
    if (!target) return;
    for (const k of KEYS) {
      const v = num(k);
      target.style.setProperty('--aqua-' + k, k === 'hue' ? String(v) : (v / 100).toFixed(2));
    }
  }
  function update() {
    for (const out of form.querySelectorAll('output[for]')) {
      const input = document.getElementById(out.getAttribute('for'));
      if (input) out.textContent = input.value + (out.dataset.unit || '');
    }
    vars(form);
    try {
      const doc = frame && frame.contentDocument;
      if (doc && doc.documentElement) vars(doc.documentElement);
    } catch (e) { /* the frame is same-origin; ignore if it is not ready */ }
    if (apply) {
      const key = KEYS.map(num).join('.');
      apply.href = apply.getAttribute('href').replace(/aqua=[0-9.]+/, 'aqua=' + key);
      if (frame) {
        // A reload of the frame keeps the current values.
        frame.dataset.src = frame.getAttribute('src').replace(/aqua=[0-9.]+/, 'aqua=' + key);
      }
    }
  }
  form.addEventListener('input', (e) => {
    const t = e.target;
    if (t.name === 'pv_aqua_amb') {
      const hue = form.querySelector('input[name="pv_aqua_hue"]');
      if (hue) hue.value = t.dataset.hue;
      sat.value = t.dataset.sat;
    } else if (t.name === 'pv_aqua_hue') {
      for (const r of form.querySelectorAll('input[name="pv_aqua_amb"]')) r.checked = false;
      if (Number(sat.value) === 0) sat.value = '75';
    }
    update();
  });
  if (expert && expertFlag) {
    expert.addEventListener('toggle', () => { expertFlag.disabled = !expert.open; });
  }
  if (frame) frame.addEventListener('load', update);
  update();
})();
