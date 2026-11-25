# ogar-weather: design note (experimental track, read-only investigation)

Status: HYPOTHESIS. Nothing here is built, and no classid is minted.

## 1. What already exists, and what is missing

**Can be reused (each verified by reading the file):**

- **Location: `ogar-osm` tiles, not floats.** `GEO_V3_FACET` (`OGAR/crates/ogar-osm/src/lib.rs:212`) reads a location as four HHTL rails. Each rail is a 256×256 tile with literal x/y (`Axis`, `:167`; TMS y increases north, `:170`). `rail_positions` is at `:342`, `facet_for` at `:322` and `classid` at `:91`. The crate depends only on `ogar-vocab`. It has **no latitude/longitude type**: a coordinate is a tile index, and the precision is the rail depth. This suits weather and privacy well. A heel or hip tile is a city-scale cell, so an observation can name a coarse tile instead of a GPS fix.
- **Geo concepts** are already minted: `OSM_NODE 0x0F01` … `OSM_USER 0x0F0A` (`ogar-vocab/src/lib.rs:2538`, `:2562`), and `geo_actions.rs` is the capability-table pattern.
- **Provenance and source precedents:**
  - `SourceStamp { source_id, observed_version }` and the read-only `DirectoryObservations` trait (`ogar-flow-m365/src/host.rs:37`, `:49`). This is the closest existing "observed facts" shape.
  - `ProviderId(&'static str)`, an opaque label that is "never an enum" so nothing downstream branches on the provider (`ogar-auth/src/user.rs:94`).
  - `CredentialProvider`, a backend-seam trait with a typed `ProviderError` (`ogar-secrets/src/lib.rs:380`, `:395`).
  - `Provenance {Ocr, Dom}`, which makes confidence depend on the source (`ogar-doc-ir/src/lib.rs:98`).
- **Domain map:** `ConceptDomain` (`ogar-vocab/src/lib.rs:1647`). The range `0x10–0x16` is "unassigned BY INTENT" (`:6606`), so weather must not be dropped into that gap.
- **The A2UI side:**
  - `Atmosphere {hue, sat, strength, transmission, glare(43..=63), halo, depth}` and `AMBIENTS` (`a2ui-theme/src/material.rs:137`, `:92`). Atmosphere is documented as "the viewer's own light", independent of theme and material (`:1-16`).
  - `Glass::light_angle_deg`, in screen space with default −110° (`a2ui-paint/src/optics.rs:109`, `:134`).
  - `RearLight {hue, sat, halo}`, "the only place colour enters the material" (`:447`).
  - `prefers-reduced-motion` already appears in `a2ui-theme/assets/themes.css` and `holo-glass/css/holo-glass.css`.

**Missing:**

- No time or epoch type exists anywhere in the OGAR crates. A search for `Epoch|Timestamp|Instant|Clock` found none.
- No observation-with-units type, no staleness concept, and no solar-geometry code.
- No precipitation, fog, snow or particle layer in a2ui-paint; optics is refraction and specular only.
- `Atmosphere` has no field for light *direction*. Direction lives only on `Glass`.

## 2. Proposed layering

