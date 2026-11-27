/* a2ui containers: one addressed content surface, shown as a page, in a pane or in a
   window (a2ui_layout::Container). The parent owns behaviour: window lifecycle, focus,
   dragging, the taskbar and the shared assets. The content owns its information and its
   authorized actions, delivered as links and forms.

   Loading is htmx's job, not this script's. A container body is an htmx region: every
   link and form inside it is delivered back into it, with the request header
   `A2-Container: pane|window`. The server answers from the same canonical URL, with
   the same authorization and scope, and wraps nothing. This script only creates the
   containers and checks that the answer is a fragment for the container it asked for.

   Markup the parent provides:
   - `[data-a2-desk]`  the desktop area windows open in, `[data-a2-task]` an optional taskbar;
   - `[data-a2-pane="name"]`  a pane, e.g. the second plane of `.a2-duo`;
   - an opener: `<a href="..." data-a2-open="window|pane|embed">`, with optional
     `data-a2-src` (the content URL when `href` is the page to fall back to),
     `data-a2-title`, `data-a2-id` (one window per id) and `data-a2-pane="name"`.
   `embed` opens an iframe window, for genuine embedding only: another origin, or a
   document that must stay isolated. A window opener on another origin always embeds:
   fragments come from this origin only. Without this script (or without htmx) every opener
   that is a link stays a link to the page. */
