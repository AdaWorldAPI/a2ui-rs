# Railway service: the installable paint demo (`Dockerfile.pwa.railway`)

Steps to create a Railway service that serves `a2ui-paint-web` with the optional
`pwa` feature (#57). It is a separate service: the existing paint service
(`Dockerfile.railway`) and the solid service (`Dockerfile.solid.railway`) are not
touched.

The demo serves one synthetic surface and no user data, so it needs no sign-in.

## 1. Create the service

1. In the Railway project that holds the existing a2ui services (or a new
   project), add a service from the GitHub repository `AdaWorldAPI/a2ui-rs`,
   branch `main` (after #57 is merged; before that, the PR branch).
2. Service variables:

   | Variable | Value | Why |
   |---|---|---|
   | `RAILWAY_DOCKERFILE_PATH` | `Dockerfile.pwa.railway` | the repo has three Dockerfiles; this picks the PWA one |

   Do **not** set `PORT`: Railway injects it and the binary reads it.
3. Settings → Deploy → healthcheck path: `/health`.
4. Settings → Networking → generate a public domain. Installing a PWA needs
   HTTPS, which the Railway domain provides.
5. No volume, no database, no secrets. The build clones public git
   dependencies only.

## 2. Check the deployment

| Check | Expected |
|---|---|
| `GET /health` | `ok` and a last line `pwa: manifest + shell-only service worker` |
| `GET /manifest.webmanifest` | JSON, `"display":"standalone"`, scope `/` |
| `GET /sw.js` | JavaScript, header `cache-control: no-cache` |
| `GET /icons/192.png`, `/icons/512.png` | PNG images of that size |
| Chrome or Edge on the domain | the address bar offers "Install"; DevTools → Application → Manifest shows no errors and the service worker is "activated" |
| install, then open from the OS app list | opens in its own window (standalone) |
| DevTools → Network → Offline, reload | the page loads from the cache; clicking the surface fails visibly (the live `/hit` is never cached) |
| the existing paint service | unchanged: its `/manifest.webmanifest` returns 404 |

## 3. What this is not

Slice P2 of `docs/MORPHEUS-PWA-APPLICATION-DESKTOP.md` only, for this demo's own
origin. It installs, launches or wraps no other site's app, and it adds no
sign-in (P3 of that document).