```rust
// ── (a) ogar-weather (OGAR): vocabulary/contract only. No I/O, no clock, no serde in the hot path.
pub struct UnixMillis(pub i64);                 // instant of the measurement (UTC)
pub struct ProviderId(pub &'static str);        // opaque label, as in ogar-auth; "preset" is one value
pub enum LocationRef {
    Tile { classid: u32, facet: [u8; 12], rail_depth: u8 }, // ogar-osm V3 facet, truncated to a privacy tier
    Manual { label: &'static str, tile: [u8; 12] },         // city picked by the user, no geolocation
}
pub struct Observation {
    pub observed_at: UnixMillis,
    pub valid_until: UnixMillis,                // provider's Expires; staleness is derived, not stored
    pub location: LocationRef,
    pub source: SourceStamp,                    // provider id + upstream version / Last-Modified
    pub attribution: &'static str,              // licence credit the UI must show (CC BY)
    // Every value is optional: absent means "not observed", never zero.
    pub precip_mm_h: Option<f32>,
    pub precip_kind: Option<PrecipKind>,        // Rain | Snow | Sleet | Drizzle | Hail
    pub cloud_cover_pct: Option<u8>,
    pub visibility_m: Option<u32>,
    pub wind_ms: Option<f32>,
    pub wind_from_deg: Option<u16>,
    pub temperature_c: Option<f32>,
}
pub enum Freshness { Fresh, Stale { age_s: u32 }, Expired }
impl Observation {
    pub fn freshness(&self, now: UnixMillis, grace_s: u32) -> Freshness;
}

// ── (b) solar geometry: pure and deterministic, needs NO provider (NOAA/Meeus solar position).
pub struct SolarGeometry {
    pub elevation_deg: f32,
    pub azimuth_deg: f32,
    pub phase: DayPhase,                        // Night | Civil/Nautical/Astronomical twilight | GoldenHour | Day
}
pub fn solar_geometry(at: UnixMillis, lat_deg: f64, lon_deg: f64) -> SolarGeometry;
// The tile centroid gives lat/lon. City-level precision is more than enough for the sun.

// ── (c) projection: lives in A2UI (a2ui-theme, zero-dep), NOT in OGAR.
pub struct SkyLight {
    pub key_light_deg: f32,                     // → Glass::light_angle_deg
    pub kelvin: u16,                            // → hue/sat via a CCT table → RearLight / Atmosphere
    pub directional_share: u8,                  // cloud cover: 100 = hard sun, 0 = diffuse overcast
}
pub struct Weathering {
    pub droplet_density: u8,
    pub snow_density: u8,
    pub fog: u8,                                // from visibility
    pub wind_drift_deg: i16,
}
pub fn project(obs: &Observation, sun: &SolarGeometry, motion: MotionPref) -> (Atmosphere, SkyLight, Weathering);
// motion == Reduced ⇒ particles become static stipple / still frames; Stale ⇒ fade toward neutral DEFAULT.
```

**Which crate owns which layer:**

| layer | crate | why |
|---|---|---|
| (a) observation | `OGAR/ogar-weather` | The vocabulary and the contract only, like `ogar-osm`. Dependencies are `ogar-vocab`, and `ogar-osm` for `LocationRef`. |
| (b) solar geometry | either a module in `ogar-weather` or its own crate | It is pure maths with no I/O, so any consumer can call it with no provider and no permission. |
| providers | separate adapters per provider, e.g. `ogar-weather-metno` | Adapters do the HTTP, the User-Agent, caching and conditional requests. They return `Observation`. This follows the `CredentialProvider` seam pattern. They never ship in the default build. |
| (c) projection | `a2ui-theme`, as a `weather` module | It maps to `Atmosphere` and `SkyLight` and stays zero-dependency, which this crate requires. |
| particle and fog rendering | `a2ui-paint` | CSS/SVG/Canvas2D by default. GPU only behind the existing optional `wgpu` feature. |

The renderer sees only the output of `project`. It never learns about providers, consent or caching.

## 3. Manual presets, built as observations

Each preset is a `const Observation` with `ProviderId("preset")`, `LocationRef::Manual`, and a fixed `observed_at` chosen to land on the intended solar phase for a reference city. Presets therefore run through `solar_geometry` and `project` exactly like live data. Neither the API nor geolocation is needed.

| preset | precipitation | cloud | visibility | sun / time |
|---|---|---|---|---|
| sunny noon | 0 | 5% | 30 km | local noon, midsummer |
| overcast | 0 | 95% | 15 km | midday |
| rain | 4 mm/h, Rain | 90% | 6 km | afternoon |
| snow | 1.5 mm/h, Snow | 100% | 2 km | winter midday |
| fog | 0 | 100% | 150 m | morning |
| golden hour | 0 | 20% | 25 km | elevation about 4° |
| night | 0 | 10% | 20 km | elevation below −18° |

