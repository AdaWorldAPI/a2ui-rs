# Morpheus PWA Application Desktop: three launch modes (v1)

**Status:** PROPOSAL / documentation only, not implemented or ratified  
**Date:** 2026-11-22  
**Owner/scope:** Morpheus consumer of AdaWorldAPI/a2ui-rs  
**Intent:** A Microsoft 365 app desktop without a default Windows, RDS, ChromeOS, Kasm, or server-side Chromium dependency.
**Implementation status (2026-11-26):** P0, P1 and P2 run as a demo in `a2ui-paint-web` behind the `pwa` feature (`Dockerfile.pwa.railway`, served at pwa.msgraph.de). `/` opens `/desktop`: a desktop with app rail, tiles, internal windows for this server's own pages and a taskbar; the app list is projected per role on the server (`?role=guest` is a fixture, not access control); Outlook, Teams, Word, Excel, OneDrive and SharePoint are plain links to allowlisted HTTPS hosts, opened in the click. The paint demo moved to `/paint`. The manifest is "Morpheus Desktop" with `start_url` `/desktop`. An optional *Physikalisches Glas* material renders the tiles through the optics engine's maps (Chromium). P3 (identity) and P4 (offline fixtures) are not implemented.

## 0. Decision in one paragraph

Build **one addressed A2UI desktop surface** for three user experiences: **(1) Web**: a pseudo-desktop in a browser tab; **(2) Direct launch**: authorized app icons open the real Microsoft cloud apps using HTTPS URLs, like a link; **(3) Installed**: the same Morpheus site is installable as a standalone PWA, with either ZITADEL-authenticated cloud operation or a strictly limited local/offline shell. The *Morpheus* PWA owns only its own origin. Outlook, Teams, Word, Excel, OneDrive and SharePoint remain independent Microsoft web apps. This is an **application broker**, not an implementation of Microsoft Office, not a WebView engine, and not a claim that Microsoft apps can be embedded in same-tab windows.

### Non-goals

- Do not create six new wrappers that purport to own Microsoft's PWAs or their manifests.
- Do not bypass CSP, X-Frame-Options, same-origin policy, authentication, conditional access or browser popup rules.
- Do not run ChromeOS, Chrome/Chromium, a VDI broker or desktop-streaming infrastructure on Railway for the basic POC.
- Do not render Microsoft 365 via GPU pixels, scrape its DOM or reproduce an Outlook/Office API with a lookalike UI.
- Do not change OGAR's charter, introduce a second A2UI widget vocabulary, or put JavaScript handlers on server-authored surfaces.
- Do not present this proposal as a shipped feature, production SSO, or a promise of complete offline Microsoft 365.

## 1. Three modes, one implementation

| Mode | What the user sees | Where Microsoft 365 executes | What we control | Constraint |
| --- | --- | --- | --- | --- |
| **1. Browser-in-browser (shell)** | A2UI pseudo-desktop as a normal tab, with taskbar/tiles/windows for **our** surfaces | On the user's browser, but M365 opens separately | A2UI DOM/canvas, internal window state, authorized launch actions | A webpage is **not** a nested browser engine; cross-origin Office pages cannot be arbitrarily painted into virtual windows |
| **2. Direct / download-like launcher** | Click Outlook/Excel/Teams icon; browser opens the real cloud app in another tab/window | Same local browser | Allowed destination, click action, app metadata and fallback link | Popup/window placement, PWA selection and focusing remain browser/OS-owned |
| **3. Installed Morpheus PWA** | Morpheus is an OS app icon and a standalone app window; launcher stays the same | Microsoft apps still open in browser-managed tabs/windows or as separately installed Microsoft PWAs | Morpheus's own manifest, scope, shell state, local cache, launch routing | Morpheus cannot install/control another origin's PWA or move its UI into Morpheus's standalone window |

**Terminology warning:** the phrase *browser in browser* means the **visual pseudo-desktop** in mode 1. A literal Chrome tab *inside* an A2UI window requires an actual browser engine and remote streaming (a separate **future** adapter); WASM, WebGL and HTML iframe do not create such an engine.

