//! The Morpheus desktop: a "Citrix lite" shell, slices P0 and P1 of
//! `docs/MORPHEUS-PWA-APPLICATION-DESKTOP.md`. Built only with `--features pwa`.
//!
//! - **P0, the shell.** `/desktop` renders a desktop with an app rail, tiles, internal
//!   windows for first-party surfaces (this server's own pages) and a taskbar. The app
//!   list is projected per role on the server: an app the role may not use is absent
//!   from the HTML, never hidden by CSS.
//! - **P1, direct launch.** Outlook, Teams, Word, Excel, OneDrive and SharePoint open
//!   the real Microsoft 365 web apps as ordinary links in the click itself, so no popup
//!   blocker intervenes and the link is its own fallback. Where the user installed a
//!   Microsoft PWA and the browser captures its links, that installed app opens; the
//!   browser decides, not this page. Each target is checked against [`ALLOWED_HOSTS`]
//!   before it is rendered.
//!
//! What it does not do: frame, paint or proxy a Microsoft page, own another origin's
//! PWA, or sign anyone in. Microsoft 365 asks for its own Entra login. The role is a
//! fixture (`?role=`) until slice P3 brings ZITADEL; it is not access control.
//!
//! A launch is recorded as a Klickweg count per app id (`POST /desktop/launch`). The
//! URL, query strings and anything about the user are never recorded.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use axum::{
    Router,
    extract::RawQuery,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};

/// Hosts an external launch may target. Nothing else is rendered as a launch link.
pub(crate) const ALLOWED_HOSTS: [&str; 3] = [
    "outlook.office.com",
    "teams.microsoft.com",
    "www.office.com",
];

/// Where an app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Launch {
    /// A Microsoft 365 web app, opened by the browser outside this page.
    External(&'static str),
    /// A page of this server, opened in an internal window.
    Internal(&'static str),
}

/// Roles of the fixture. Slice P3 replaces this with a server-side session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Staff,
    Guest,
}

impl Role {
    fn parse(raw: &str) -> Self {
        let role = raw
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "role")
            .map(|(_, v)| v);
        match role {
            Some("guest") => Self::Guest,
            _ => Self::Staff,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Staff => "staff",
            Self::Guest => "guest",
        }
    }
}

/// One entry of the app registry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct App {
    pub id: &'static str,
    pub label: &'static str,
    pub monogram: &'static str,
    pub hint: &'static str,
    pub launch: Launch,
    pub roles: &'static [Role],
}

const ALL: &[Role] = &[Role::Staff, Role::Guest];
const STAFF: &[Role] = &[Role::Staff];

/// The registry: the only source of launch targets.
pub(crate) const APPS: [App; 9] = [
    App {
        id: "outlook",
        label: "Outlook",
        monogram: "O",
        hint: "E-Mail und Kalender",
        launch: Launch::External("https://outlook.office.com/mail/"),
        roles: ALL,
    },
    App {
        id: "teams",
        label: "Teams",
        monogram: "T",
        hint: "Chat, Besprechungen",
        launch: Launch::External("https://teams.microsoft.com/"),
        roles: ALL,
    },
    App {
        id: "word",
        label: "Word",
        monogram: "W",
        hint: "Dokumente",
        launch: Launch::External("https://www.office.com/launch/word"),
        roles: STAFF,
    },
    App {
        id: "excel",
        label: "Excel",
        monogram: "X",
        hint: "Tabellen",
        launch: Launch::External("https://www.office.com/launch/excel"),
        roles: STAFF,
    },
    App {
        id: "onedrive",
        label: "OneDrive",
        monogram: "D",
        hint: "Eigene Dateien",
        launch: Launch::External("https://www.office.com/launch/onedrive"),
        roles: STAFF,
    },
    App {
        id: "sharepoint",
        label: "SharePoint",
        monogram: "S",
        hint: "Teamseiten",
        launch: Launch::External("https://www.office.com/launch/sharepoint"),
        roles: STAFF,
    },
    App {
        id: "paint",
        label: "A2UI Oberfläche",
        monogram: "A",
        hint: "Adressierte Oberfläche, ein Klick = eine Ordinal-Aktion",
        launch: Launch::Internal("/paint"),
        roles: ALL,
    },
    App {
        id: "hub",
        label: "Service Hub",
        monogram: "H",
        hint: "Kacheln aus Glas (Fixture)",
        launch: Launch::Internal("/lab/optics/hub"),
        roles: STAFF,
    },
    App {
        id: "lab",
        label: "Materiallabor",
        monogram: "L",
        hint: "Optische Oberflächen",
        launch: Launch::Internal("/lab/optics"),
        roles: STAFF,
    },
];