If a demo should look the same everywhere, a preset may also pin `SolarGeometry` directly.

## 4. Providers and licensing

All terms below were read on 2026-11-25. They need operator and counsel review before any commercial product relies on them.

- **Open-Meteo:** the free API is *non-commercial only*. Websites or apps with subscriptions or ads, and commercial products, need a paid plan (Standard, Professional or Enterprise). The data is CC BY 4.0. The free tier allows 600/min, 5,000/h and 10,000/day. https://open-meteo.com/en/terms. **Not usable commercially without a contract.**
- **MET Norway (api.met.no):**
  - Licence: NLOD 2.0 and CC BY 4.0. Credit "Data from MET Norway" and link the source. MET's weather icons are MIT-licensed. Do not use the Yr or NRK brand. https://api.met.no/doc/License, https://api.met.no/doc/TermsOfService
  - Required: a User-Agent that names the application and a contact address (a fake one leads to a ban), coordinates truncated to at most 4 decimals, mandatory caching that honours `Expires` and `If-Modified-Since`, under 20 requests per second, and backing off on 429.
  - Commercial use is not stated outright on those pages. It is implied by CC BY and NLOD; confirm it on met.no's licensing page (unverified).
- **DWD Open Data:** CC BY 4.0, with GeoNutzV source attribution such as "Datenbasis: Deutscher Wetterdienst", which applies to excerpts and modified data too. A 2019 DWD presentation says commercial use and redistribution are allowed. https://opendata.dwd.de/climate_environment/CDC/Terms_of_use.pdf, https://beobachtungen.dwd.de/dwd_cas/copyright_en. The format is raw files (GRIB, MOSMIX), not a simple JSON point API, so the adapter would be heavier.
- **NOAA/NWS (api.weather.gov):**
  - The data is public domain, "free to use for any purpose", under https://www.weather.gov/disclaimer.
  - The API requires a User-Agent with contact details (https://www.weather.gov/documentation/services-web-api). Its coverage is in practice limited to the US and its territories.
  - You may not imply NWS endorsement or use the NWS logo.

**Consent:**

- Geolocation is strictly opt-in, behind the cloud/sun button, and is off by default.
- The fallback is a manual city chosen by the user, which becomes a `LocationRef::Manual` tile.
- The default for everyone is "presets only, no network".
- Coordinates are truncated to the coarse tile before they leave the adapter. That covers MET's 4-decimal rule and minimises what is shared.
- Location and observations are never stored server-side, and do not need to be.

**Stale data:** past `valid_until` plus a grace period, the projection fades toward `Atmosphere::DEFAULT` instead of freezing on old rain. The attribution line stays visible whenever live data is shown.

## 5. Open questions for the operator

1. **Classid mint.** Does weather need a concept classid at all? Three options:
   - Keep `Observation` an unminted contract type, which is enough for rendering.
   - Mint a weather or environment domain in `ogar-vocab`. This needs the operator's ruling and should not go into the intentional `0x10–0x16` gap.
   - Treat it as a Geo sub-concept under `0x0F`.

   Minting is the operator's call only.
2. Should observations ever be persisted as rows (V3 facet: tile key + a value slab), or stay ephemeral view state?
3. Provider choice:
   - MET Norway, with attribution, as the default live source?
   - Is a paid Open-Meteo plan acceptable?
   - NWS for US users only?
   - A licensing sign-off by counsel before shipping.
4. Is solar geometry its own crate (it is reusable beyond weather) or a module in `ogar-weather`?
5. Should `Atmosphere` gain a direction field, or does the key light stay on `Glass` and get set by `project`?
6. Should the user's own atmosphere (Ruby etc.) blend with weather or be replaced by it? And what is the default strength of the weather overlay?
7. Which privacy tier does a location use: heel or hip, and roughly what cell size?