### Concrete app example

- Morpheus Web is hosted at an HTTPS origin by a Rust service on Railway.
- Its app shelf contains **Outlook**, **Teams**, **Word**, **Excel**, **OneDrive**, **SharePoint**, and optionally first-party A2UI/graph apps.
- A user clicks Outlook: on a *direct user gesture* the browser opens the approved Outlook Web destination, not a remote Windows VM.
- M365 sign-in and the available app content are determined by the user's **Microsoft Entra** session, licenses and conditional-access policies.
- The Morpheus tab or installed standalone PWA remains the launcher and workflow surface. This is **not** a guarantee that Microsoft opens inside that PWA window.

## 2. Deployment and execution boundaries

~~~mermaid
flowchart TB
    User["User browser / installed Morpheus PWA"]
    Shell["a2ui-wasm + a2ui-paint shell / taskbar"]
    Backend["Rust broker on Railway"]
    Zitadel["ZITADEL OIDC"]
    Registry["Authorized app registry + role projection"]
    MS["Microsoft 365 web applications / Entra"]
    Cache["Browser-local IndexedDB / SW cache"]

    User --> Shell
    Shell <--> Backend
    Backend <--> Zitadel
    Backend --> Registry
    Shell -->|"direct navigation, user gesture"| MS
    Shell <--> Cache
~~~

**Railway serves** static shell/manifest/WASM assets and optional Rust endpoints for authenticated session, app registry and authorization. **The client** owns DOM layout, WebGL2/WebGPU visualization where useful, app-icon clicks and the browser install request. Microsoft supplies its own applications and web compute. No Chromium process runs on Railway in this baseline.

WebGL2/WASM are useful for A2UI rendering/graph visualization, **not required for opening Microsoft links**. Reuse the existing single-renderer/session rule and UX zone budgets in docs/UX-ZONES-AND-BUDGETS.md; do not mount a GPU context for each app tile.

## 3. A2UI contract: address actions, not application-specific widgets

This proposal is a *consumer/adapter extension*, not a new render protocol.

| Existing seam | Reuse | Proposed work (NOT YET SHIPPED) |
| --- | --- | --- |
| a2ui-server DesktopSession | Session, role-projected ClassView, sealed NodeDelta, ordinal ActionInvoke, Klickweg edges | Project allowed launcher entries and their permitted actions; explicitly deny unapproved targets |
| a2ui-wasm | Resolve ClassView/ObjectSlot nesting and actions in the client | Surface launcher metadata and installation status to Morpheus's thin browser adapter |
| a2ui-paint / a2ui-layout | Desktop visual projection, zones, templates and hit testing | Shell skin/tiles/taskbar and local focus state for **first-party** windows without creating a new widget enum |
| Morpheus consumer (potentially separate project) | Site manifest, app URLs, Zitadel wiring and client browser APIs | An allowlisted external-link launch adapter; local shell persistence and install UX |

The canonical nested shape remains **desktop -> window -> region -> widget** through ObjectSlot / ClassView, rather than a second window/widget vocabulary. **T1:** template skins, not a new closed widget language. **T2:** server-issued ActionDef ordinals, not embedded onClick lambdas in the surface. **T3:** no JSON/serde in the hot A2UI delta path; browser platform metadata is a membrane/configuration concern.

### Illustrative app-registry membrane (NOT an existing API)

~~~yaml
apps:
  outlook:
    label: Outlook
    url: https://outlook.office.com/mail/
    launch_adapter: external_https
    requires: [m365]
  teams:
    label: Teams
    url: https://teams.microsoft.com/
    launch_adapter: external_https
    requires: [m365]
  sharepoint:
    label: SharePoint
    url: https://contoso.sharepoint.com/
    launch_adapter: external_https
    requires: [m365]
  morpheus_graph:
    label: Context Graph
    launch_adapter: internal_surface
    target_class_view: graph
~~~

**Domain/tenant hostnames above are examples**, never a tenant discovery rule. In production, pin the allowed HTTPS targets server-side, do not accept arbitrary URLs from user-editable metadata or graph responses, and do not confuse a link being visible with the user having Microsoft permissions. Browser launch adapters are not extra A2UI widget types.