/// An external target passes only as `https://<allowed host>/...` with no credentials
/// and no port.
pub(crate) fn allowed_external(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host)
}

/// The apps a role may use, in registry order, with invalid external targets removed.
pub(crate) fn projected(role: Role) -> Vec<&'static App> {
    APPS.iter()
        .filter(|a| a.roles.contains(&role))
        .filter(|a| match a.launch {
            Launch::External(u) => allowed_external(u),
            Launch::Internal(p) => p.starts_with('/') && !p.starts_with("//"),
        })
        .collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One app as a launcher element: a link for external apps, a button for internal ones.
fn app_tag(a: &App, class: &str) -> String {
    let inner = format!(
        r#"<span class="mono">{m}</span><span class="txt"><b>{l}</b><small>{h}</small></span>"#,
        m = esc(a.monogram),
        l = esc(a.label),
        h = esc(a.hint)
    );
    match a.launch {
        Launch::External(u) => format!(
            r#"<a class="{class} ext" data-app="{id}" href="{u}" target="_blank" rel="noopener noreferrer" title="{l} öffnet sich in Microsoft 365 (eigenes Fenster)">{inner}<span class="out" aria-hidden="true">↗</span></a>"#,
            id = a.id,
            u = esc(u),
            l = esc(a.label)
        ),
        Launch::Internal(p) => format!(
            r#"<button type="button" class="{class} int" data-app="{id}" data-src="{p}" data-title="{l}">{inner}</button>"#,
            id = a.id,
            p = esc(p),
            l = esc(a.label)
        ),
    }
}

/// The desktop page for a role.
pub(crate) fn page(role: Role) -> String {
    let apps = projected(role);
    let tiles: String = apps.iter().map(|a| app_tag(a, "tile")).collect();
    let rail: String = apps.iter().map(|a| app_tag(a, "rail-app")).collect();
    PAGE.replace("{pwa_head}", crate::PWA_HEAD)
        .replace("{role}", role.key())
        .replace("{tiles}", &tiles)
        .replace("{rail}", &rail)
}