(function () {
  'use strict';
  var HEADER = 'A2-Container';
  var htmx = window.htmx;
  if (!htmx) return;
  var z = 10, wins = {};

  function desk() { return document.querySelector('[data-a2-desk]'); }
  function task() { return document.querySelector('[data-a2-task]'); }

  /* A container body is an htmx region asking for its own container. */
  function prime(body, kind) {
    body.setAttribute('data-a2-container', kind);
    body.setAttribute('hx-boost', 'true');
    body.setAttribute('hx-target', 'this');
    body.setAttribute('hx-swap', 'innerHTML');
    body.setAttribute('hx-push-url', 'false');
    body.setAttribute('hx-headers', JSON.stringify({ 'A2-Container': kind }));
    htmx.process(body);
  }
  function load(body, url) {
    body.setAttribute('data-a2-src', url);
    var h = {}; h[HEADER] = body.getAttribute('data-a2-container');
    return htmx.ajax('GET', url, { source: body, target: body, swap: 'innerHTML', headers: h });
  }

  /* Only a fragment for the container that asked is shown in it. Anything else (a
     sign-in page reached by a redirect, a proxy's error page, a full page) leaves the
     container and is shown as a page. A server-rendered error fragment is shown. */
  document.addEventListener('htmx:beforeSwap', function (e) {
    var t = e.detail.target, body = t && t.closest && t.closest('[data-a2-container]');
    if (!body) return;
    var xhr = e.detail.xhr, want = body.getAttribute('data-a2-container');
    if (xhr.getResponseHeader(HEADER) === want) {
      e.detail.shouldSwap = true; e.detail.isError = false;
      return;
    }
    e.detail.shouldSwap = false;
    if (xhr.status >= 400 && xhr.status !== 401) {
      body.textContent = 'Could not load this view (' + xhr.status + ').';
      return;
    }
    window.location.assign(xhr.responseURL || body.getAttribute('data-a2-src'));
  });

  /* After a swap the container follows its content: the address, the title. */
  document.addEventListener('htmx:afterSwap', function (e) {
    var body = e.detail.target;
    if (!body || !body.hasAttribute || !body.hasAttribute('data-a2-container')) return;
    var url = e.detail.xhr && e.detail.xhr.responseURL;
    if (url) body.setAttribute('data-a2-src', url);
    var root = body.querySelector('[data-a2-content]');
    var title = root && root.getAttribute('data-a2-title');
    var w = winOf(body);
    if (w) {
      if (title) setTitle(w, title);
      if (url) w.page.href = url;
    }
  });

  function winOf(node) {
    var el = node.closest && node.closest('.a2-window');
    if (!el) return null;
    for (var k in wins) if (wins[k].el === el) return wins[k];
    return null;
  }
  function setTitle(w, title) {
    w.title.textContent = title; w.el.setAttribute('aria-label', title);
    if (w.btn) w.btn.textContent = title;
  }
  function focus(w) {
    w.el.classList.remove('a2-min'); w.el.style.zIndex = ++z;
    Object.keys(wins).forEach(function (k) { if (wins[k].btn) wins[k].btn.classList.toggle('on', wins[k] === w); });
  }
  function minimise(w) { w.el.classList.add('a2-min'); if (w.btn) w.btn.classList.remove('on'); }
  function close(id) {
    var w = wins[id]; if (!w) return;
    w.el.remove(); if (w.btn) w.btn.remove(); delete wins[id];
  }

  function button(text, title) {
    var b = document.createElement('button'); b.type = 'button'; b.textContent = text; b.title = title;
    return b;
  }

  function openWindow(id, title, src, embed) {
    var d = desk();
    if (!d) { window.location.assign(src); return; }
    if (wins[id]) { focus(wins[id]); return; }
    var n = Object.keys(wins).length, dw = d.clientWidth, dh = d.clientHeight;
    var ww = Math.min(960, Math.max(Math.min(360, dw), dw - 80));
    var wh = Math.min(620, Math.max(Math.min(240, dh), dh - 60));
    var el = document.createElement('section'); el.className = 'a2-window';
    el.style.width = ww + 'px'; el.style.height = wh + 'px';
    el.style.left = Math.max(0, Math.min(40 + 28 * n, dw - ww)) + 'px';
    el.style.top = Math.max(0, Math.min(30 + 24 * n, dh - wh)) + 'px';
    el.setAttribute('role', 'dialog'); el.setAttribute('aria-label', title);
    var bar = document.createElement('div'); bar.className = 'a2-window-bar';
    var t = document.createElement('b'); t.textContent = title; bar.appendChild(t);
    var page = document.createElement('a'); page.href = src; page.textContent = '↗'; page.title = 'Open as page';
    bar.appendChild(page);
    var dock = null;
    if (!embed && document.querySelector('[data-a2-pane]')) {
      dock = button('⇥', 'Show beside'); bar.appendChild(dock);
    }
    var mn = button('–', 'Minimise'), cl = button('×', 'Close');
    bar.appendChild(mn); bar.appendChild(cl);
    var body = document.createElement('div'); body.className = 'a2-window-body';
    var frame = null;
    if (embed) {
      frame = document.createElement('iframe'); frame.src = src; frame.title = title;
      frame.setAttribute('referrerpolicy', 'same-origin');
      body.appendChild(frame);
    }
    el.appendChild(bar); el.appendChild(body); d.appendChild(el);
    var w = wins[id] = { el: el, title: t, page: page, frame: frame, btn: null };
    var tb = task();
    if (tb) {
      w.btn = button(title, title);
      tb.insertBefore(w.btn, tb.querySelector('.sp'));
      w.btn.onclick = function () {
        if (w.btn.classList.contains('on') && !el.classList.contains('a2-min')) minimise(w); else focus(w);
      };
    }
    mn.onclick = function () { minimise(w); };
    cl.onclick = function () { close(id); };
    if (dock) dock.onclick = function () {
      var p = document.querySelector('[data-a2-pane]');
      toPane(p.getAttribute('data-a2-pane'), body.getAttribute('data-a2-src') || src, null);
      close(id);
    };
    el.addEventListener('pointerdown', function () { focus(w); });
    el.addEventListener('focusin', function () { focus(w); });
    drag(w, bar);
    if (!embed) { prime(body, 'window'); load(body, src); }
    focus(w);
  }

  /* A window moves by its title bar. A drag ends on release, on cancel and when the
     pointer capture is lost. Only an embedded frame needs its pointer events held while
     the window moves, because events over a frame go to the other document. */
  function drag(w, bar) {
    bar.addEventListener('pointerdown', function (e) {
      if (e.target.closest('button, a')) return;
      var d = desk(), el = w.el, sx = e.clientX, sy = e.clientY, ox = el.offsetLeft, oy = el.offsetTop;
      if (w.frame) w.frame.style.pointerEvents = 'none';
      bar.setPointerCapture(e.pointerId);
      function mv(ev) {
        var maxX = Math.max(0, d.clientWidth - el.offsetWidth), maxY = Math.max(0, d.clientHeight - el.offsetHeight);
        el.style.left = Math.min(maxX, Math.max(0, ox + ev.clientX - sx)) + 'px';
        el.style.top = Math.min(maxY, Math.max(0, oy + ev.clientY - sy)) + 'px';
      }
      function end() {
        if (w.frame) w.frame.style.pointerEvents = '';
        bar.removeEventListener('pointermove', mv);
        ['pointerup', 'pointercancel', 'lostpointercapture'].forEach(function (k) { bar.removeEventListener(k, end); });
      }
      bar.addEventListener('pointermove', mv);
      ['pointerup', 'pointercancel', 'lostpointercapture'].forEach(function (k) { bar.addEventListener(k, end); });
    });
  }

  /* An embedded frame keeps its events; focusing it blurs this window, which is the one
     signal that does not reach into the other document. */
  window.addEventListener('blur', function () {
    setTimeout(function () {
      var f = document.activeElement;
      if (f && f.tagName === 'IFRAME') { var w = winOf(f); if (w) focus(w); }
    }, 0);
  });

  function sameOrigin(url) {
    try { return new URL(url, window.location.href).origin === window.location.origin; } catch (e) { return false; }
  }

  function toPane(name, src, pageHref) {
    var p = document.querySelector('[data-a2-pane="' + (name || '') + '"]') || (!name && document.querySelector('[data-a2-pane]'));
    if (!p) { window.location.assign(pageHref || src); return; }
    if (p.getAttribute('data-a2-container') !== 'pane') prime(p, 'pane');
    load(p, src);
    if (pageHref && pageHref !== window.location.pathname + window.location.search) {
      history.pushState({ a2: 1 }, '', pageHref);
    }
  }
  /* The pane's page address went into the history; going back renders that page. */
  window.addEventListener('popstate', function (e) { if (e.state && e.state.a2) window.location.reload(); });

  /* Openers. Capture phase, so an opener inside a container is not delivered back into
     that container by its htmx region. A modified click is left to the browser. */
  document.addEventListener('click', function (e) {
    var a = e.target.closest && e.target.closest('[data-a2-open]');
    if (!a) return;
    if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
    var how = a.getAttribute('data-a2-open');
    var href = a.getAttribute('href');
    var src = a.getAttribute('data-a2-src') || href;
    if (!src) return;
    e.preventDefault(); e.stopPropagation();
    var title = a.getAttribute('data-a2-title') || a.textContent.trim();
    /* A fragment comes from this origin only (htmx refuses other origins too). Another
       origin is another application: it embeds, or opens as its own page. */
    var local = sameOrigin(src);
    if (how === 'pane') { if (local) toPane(a.getAttribute('data-a2-pane'), src, href); else window.location.assign(href || src); }
    else openWindow(a.getAttribute('data-a2-id') || src, title, src, how === 'embed' || !local);
  }, true);
})();