### The popup-gesture trap

Popup blockers may reject a new window **after** awaiting an ActionStream server round trip. For an approved external link, the client must already possess a role-projected, allowlisted and verified destination when the click is rendered. The browser-adapter invokes native navigation **synchronously within that click gesture**, then sends/records the corresponding ActionInvoke/Klickweg. If launch is blocked, render an explicit, safe normal hyperlink as fallback. Never accept a late arbitrary URL response and navigate to it without separate validation or user interaction.

This splits a browser-host capability from an A2UI semantic action; the *surface* still does not carry executable JS behavior. The implementation must define whether click acknowledgement/log failure is advisory and whether launches that require fresh authorization must instead have a new-user-gesture confirmation after authorization.

## 4. Installed PWA contract

An example **Morpheus-owned** manifest, not an Outlook/Teams manifest:

~~~json
{
  "id": "/desktop",
  "name": "Morpheus Desktop",
  "short_name": "Morpheus",
  "start_url": "/desktop",
  "scope": "/",
  "display": "standalone",
  "theme_color": "#111827",
  "background_color": "#111827",
  "icons": [
    {"src": "/icons/192.png", "sizes": "192x192", "type": "image/png"},
    {"src": "/icons/512.png", "sizes": "512x512", "type": "image/png"}
  ],
  "launch_handler": {"client_mode": "focus-existing"}
}
~~~

Host over HTTPS, serve the manifest from the same origin, add actual icon assets, and provide an ordinary web fallback on browsers without install affordances. The **Launch Handler API is browser-dependent**; focus-existing applies to **Morpheus launches**, not foreign M365 PWAs. A service worker may cache the **Morpheus shell only** for offline startup; it does not make Outlook/Teams offline or grant access to Microsoft cookies. Installation is user/browser/enterprise-policy controlled, not an automatic EXE download.

## 5. Identity and local execution

- **Cloud mode:** ZITADEL OIDC Authorization Code + PKCE for Morpheus, with session validation and **role/tenant projection server-side before NodeDelta leaves the server**. Do not put service-account secrets in WASM, local storage or an app registry. A secure BFF/session-cookie architecture is preferable where available.
- **Microsoft 365:** independent Entra authentication; SSO can work if the browser and tenants are configured appropriately, but **ZITADEL login does not automatically grant an Entra or Microsoft Graph session**. Respect CA, MFA, M365 licensing and browser-profile boundaries.
- **Local/headless mode:** define this narrowly as a browser-local, offline-capable **demo shell** with persisted layout and optional synthetic fixtures (IndexedDB/Cache Storage). It is **not** a privileged, trusted account for the Railway API and cannot claim authenticated Microsoft data. Use separately scoped local state, conspicuous demo mode, and deny protected operations while offline. A future native/headless local *agent* would be a different executable with its own threat model.
- **Privacy:** never log OAuth authorization codes, tokens, cookies, complete Outlook URLs with sensitive query strings, message bodies or private document URLs into Klickweg telemetry. Keep allowed app ID and minimal invocation timing/status instead.

## 6. What browser security will not permit

1. A normal page or installed PWA cannot turn third-party Microsoft pages into same-origin ObjectSlots. Office services may prohibit framing via CSP frame-ancestors/X-Frame-Options; same-origin policy prevents owning their DOM even where framing is permitted.
2. WebGL, WebGPU and WASM cannot bypass browser sandbox, origin, cookie, iframe or popup restrictions.
3. window.open can request a new window in response to a gesture, **not reliably select or dock an existing foreign PWA**, control its taskbar icon, follow its navigation, inspect its contents or force cross-origin focus. COOP may further sever the opener relationship.
4. A shortcut to an official web app is not equivalent to installing that app as a PWA. Morpheus's own manifest/launch_handler applies only to Morpheus.
5. In-browser Microsoft 365 capabilities differ from Office desktop COM/MAPI/legacy add-ins. A separate RDP/RemoteApp adapter may later cover those workloads.