const PAGE: &str = r##"<!doctype html><html lang="de"><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Morpheus Desktop</title>{pwa_head}
<style>
*{box-sizing:border-box}
html,body{margin:0;height:100%}
body{font:14px/1.4 ui-sans-serif,system-ui,"Segoe UI",sans-serif;color:#1b2430;background:#e8ecf2;display:grid;grid-template-rows:44px 1fr 40px;grid-template-columns:64px 1fr;overflow:hidden}
.top{grid-column:1/3;display:flex;align-items:center;gap:14px;padding:0 14px;background:#1f2a3a;color:#e9eef6}
.top b{font-size:15px;letter-spacing:.02em}
.top .role{opacity:.7;font-size:12px}
.top .sp{flex:1}
.top button{font:inherit;color:inherit;background:rgba(255,255,255,.12);border:0;border-radius:6px;padding:5px 10px;cursor:pointer}
.rail{background:#f6f8fb;border-right:1px solid #d6dce5;display:flex;flex-direction:column;align-items:center;gap:6px;padding:8px 0;overflow:auto}
.rail-app{position:relative;display:grid;place-items:center;width:48px;height:48px;border-radius:10px;border:0;background:transparent;text-decoration:none;color:inherit;cursor:pointer}
.rail-app:hover,.rail-app:focus-visible{background:#e2e8f1}
.rail-app .txt,.rail-app .out{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}
.mono{flex-shrink:0;display:grid;place-items:center;width:32px;height:32px;border-radius:8px;font-weight:700;color:#fff;background:#4a5a72}
.int .mono{background:#2f6fd6}
.ext .mono{background:#4a5a72}
.desk{position:relative;overflow:hidden;padding:22px}
.desk h1{margin:0 0 4px;font-size:20px}
.desk p.lead{margin:0 0 18px;color:#4c5869;max-width:720px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:12px;max-width:1100px}
.tile{position:relative;display:flex;gap:12px;align-items:center;padding:14px;border-radius:12px;background:#fff;border:1px solid #d6dce5;text-decoration:none;color:inherit;font:inherit;text-align:left;cursor:pointer}
.tile:hover,.tile:focus-visible{border-color:#2f6fd6;box-shadow:0 2px 10px rgba(31,42,58,.12)}
.tile .txt{display:flex;flex-direction:column;min-width:0}
.tile small{color:#5b6677}
.tile .out{position:absolute;top:8px;right:10px;color:#5b6677}
.legend{margin-top:16px;color:#5b6677;font-size:12px}
.win{position:absolute;display:flex;flex-direction:column;background:#fff;border:1px solid #b9c2cf;border-radius:10px;box-shadow:0 12px 40px rgba(20,30,45,.25);overflow:hidden;min-width:320px;min-height:200px;resize:both}
.win .bar{display:flex;align-items:center;gap:8px;height:34px;padding:0 6px 0 12px;background:#eef2f7;border-bottom:1px solid #d6dce5;cursor:move;user-select:none}
.win .bar b{flex:1;font-size:13px}
.win .bar button{border:0;background:transparent;width:28px;height:26px;border-radius:6px;cursor:pointer;font-size:14px}
.win .bar button:hover{background:#dfe5ee}
.win iframe{flex:1;border:0;width:100%}
.win.min{display:none}
.task{grid-column:1/3;display:flex;align-items:center;gap:6px;padding:0 10px;background:#1f2a3a;color:#e9eef6;overflow-x:auto}
.task button{font:inherit;font-size:12px;color:inherit;background:rgba(255,255,255,.08);border:0;border-radius:6px;padding:5px 10px;cursor:pointer;white-space:nowrap}
.task button.on{background:rgba(255,255,255,.24)}
.task .sp{flex:1}
.task .clock{font-variant-numeric:tabular-nums;opacity:.85}
@media (max-width:640px){body{grid-template-columns:0 1fr}.rail{display:none}}
/* Physikalisches Glas: the ray-traced surface of a2ui_paint::optics::surface (clear card)
   as an SVG backdrop filter over the engine's maps, over contour rings so the refraction
   at the tile edges has lines to bend. Chromium applies url() backdrop filters; other
   browsers show the tile without refraction. */
body.glass .desk{background:repeating-radial-gradient(circle at 22% 38%,transparent 0 13px,rgba(24,33,43,.09) 13px 14px),repeating-radial-gradient(circle at 80% 70%,transparent 0 17px,rgba(24,33,43,.07) 17px 18px),linear-gradient(180deg,#eef1f5,#e6eaf0)}
body.glass .tile{background:transparent;border-color:transparent;isolation:isolate}
body.glass .tile:hover,body.glass .tile:focus-visible{border-color:rgba(47,111,214,.6)}
.tile .glass{position:absolute;left:-1px;top:-1px;border-radius:12px;z-index:-1;pointer-events:none;-webkit-backdrop-filter:url(#phys);backdrop-filter:url(#phys)}
.tile .glass .layer{position:absolute;inset:0;border-radius:inherit;background-size:100% 100%}
@media (prefers-reduced-transparency:reduce),(forced-colors:active){body.glass .tile{background:Canvas;border-color:CanvasText}.tile .glass{display:none}}
</style>
<body data-role="{role}">
<header class="top"><b>Morpheus Desktop</b><span class="role">Rolle: {role} (Fixture)</span><span class="sp"></span><label style="font-size:12px"><input type="checkbox" id="glass"> Physikalisches Glas</label><button type="button" id="install" hidden>Installieren</button></header>
<nav class="rail" aria-label="Apps">{rail}</nav>
<main class="desk" id="desk">
<h1>Arbeitsplatz</h1>
<p class="lead">Microsoft-365-Apps öffnen sich als echte Web-Apps von Microsoft (↗), in einem eigenen Fenster oder der installierten App. Eigene Oberflächen öffnen sich hier als Fenster.</p>
<div class="grid">{tiles}</div>
<p class="legend">↗ = Microsoft 365, eigene Anmeldung bei Microsoft. Blaue Kacheln = Fenster auf diesem Desktop.</p>
</main>
<footer class="task" id="task"><span class="sp"></span><span class="clock" id="clock"></span></footer>
<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>
<filter id="phys" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">
<feImage id="pmap" x="0" y="0" preserveAspectRatio="none" result="map"/>
<feDisplacementMap id="pdg" in="SourceGraphic" in2="map" xChannelSelector="R" yChannelSelector="G" result="direct"/>
<feGaussianBlur id="pfine" in="direct" stdDeviation="0" result="fine"/>
<feGaussianBlur id="pwide" in="direct" stdDeviation="0" result="wide"/>
<feComposite id="pmixDF" in="direct" in2="fine" operator="arithmetic" k1="0" k2="1" k3="0" k4="0" result="df"/>
<feComposite id="pmixW" in="df" in2="wide" operator="arithmetic" k1="0" k2="1" k3="0" k4="0"/>
</filter></defs></svg>
<script>
(function () {
  var desk = document.getElementById('desk'), task = document.getElementById('task');
  var spacer = task.querySelector('.sp'), z = 10, wins = {};
  function record(id) {
    try { navigator.sendBeacon('/desktop/launch?app=' + encodeURIComponent(id)); } catch (e) {}
  }
  function focus(w) {
    w.el.classList.remove('min'); w.el.style.zIndex = ++z;
    Object.keys(wins).forEach(function (k) { wins[k].btn.classList.toggle('on', wins[k] === w); });
  }
  function close(id) {
    var w = wins[id]; if (!w) return;
    w.el.remove(); w.btn.remove(); delete wins[id];
  }
  function open(id, title, src) {
    if (wins[id]) { focus(wins[id]); return; }
    var n = Object.keys(wins).length;
    var el = document.createElement('section'); el.className = 'win';
    el.style.left = (40 + 28 * n) + 'px'; el.style.top = (30 + 24 * n) + 'px';
    el.style.width = Math.min(900, desk.clientWidth - 80) + 'px';
    el.style.height = Math.min(600, desk.clientHeight - 60) + 'px';
    el.setAttribute('role', 'dialog'); el.setAttribute('aria-label', title);
    var bar = document.createElement('div'); bar.className = 'bar';
    var t = document.createElement('b'); t.textContent = title; bar.appendChild(t);
    var mn = document.createElement('button'); mn.type = 'button'; mn.textContent = '–'; mn.title = 'Minimieren';
    var cl = document.createElement('button'); cl.type = 'button'; cl.textContent = '×'; cl.title = 'Schließen';
    bar.appendChild(mn); bar.appendChild(cl);
    var fr = document.createElement('iframe'); fr.src = src; fr.title = title;
    el.appendChild(bar); el.appendChild(fr); desk.appendChild(el);
    var btn = document.createElement('button'); btn.type = 'button'; btn.textContent = title;
    task.insertBefore(btn, spacer);
    var w = wins[id] = { el: el, btn: btn };
    btn.onclick = function () { if (btn.classList.contains('on') && !el.classList.contains('min')) { el.classList.add('min'); btn.classList.remove('on'); } else focus(w); };
    mn.onclick = function () { el.classList.add('min'); btn.classList.remove('on'); };
    cl.onclick = function () { close(id); };
    el.addEventListener('pointerdown', function () { focus(w); });
    bar.addEventListener('pointerdown', function (e) {
      if (e.target.tagName === 'BUTTON') return;
      var sx = e.clientX, sy = e.clientY, ox = el.offsetLeft, oy = el.offsetTop;
      fr.style.pointerEvents = 'none'; bar.setPointerCapture(e.pointerId);
      function mv(ev) { el.style.left = Math.max(0, ox + ev.clientX - sx) + 'px'; el.style.top = Math.max(0, oy + ev.clientY - sy) + 'px'; }
      function up() { fr.style.pointerEvents = ''; bar.removeEventListener('pointermove', mv); bar.removeEventListener('pointerup', up); }
      bar.addEventListener('pointermove', mv); bar.addEventListener('pointerup', up);
    });
    focus(w);
  }
  document.querySelectorAll('[data-app]').forEach(function (a) {
    if (a.classList.contains('int')) {
      a.addEventListener('click', function () { open(a.dataset.app, a.dataset.title, a.dataset.src); record(a.dataset.app); });
    } else {
      // the browser follows the link in this same gesture; recording is advisory
      a.addEventListener('click', function () { record(a.dataset.app); });
    }
  });
  var clock = document.getElementById('clock');
  function tick() { clock.textContent = new Date().toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' }); }
  tick(); setInterval(tick, 15000);
  // Physikalisches Glas: every map comes from the optics engine; this page only builds
  // the map URLs for the measured tile size and reads the response headers.
  var gbox = document.getElementById('glass'), gsize = '', dpr = window.devicePixelRatio || 1;
  function blobUrl(u, apply) {
    fetch(u).then(function (r) { if (!r.ok) throw new Error(r.status); return r.blob().then(function (b) { apply(URL.createObjectURL(b), r.headers); }); }).catch(function () {});
  }
  function glassMaps() {
    var t = document.querySelector('.tile'); if (!t) return;
    var w = Math.round(t.offsetWidth), h = Math.round(t.offsetHeight), key = w + 'x' + h + '@' + dpr;
    if (key === gsize) return; gsize = key;
    var q = 'dpr=' + dpr + '&preset=clear-card&w=' + w + '&h=' + h + '&cornerpx=12';
    blobUrl('/lab/optics/surface/map?kind=disp&' + q, function (obj, hd) {
      var box = hd.get('x-a2ui-box').split('x').map(Number), m = document.getElementById('pmap');
      m.setAttribute('href', obj); m.setAttribute('width', box[0] / dpr); m.setAttribute('height', box[1] / dpr);
      document.getElementById('pdg').setAttribute('scale', +hd.get('x-a2ui-scale-g') / dpr);
      var wt = hd.get('x-a2ui-weights').split(' ').map(Number), sg = hd.get('x-a2ui-sigma').split(' ').map(Number);
      document.getElementById('pfine').setAttribute('stdDeviation', sg[0] / dpr);
      document.getElementById('pwide').setAttribute('stdDeviation', sg[1] / dpr);
      document.getElementById('pmixDF').setAttribute('k2', wt[0]); document.getElementById('pmixDF').setAttribute('k3', wt[1]);
      document.getElementById('pmixW').setAttribute('k3', wt[2]);
      var tn = hd.get('x-a2ui-tint').split(' ').map(Number);
      document.querySelectorAll('.tile .glass').forEach(function (g) {
        g.style.width = box[0] / dpr + 'px'; g.style.height = box[1] / dpr + 'px';
        g.style.background = 'rgba(' + tn[0] + ',' + tn[1] + ',' + tn[2] + ',' + tn[3] + ')';
      });
    });
    blobUrl('/lab/optics/surface/map?kind=spec&' + q, function (obj) { document.querySelectorAll('.tile .specL').forEach(function (l) { l.style.backgroundImage = 'url(' + obj + ')'; }); });
    blobUrl('/lab/optics/surface/map?kind=rim&' + q, function (obj) { document.querySelectorAll('.tile .rimL').forEach(function (l) { l.style.backgroundImage = 'url(' + obj + ')'; }); });
  }
  function setGlass(on) {
    document.body.classList.toggle('glass', on); gbox.checked = on;
    try { localStorage.setItem('morpheus.glass', on ? '1' : '0'); } catch (e) {}
    if (!on) return;
    document.querySelectorAll('.tile').forEach(function (t) {
      if (!t.querySelector('.glass')) t.insertAdjacentHTML('afterbegin', '<span class="glass"><span class="layer rimL"></span><span class="layer specL"></span></span>');
    });
    glassMaps();
  }
  var gq = new URLSearchParams(location.search).get('glass'), gs = null;
  try { gs = localStorage.getItem('morpheus.glass'); } catch (e) {}
  setGlass(gq ? gq === '1' : gs === '1');
  gbox.addEventListener('change', function () { setGlass(gbox.checked); });
  var rt = 0; addEventListener('resize', function () { clearTimeout(rt); rt = setTimeout(function () { if (gbox.checked) glassMaps(); }, 150); });
  var deferred = null, ib = document.getElementById('install');
  addEventListener('beforeinstallprompt', function (e) { e.preventDefault(); deferred = e; ib.hidden = false; });
  ib.onclick = function () { if (deferred) { deferred.prompt(); deferred = null; ib.hidden = true; } };
})();
</script>
</html>"##;

fn launches() -> &'static Mutex<BTreeMap<&'static str, u64>> {
    static L: OnceLock<Mutex<BTreeMap<&'static str, u64>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(BTreeMap::new()))
}

async fn desktop(RawQuery(raw): RawQuery) -> impl IntoResponse {
    Html(page(Role::parse(raw.as_deref().unwrap_or(""))))
}

/// Count one launch of a known app. Unknown ids are refused and nothing else is kept.
async fn launch(RawQuery(raw): RawQuery) -> impl IntoResponse {
    let raw = raw.unwrap_or_default();
    let id = raw
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "app")
        .map(|(_, v)| v)
        .unwrap_or("");
    match APPS.iter().find(|a| a.id == id) {
        Some(a) => {
            if let Ok(mut m) = launches().lock() {
                *m.entry(a.id).or_insert(0) += 1;
            }
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

/// The launch counts per app id, as plain text.
async fn launch_counts() -> impl IntoResponse {
    let body = launches()
        .lock()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {v}\n"))
                .collect::<String>()
        })
        .unwrap_or_default();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

/// The desktop routes. `/` opens the desktop; the paint demo moves to `/paint`.
pub(crate) fn routes(app: Router) -> Router {
    app.route("/", get(|| async { Redirect::temporary("/desktop") }))
        .route("/desktop", get(desktop))
        .route("/desktop/launch", post(launch))
        .route("/desktop/launches", get(launch_counts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(role: Role) -> Vec<&'static str> {
        projected(role).iter().map(|a| a.id).collect()
    }

    #[test]
    fn every_registry_target_is_valid() {
        for a in APPS {
            if let Launch::External(u) = a.launch {
                assert!(allowed_external(u), "{} -> {u}", a.id);
            }
        }
        assert_eq!(projected(Role::Staff).len(), APPS.len());
    }

    #[test]
    fn unsafe_targets_are_refused() {
        for bad in [
            "http://outlook.office.com/mail/",
            "javascript:alert(1)",
            "https://evil.example/",
            "https://outlook.office.com.evil.example/",
            "https://user@outlook.office.com/",
            "https://outlook.office.com:8443/",
            "//outlook.office.com/",
            "",
        ] {
            assert!(!allowed_external(bad), "{bad}");
        }
        assert!(allowed_external("https://teams.microsoft.com/"));
    }

    /// A denied app is absent from the page, not hidden.
    #[test]
    fn a_role_sees_exactly_its_apps() {
        assert_eq!(ids(Role::Guest), ["outlook", "teams", "paint"]);
        let guest = page(Role::Guest);
        for denied in ["word", "excel", "onedrive", "sharepoint", "hub", "lab"] {
            assert!(
                !guest.contains(&format!("data-app=\"{denied}\"")),
                "{denied}"
            );
        }
        assert!(!guest.contains("office.com/launch"));
        let staff = page(Role::Staff);
        for a in APPS {
            assert!(
                staff.contains(&format!("data-app=\"{}\"", a.id)),
                "{}",
                a.id
            );
        }
    }

    #[test]
    fn the_role_fixture_defaults_to_staff() {
        assert_eq!(Role::parse(""), Role::Staff);
        assert_eq!(Role::parse("role=guest"), Role::Guest);
        assert_eq!(Role::parse("x=1&role=nonsense"), Role::Staff);
    }

    /// Physikalisches Glas takes its maps from the optics engine and computes no optics
    /// of its own.
    #[test]
    fn the_glass_material_renders_the_engine() {
        let p = page(Role::Staff);
        assert!(p.contains("/lab/optics/surface/map?kind=disp&"));
        assert!(p.contains("backdrop-filter:url(#phys)"));
        assert!(p.contains(r#"<filter id="phys""#));
        for forbidden in ["Math.sqrt", "Math.hypot", "createImageData", "putImageData"] {
            assert!(!p.contains(forbidden), "{forbidden}");
        }
    }

    /// External apps are plain links opened in the click; internal ones open a window.
    #[test]
    fn external_apps_are_links_and_internal_apps_are_windows() {
        let p = page(Role::Staff);
        assert!(p.contains(
            r#"href="https://outlook.office.com/mail/" target="_blank" rel="noopener noreferrer""#
        ));
        assert!(p.contains(r#"data-src="/paint""#));
        assert!(!p.contains("window.open"));
        assert!(
            !p.contains("<iframe src=\"https"),
            "no Microsoft page is framed"
        );
    }
}