**Optional future alternative, explicitly out of this POC:** render a real Chromium session on a Linux host, stream its pixels and input into an ObjectSlot for a literal nested browser. This has session RAM, streaming latency, media redirection, Entra CA and operational costs; implement only if an internal-window requirement is proven.

## 7. Implementation slices and falsifiable acceptance gates

| Slice | Deliverable | Independent gate |
| --- | --- | --- |
| **P0: shell** | Morpheus consumer uses existing A2UI surface for desktop/taskbar/app icons; first-party window moves/focus only | Deterministic fixture projects exactly the role's app IDs; denied app IDs are **absent** from client frame, never merely CSS-hidden |
| **P1: direct launch** | External HTTPS adapter for Outlook/Teams/Word/Excel/OneDrive/SharePoint | A click can synchronously open a preapproved target; popup-block test shows a safe normal link; attacker-supplied http/javascript/cross-tenant URL is refused |
| **P2: installed** | Same site, valid manifest/icons/HTTPS and install/fallback UX | Install on a supported Chrome/Edge target, relaunch Morpheus with standalone display, verify plain-tab mode remains functional |
| **P3: identity** | ZITADEL PKCE cloud session and server-projected app ACL; separate Entra redirect for M365 | Unauthenticated and cross-role/cross-tenant app requests return no privileged projection; a logged-in Morpheus user **without** Entra login is still prompted by Microsoft |
| **P4: local demo** | Cached shell + synthetic offline fixtures; no protected API calls | With network disabled, shell loads previously cached fixture but cannot show live/private M365 data or execute server operations |
| **P5: optional exploration** | Remote browser stream adapter only if actual embedded third-party windows become a requirement | Record latency/RAM/Conditional Access impacts for 1–5 sessions; no default deployment or licensing claim |

### Minimum POC acceptance scenario

On a normal desktop Chrome/Edge browser, visit the Railway-hosted Morpheus origin, see six M365 app icons and a first-party graph window, click Outlook to open the **real** Microsoft app, return to Morpheus, install Morpheus as a PWA, relaunch it from the OS shelf, and repeat. Confirm the difference between **external Microsoft browser window** and **internal A2UI first-party window** is visible and never disguised. Add one denied app and one popup-blocked launch probe. Do **not** claim Microsoft sign-in or an installed third-party PWA is under Morpheus's control.

### Open implementation choices

- Should the first Morpheus consumer live in a2ui-paint-web as a **demo only** or in a separate Morpheus repository, keeping a2ui-rs library-purpose clean?
- How should browser launch capabilities be represented to the client without a mutable URL field becoming a trusted authorization source?
- Which browser/device matrix (Chrome/Edge desktop, Android, Safari/iOS) is in scope for the first acceptance run?
- Is Zitadel needed for P0, or can a purely synthetic fixture validate the projection boundary first?

## 8. Source references and repo provenance

Repository:
- [a2ui-rs charter and T1/T2/T3](../CLAUDE.md)
- [DesktopSession orchestration](../crates/a2ui-server/src/desktop.rs)
- [UX zones, budgets and GPU-context rule](./UX-ZONES-AND-BUDGETS.md)
- [Existing WASM/WebGL2 browser build](./WASM-INTEGRATION.md)

Platform documentation (accessed 2026-11-22):
- [MDN: Making PWAs installable](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Guides/Making_PWAs_installable)
- [MDN: Standalone PWA](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/How_to/Create_a_standalone_app)
- [Chrome: Launch Handler API](https://developer.chrome.com/docs/web-platform/launch-handler)
- [ZITADEL: OIDC Authorization Code + PKCE](https://zitadel.com/docs/guides/integrate/login/oidc/login-users)
- [MDN: Same-origin policy](https://developer.mozilla.org/en-US/docs/Web/Security/Same-origin_policy)
- [MDN: CSP frame-ancestors](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors)
- [MDN: Window.open](https://developer.mozilla.org/en-US/docs/Web/API/Window/open)

**Classification:** this document is a proposed Morpheus consumer workflow, not an amendment to OGAR's authoritative screen-addressing charter and not evidence that any of the slices have shipped.
