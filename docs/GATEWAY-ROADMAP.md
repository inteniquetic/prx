# PRX API Gateway Roadmap

แผนยกระดับ `prx` จาก reverse proxy → API gateway เต็มรูปแบบ (อ้างอิง feature set ของ API gateway ชั้นนำในตลาดเป็น parity checklist) พร้อมปรับ WebUI ไปใช้ [shadcn-svelte](https://www.shadcn-svelte.com/)

แบ่งเป็น 2 track ที่เดินคู่กัน:

- **Track A** — Backend gateway features (Rust / Pingora)
- **Track B** — WebUI migration + หน้าจอใหม่ (Svelte 5 + shadcn-svelte)

---

## 1. หลักการออกแบบ (ต้องคงไว้ทุก phase)

1. **DB-less เสมอ** — ทุก entity ใหม่ (consumer, plugin, certificate) อยู่ใน `Prx.toml` ไฟล์เดียว hot-reload ได้ผ่าน watcher เดิม ไม่มี database
2. **Config ไม่ mutate in place** — feature ใหม่ precompute ทุกอย่างใน `RuntimeConfig::from_config` (src/runtime.rs) แล้ว swap ผ่าน `ArcSwap` เหมือนเดิม; hot path อ่านจาก snapshot แบบ lock-free
3. **State ที่ต้องรอด config reload ห้ามอยู่ใน `RuntimeConfig`** — ทุก swap จะ reset state ข้างใน (พฤติกรรมเดียวกับ circuit breaker ปัจจุบัน) ดังนั้น rate-limit counters, active-health state, cache index ต้องอยู่ global store (pattern เดียวกับ `Lazy` statics ใน src/metrics.rs) key ด้วย **ชื่อ** (route/service/consumer) ไม่ใช่ index
4. **Validation จุดเดียว** — invariant ใหม่ทั้งหมดลง `PrxConfig::validate()` (src/config.rs) เท่านั้น; referential integrity เป็น load-bearing เพราะ `RouteRuntime::from_config` panic ถ้าอ้างชื่อที่ไม่มีจริง
5. **ทุก config write ผ่าน `ConfigAdmin`** — atomic replace + verify + rollback เหมือนเดิม; ห้ามมี path เขียนไฟล์ตรง
6. **Zero-exception `cargo audit`** — dependency ใหม่ทุกตัวต้องผ่าน audit clean (ops/ZERO-EXCEPTION-POLICY.md); ห้ามใส่ ignore
7. **ทุก phase จบด้วย `make gate` เขียว** + e2e test ใน tests/ (single-threaded, real TCP ports) + อัปเดต docs/CONFIG-WIKI.md
8. **Plugin-first** — behavior ใหม่เกือบทั้งหมดเป็น plugin บน pipeline เดียว ไม่ hardcode ใน `PrxProxy` ยกเว้น core routing/LB

---

## 2. สถานะปัจจุบัน (สรุปจาก gap analysis)

### มีแล้ว
route/service/upstream model, host (exact + `*.wildcard`) + longest-prefix routing, weighted RR / random / hash LB, retry + backoff + failover, passive circuit breaker, Prometheus metrics, health/ready endpoints, DB-less TOML + hot reload + admin CRUD (services/routes) + embedded WebUI

> ⚠️ **ข้อค้นพบจากการ verify โค้ดจริง**: TLS ทั้งระบบตอนนี้ **ถูก compile ออก** — Cargo.toml เปิด pingora แค่ feature `lb` ทำให้ pingora-core ใช้ noop TLS stub (`vendor/pingora-core/src/protocols/tls/noop_tls`) ที่ `unimplemented!` ตอน handshake: TLS termination, h2-on-TLS และ `tls = true` ฝั่ง upstream จะ panic ทันทีที่มี TLS connection แรก และไม่มี e2e TLS test จึงไม่เคยถูกจับได้ → เป็นงาน Phase A0 ข้อ 9 ก่อนทุกงาน TLS อื่น

### มีบางส่วน (ต้องซ่อม/ต่อยอด)
| จุด | ปัญหา |
|---|---|
| `methods` บน route | parse + เก็บ + โชว์ใน UI แล้ว แต่ **ไม่เคยถูกเช็คตอน match** (`RouteRuntime` ทิ้ง field นี้) |
| Route priority | เรียงตามความยาว path อย่างเดียว — route ไม่มี host ที่ prefix ยาวกว่า ชนะ route host ตรงเป๊ะ |
| Hash LB | key ตายตัวเป็น host+path, ไม่ใช่ consistent hashing (ring reshuffle เมื่อ upstream เปลี่ยน) |
| Health check | มีแต่ on-demand TCP probe ผ่าน admin — ไม่มี background probe และผลไม่ feed เข้า LB |
| Host header → upstream | ถูก rewrite เป็น `upstream.sni` **เสมอ** — ไม่มีทาง preserve host ของ client |
| Admin API | CRUD แค่ services/routes, **ไม่มี auth**, เขียนทับ TOML ทำ comment หาย |
| Request size limit | มีเฉพาะฝั่ง admin (10MB) — traffic จริงไม่จำกัด |

### ยังไม่มีเลย (หมวดใหญ่)
plugin framework, consumers + authentication ทั้งหมด, rate limiting, CORS / IP restriction, request/response transformation, proxy cache, log sinks / tracing export, active health checks, regex path matching, TLS ที่ใช้งานได้จริง / SNI cert selection / cert management, gRPC / h2c / L4 stream

---

## 3. สถาปัตยกรรมใหม่ที่ต้องวางก่อน (หัวใจของ Track A)

### 3.1 Plugin framework (`src/plugin/`)

In-process, compiled-in, trait-based — ไม่ทำ external plugin server / WASM / scripting (ดู §6)

```rust
pub enum Flow {
    Continue,
    Respond { status: u16, body: Bytes },   // short-circuit เช่น 401/429/CORS preflight
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn priority(&self) -> i32;                                   // สูง = รันก่อน, ตายตัวต่อ plugin
    async fn on_request(&self, s: &mut Session, ctx: &mut RequestCtx) -> Result<Flow>;
    async fn on_upstream_request(&self, h: &mut RequestHeader, ctx: &mut RequestCtx) -> Result<()>;
    async fn on_response(&self, h: &mut ResponseHeader, ctx: &mut RequestCtx) -> Result<()>;
    // body hooks: ถูกเรียกต่อ chunk (Option<Bytes> + end-of-stream flag) — default no-op, จองไว้ตั้งแต่ A1
    async fn on_request_body(&self, chunk: &mut Option<Bytes>, end: bool, ctx: &mut RequestCtx) -> Result<()>;
    async fn on_response_body(&self, chunk: &mut Option<Bytes>, end: bool, ctx: &mut RequestCtx) -> Result<()>;
    fn on_log(&self, s: &Session, ctx: &RequestCtx);
}
```

Mapping เข้า `ProxyHttp` phases ที่มีอยู่ (src/proxy.rs):

| Plugin hook | ProxyHttp phase | หมายเหตุ |
|---|---|---|
| `on_request` | `request_filter` | หลัง route match — auth, IP restriction, rate limit, CORS preflight, termination |
| `on_upstream_request` | `upstream_request_filter` | X-Forwarded-*, transformer, hide_credentials |
| `on_response` | `response_filter` | CORS headers, response transformer (headers) — ตัว `response_filter` เปล่าถูก implement ตั้งแต่ A0 |
| `on_request_body` / `on_response_body` | `request_body_filter` / `response_body_filter` | per-chunk — streamed size limit (A3), body transform (A7) |
| `on_log` | `logging` | log sinks, tracing export |

- **Config**: block ใหม่ใน `Prx.toml`

  ```toml
  [[plugin]]
  name = "rate-limiting"        # ชื่อจาก registry ที่ compile ไว้
  route = "api-v1-route"        # หรือ service = "..." / ไม่ระบุ = global
  enabled = true
  [plugin.config]               # schema ของแต่ละ plugin (serde struct ของตัวเอง)
  minute = 60
  policy = "local"
  ```

- **Registry**: `fn build_plugin(name, &toml::Value) -> Result<Arc<dyn Plugin>>` — `validate()` เรียก registry เดียวกันเพื่อ reject ชื่อที่ไม่รู้จัก/config ผิด schema ตั้งแต่ตอน load
- **Chain resolution**: precompute ใน `RuntimeConfig::from_config` เป็น ordered chain ต่อ route (merge global → service → route; config ที่ specific สุดชนะต่อชื่อ plugin; เรียงด้วย priority) — hot path แค่ iterate slice
- **Consumer-scoped config** ทำใน Phase A3 (ต้องมี identity ก่อน): lookup เพิ่มหลัง auth สำเร็จ
- **State store**: `src/plugin/state.rs` — global `Lazy<DashMap<(plugin, scope_key), Arc<...>>>` สำหรับ counter/cache ที่ต้องรอด reload

### 3.2 Consumer entity

```toml
[[consumer]]
name = "mobile-app"
groups = ["mobile", "partner"]    # ให้ acl plugin (A3) ใช้

[[consumer.credential]]
type = "key"                  # key | basic | jwt
key = "..."                   # หรือ username/password_hash หรือ jwt key/secret/algorithm
```

- `RequestCtx` เพิ่ม `consumer: Option<ConsumerIdentity>`, `request_id: String`
- Auth plugin ตัวไหน validate สำเร็จ → set identity → plugin อื่น (ACL, rate-limit by consumer) ใช้ต่อ
- `anonymous = "consumer-name"` บน auth plugin = fallback แทน 401 (เปิดทาง multi-auth OR)

---

## 4. Track A — Backend phases

> ทุก phase: อัปเดต `PrxConfig::validate()`, เพิ่ม e2e ใน tests/, อัปเดต CONFIG-WIKI/PLAYBOOK, `make gate` เขียว

### Phase A0 — Routing correctness & proxy hygiene (ซ่อมก่อนสร้าง)

ของที่ gateway จริงต้องมีและส่วนใหญ่เป็น diff เล็กในไฟล์เดิม:

1. **บังคับใช้ `methods`** — ใส่เข้า `RouteRuntime` + เช็คใน `select_route` (ตอนนี้เป็น dead config)
2. **Route priority ใหม่ + path normalization** — host ตรงเป๊ะ > wildcard host > ไม่มี host, แล้วค่อย prefix ยาวสุด; normalize path ก่อน match (percent-decode + dot-segment removal ตาม RFC 3986) พร้อม e2e ยืนยันว่า path แบบ encode (`/api%2F../admin`) หลบ route-scoped plugin ไม่ได้ — เป็นเรื่อง security ไม่ใช่ polish
3. **`preserve_host` + `strip_path` + service-level `path` prepend** ต่อ route/service — default คงพฤติกรรมปัจจุบัน (rewrite host = sni) เพื่อไม่ breaking
4. **X-Forwarded-For/Proto/Host/Port + X-Real-IP** ไปทาง upstream + `trusted_ips` สำหรับ real-IP resolution
5. **Request ID** — สร้าง per request, ใส่ log + response header `X-Request-Id` (เคารพ header เดิมถ้า trusted)
6. **JSON error bodies** มาตรฐานเดียว (404/502/503) + latency headers (`X-Prx-Proxy-Latency`, `X-Prx-Upstream-Latency`) — ต้อง implement `response_filter` เปล่าๆ ใน `PrxProxy` ที่ phase นี้เลย (~10 บรรทัด; A1 ค่อย thread plugin chain เข้าไป)
7. **weight = 0 = drain** — เลิก clamp `1..=256` (src/runtime.rs `upstream_weight`) และลบ empty-ring fallback ใน `build_selection_ring` ที่จะแอบชุบชีวิต service ที่ weight เป็น 0 ทั้งหมด; validate: ทุก service ต้องมี weight > 0 อย่างน้อยหนึ่งตัว
8. **HTTP→HTTPS redirect** ต่อ route (`https_redirect_status_code`)
9. **เปิด TLS backend จริง** — ตอนนี้ TLS เป็น noop stub (ดู §2): เลือก feature `openssl` หรือ `boringssl` บน pingora (**อย่าเลือก rustls** — cert callback ที่ A6 ต้องใช้มีเฉพาะสอง backend แรกใน pingora 0.7), รัน `cargo audit` กับ dependency tree ใหม่ (openssl-sys/boring-sys), แก้ Docker build ให้มี C toolchain, เพิ่ม e2e TLS termination + tls-upstream
10. **Regex path matching** (ก้อนใหญ่สุดของ phase นี้) — route แบบ `~`-prefix regex + กติกา priority: ใน host tier เดียวกัน regex (เรียงด้วย `regex_priority` ที่ระบุ) ชนะ prefix, prefix เรียงตามความยาว

Deps ใหม่: `uuid` (หรือ ULID), `regex`
ไฟล์หลัก: src/runtime.rs, src/proxy.rs, src/config.rs, Cargo.toml (TLS feature)

### Phase A1 — Plugin framework

- ตาม §3.1 ทั้งหมด รวม body hooks แบบ default no-op (จองไว้ให้ A3/A7) — `response_filter` เปล่ามีแล้วจาก A0, phase นี้ thread plugin chain เข้าทุก hook
- Plugin แรกเพื่อพิสูจน์ pipeline: **request-termination** (เรียบสุด: match → ตอบ status/body ที่ config ไว้)
- Definition of done: plugin scope ทั้ง global/service/route ทำงาน, chain order deterministic, reload แล้ว chain ใหม่มีผลโดย request ค้างเก่าไม่พัง, e2e ครอบ

Deps ใหม่: `dashmap` (state store §3.1)

### Phase A2 — Authentication, consumers & admin hardening

- **Consumer entity** ตาม §3.2 + CRUD `/admin/consumers*`
- **key-auth** plugin (header/query, `hide_credentials`, `anonymous` fallback)
- **basic-auth** plugin (password hash ด้วย `argon2`)
- **Consumer identity headers** → upstream (`X-Consumer-Name`, `X-Anonymous-Consumer`)
- **Admin API auth** — bearer token จาก config/env (`PRX_ADMIN_TOKEN`) แบบ **opt-in**: ไม่ตั้ง = เปิดเหมือนเดิม + warn ใน log (กัน embedded WebUI ใช้ไม่ได้ระหว่างรอ Track B) และเพิ่มช่องกรอก token ใน WebUI **เดิม** (Svelte 4) เป็น diff เล็กใน phase นี้เลย ไม่รอ B3; จำเป็นก่อนเพิ่ม entity อ่อนไหว (credentials!) เข้า admin
- **Credential ไม่เก็บ plaintext** ใน TOML สำหรับ basic (hash at rest); key-auth เก็บ key ตรง (เหมือน gateway อ้างอิง) แต่รองรับ env-var reference `${ENV_VAR}` ใน field เป็น opt-in

Deps ใหม่: `argon2`
หมายเหตุ: JWT แยกไป A3 เพื่อให้ A2 จบเร็ว

### Phase A3 — Traffic control & security plugins

- **rate-limiting** — fixed window (second/minute/hour/day), `limit_by` consumer/credential/ip/service/header/path, policy `local` (in-memory ผ่าน state store §3.1), response headers `X-RateLimit-*` + `Retry-After`, `fault_tolerant`; policy `redis` เป็น optional cargo feature ทีหลัง
- **jwt** — HS256/384/512 + RS256 + exp/nbf, key lookup ต่อ consumer, token จาก header/query/cookie (dep: `jsonwebtoken`)
- **acl** — allow/deny กลุ่มของ consumer
- **cors** — origins (exact/regex), methods, headers, credentials, max-age, preflight short-circuit
- **ip-restriction** — allow/deny CIDR (dep: `ipnet`)
- **request-size-limiting** — Content-Length เช็คใน `on_request`; body แบบ chunked/streamed บังคับผ่าน `on_request_body` (hook มีแล้วจาก A1)
- **request-transformer / response-transformer** — add/remove/rename/replace เฉพาะ **headers + query** ก่อน (body transform อยู่ A7 บน body hooks)
- **correlation-id** — ต่อยอด request ID ของ A0 (custom header name + echo downstream) ไม่ใช่ระบบใหม่

Deps ใหม่: `jsonwebtoken`, `ipnet` (`regex` มีแล้วจาก A0, `dashmap` จาก A1)

### Phase A4 — Observability

- **Trace export** — OpenTelemetry OTLP + W3C `traceparent` extract/inject (deps: `opentelemetry*`, `tracing-opentelemetry` — ก้อนใหญ่ ลาก tonic/prost/hyper มาด้วย) — **entry gate**: spike บน branch แล้ว `cargo audit --deny warnings` ต้องผ่านก่อนเริ่มจริง และถือโอกาสแก้ scripts/release-gate.sh ให้ใช้ `--deny warnings` ให้ตรงกับที่ ops/ZERO-EXCEPTION-POLICY.md เขียนไว้ (ตอนนี้สคริปต์หลวมกว่านโยบาย — `cargo audit` เฉยๆ ไม่ fail บน unmaintained warning)
- **Log sinks เป็น plugin**: `file-log`, `http-log` (batch + queue) บน `on_log` hook ด้วย log object มาตรฐานเดียว (request/response/latencies/consumer/route/tries)
- **Prometheus metrics เพิ่ม**: bandwidth, per-consumer status, plugin latency, upstream health gauge
- Latency แยกช่วง (`connected_to_upstream` phase → upstream connect time)

### Phase A5 — Load balancing & health

- **Active health checks** — background tokio task ต่อ service (HTTP/TCP probe, interval แยก healthy/unhealthy, threshold) — state อยู่ global store, **feed เข้า `select_from_ring`** ร่วมกับ passive
- **Passive health จาก HTTP status** — นับ 5xx เป็น failure ได้ (ตอนนี้นับเฉพาะ transport error)
- **Least-connections** LB (in-flight counter ต่อ upstream)
- **Consistent hashing จริง** — `hash_on` consumer/ip/header/cookie/query + fallback; reuse `Continuum` จาก `pingora-ketama` (อยู่ใน dependency tree ผ่าน feature `lb` อยู่แล้ว) — มี adaptation cost เพราะมัน key ด้วย `Backend`/`SocketAddr` ไม่ใช่ index ring ของ prx
- **Manual health control** — `POST /admin/upstreams/.../health` force healthy/unhealthy + reset circuit
- **Health status API** — สถานะ per-target (circuit + active probe) แบบ machine-readable ให้ WebUI

### Phase A6 — TLS & protocol expansion

> Prerequisite: TLS backend จริงจาก A0 ข้อ 9 — `TlsSettings::with_callbacks` + `certificate_callback` มีเฉพาะ backend `openssl`/`boringssl` ใน pingora 0.7 (rustls ใช้ feature นี้ไม่ได้)

- **SNI-based cert selection** — `[[certificate]]` entities + map hostname→cert ผ่าน `certificate_callback` (ถูกเรียกต่อ handshake → อ่านจาก ArcSwap snapshot = ได้ cert **hot-reload** ฟรี แก้ข้อจำกัด startup-only)
- **TLS version/cipher config** บน listener
- **Upstream mTLS** (client cert ต่อ service) + **custom CA bundle** ต่อ service (แทน system trust อย่างเดียว)
- **gRPC passthrough** — ALPN h2 ไป upstream (`peer.options.alpn`), e2e ด้วย gRPC echo
- **h2c** downstream (ตาม API ที่ pingora 0.7 มีให้ — ตรวจตอน implement)
- **WebSocket** — เพิ่ม e2e ยืนยัน upgrade passthrough (โค้ดน่าจะรองรับผ่าน pingora แล้ว แต่ยังไม่มี test)
- ACME/Let's Encrypt = ไว้หลังสุดของ track (optional)

### Phase A7 — Admin & config management ส่วนขยาย

- CRUD ครบทุก entity ใหม่ (plugins, certificates) + pagination + `?tags=` filter (เพิ่ม `tags` ทุก entity)
- `POST /admin/validate` — dry-run validate config body (โครงจาก `POST /web/health/routes` เดิม)
- `prx validate` CLI subcommand (offline check ก่อน deploy)
- TOML write แบบรักษา comment/format (`toml_edit` — อยู่ใน dependency tree ผ่าน `toml` 0.8 อยู่แล้ว แทบฟรี) — แก้ปัญหา CRUD ทำ comment หาย
- **Body transformation** (JSON add/remove/replace) บน request/response transformer ผ่าน body hooks จาก A1
- Proxy-cache plugin (in-memory, `X-Cache-Status`) — ประเมิน pingora cache phases ก่อนเขียนเอง

---

## 5. Track B — WebUI: shadcn-svelte

> ข้อเท็จจริงบังคับ: shadcn-svelte ปัจจุบันอยู่บน **bits-ui (Svelte 5 เท่านั้น)** + **Tailwind v4 (CSS-first)**; ใช้กับ plain Vite SPA ได้ (มี installation path official, ไม่ต้องใช้ SvelteKit) — ตอนนี้ webui เป็น Svelte 4.2 + Tailwind 3.4 + Vite 5 จึงต้อง migrate stack ก่อน

### Phase B1 — Stack migration (atomic bump เดียว)

1. Bump พร้อมกัน: `svelte@^5`, `svelte-check@^4`, `@sveltejs/vite-plugin-svelte@^5`, `vite@^6` (เวอร์ชันพวกนี้ผูกกัน — bump ทีละตัวจะเจอ error งงๆ) — **กับดัก: หลังข้อนี้ build เขียวแต่แอปไม่ boot** เพราะ src/main.ts ใช้ `new App({...})` ซึ่ง Svelte 5 เลิกรองรับ และ webui ไม่มี runtime test ที่จะจับ → แก้เป็น `mount(App, { target })` ใน commit เดียวกัน หรือ land ข้อ 1+2 เป็น commit เดียว
2. `npx sv migrate svelte-5` — auto: `on:click`→`onclick`, `export let`→`$props()`, `$:`→`$derived`/`$effect`, `<slot>`→snippet (มีที่เดียวคือ AppLayout)
3. **แปลง `createEventDispatcher` เป็น callback props ด้วยมือ (12 ไฟล์ — 11 ตัวที่ใช้จริง + AppToolbar ซึ่งเป็น dead code ให้ลบทิ้งแทน)** — งาน manual ก้อนใหญ่สุด และจำเป็นเพราะ component Svelte 5 ไม่ forward `on:` directives (ปุ่ม shadcn ที่รับ `on:click` จะเงียบเฉยๆ); `<svelte:fragment slot="header-actions">` ใน 4 pages ต้องแก้เป็น snippet พร้อม AppLayout ด้วย
4. Tailwind 3→4: ลบ `tailwind.config.ts` + postcss/autoprefixer → `tailwindcss@^4` + `@tailwindcss/vite`; `@tailwind base/...` → `@import "tailwindcss";`; สี custom ใน config เดิม (ink/fog/tide/aqua/mint, shadow-panel) **ประกาศไว้แต่ไม่มี component ไหนใช้เลย — ลบทิ้ง อย่า port ไป `@theme`**; รัน `npx @tailwindcss/upgrade` แล้วตรวจมือสิ่งที่ tool ไม่จับ: `shadow-sm` (v4 เปลี่ยนความหมาย — มีใช้จริงหลายจุดรวม `class:shadow-sm` directive ที่ tool ไม่ rewrite) และ `focus:outline-none` (~30 จุด → พฤติกรรมเดิมใน v4 คือ `outline-hidden`)
5. เพิ่ม `$lib` alias ใน tsconfig + vite.config.ts (ตอนนี้ import เป็น relative ทั้งหมด)
6. `npx shadcn-svelte@latest init` — global CSS ชี้ `src/app.css` (อย่าใช้ default `src/routes/...` ของ docs — repo นี้ไม่มี routes/), base color slate
7. Dark mode: แอปเป็น dark-only อยู่แล้ว → ใส่ `class="dark"` ที่ `<html>` ใน index.html แล้ว **ลบสี hardcode ใน app.css ทิ้ง** ให้ CSS variables ของ shadcn คุม (ใส่ mode-watcher เฉพาะเมื่ออยากได้ light mode จริง)
8. Verify: `npm run check` + `npm run build` + `cargo build` (embed dist ใหม่) + smoke test ผ่าน admin listener

### Phase B2 — แปลง component เดิม (ทีละหน้า ไล่จากเปลือกเข้าใน)

`npx shadcn-svelte@latest add button card dialog sheet table input label select switch checkbox tabs sidebar separator badge alert skeleton tooltip dropdown-menu scroll-area sonner field toggle-group`

| เดิม | แทนด้วย |
|---|---|
| AppLayout + Sidebar | `Sidebar.Provider/Root/Menu` + `Sidebar.Inset` (ทำก่อน — ทุกอย่าง nest ในนี้) |
| StatsCard / ServerCard / ObservabilityCard | `Card` + `Field/Input/Switch/Badge` |
| RouteTableCard / RoutesPage / ServicesPage | `Table` + `Input` (search) + `Badge` + `Skeleton`/`Alert` (สถานะ health) |
| RouteFormModal / ServiceFormModal | `Dialog` (ได้ focus-trap/esc ฟรี) |
| RouteDetailPanel | `Sheet` (side=right) |
| SettingsPage (หน้าใหญ่สุด ~850 บรรทัด) | `Tabs` + `Switch` + `Toggle Group` (แทน tab bar / checkbox-toggle / segmented control ที่เขียนเอง) + `Field/Input` |
| DashboardPage | composition `Card` grid — ไม่มี 1:1 แต่เป็นหน้าที่งาน runes-migration เยอะสุด |
| TomlPreviewCard | `Card` + `ScrollArea` + `Button` copy |
| ValidationCard | `Alert` (destructive) |
| AppToolbar | **ลบทิ้ง** — dead code ไม่มีใคร import; status UI จริงคือ `setAdminStatus` ใน App.svelte + status divs ใน header-actions ของ 4 pages → แปลงเป็น `svelte-sonner` toasts (`<Toaster/>` ครั้งเดียวใน App.svelte) |

- **ไม่ใช้** Form stack ของ shadcn (formsnap + superforms — ผูก SvelteKit): ใช้ `Field/Input/Select` เปล่าๆ + validation เดิม (`validationIssues` ใน `stores/config.ts` และ `validate()` ในสอง modal — `configNormalize.ts` เป็นแค่ตัว normalize ไม่ใช่ validation)
- Data Table (`@tanstack/table-core`) เฉพาะถ้าต้องการ sort/filter จริง — เริ่มจาก `Table` ธรรมดาก่อน
- svelte stores เดิม (config/navigation) **ใช้ต่อได้** ใน Svelte 5 — ไม่ต้อง rewrite

### Phase B3 — หน้าจอใหม่ตาม backend phases

| Backend phase | WebUI เพิ่ม |
|---|---|
| A2 | Consumers page (list/form/credentials); ช่อง admin token มีบน UI เดิมตั้งแต่ A2 แล้ว — ยกมาเป็น component จริงที่นี่ |
| A1/A3 | Plugins page: enable/config plugin ต่อ scope (global/service/route) + แสดง chain ที่ resolve แล้วต่อ route |
| A5 | Health dashboard: สถานะ per-target (active + passive), ปุ่ม force healthy/reset circuit |
| A6 | Certificates page (upload/rotate, SNI mapping) |
| A4 | Metrics cards เพิ่มบน Dashboard (ดึงจาก Prometheus endpoint ที่ A4 ทำให้จริง; health-status cards อยู่แถว A5 แล้ว; chart ด้วย layerchart เป็น optional ทีหลัง) |

### ความเสี่ยงหลักของ Track B

- ลำดับ build ผูกกับ binary: `vite build` ต้องมาก่อน `cargo build` เสมอ ไม่งั้น UI เก่าถูก embed — ตรวจ CI/Dockerfile ด้วย
- ครึ่งทาง migrate แล้วผสม `on:click` กับ shadcn component = ปุ่มตายเงียบ → migrate event wiring ให้จบเป็นรายหน้า
- webui ไม่มี test suite (มีแค่ svelte-check + build) → ทุกหน้า smoke test กับ prx จริงหลัง embed

---

## 6. สิ่งที่จงใจไม่ทำ (YAGNI — ตัดสินใจแล้ว จะกลับมาเมื่อมีเหตุจริง)

| ตัด | เหตุผล |
|---|---|
| Database mode / hybrid CP-DP | DB-less TOML คือจุดแข็งของ prx; multi-node ค่อยว่ากันเมื่อมี use case |
| External plugin servers / WASM / Lua scripting | compiled-in trait ครอบ use case ปัจจุบันทั้งหมด ด้วย cost ต่ำกว่ามาก |
| Expressions router | host+method+path(+regex) พอ; ค่อยเพิ่มเมื่อชนข้อจำกัดจริง |
| OAuth2 **provider** mode / LDAP / session cookie | ตัว validate (JWT) พอสำหรับ gateway; การเป็น identity provider เป็นคนละ product |
| TCP/TLS/UDP L4 stream proxy, gRPC-Web bridging, REST↔gRPC transcoding | ยังไม่มี demand; gRPC passthrough (A6) ครอบเคสหลัก — pingora มี stream service รองรับถ้าถึงเวลา |
| hmac-auth | niche; JWT ครอบเคส signed-request ส่วนใหญ่ |
| Header-based route matching | host+method+path(+regex) พอ; เพิ่มเมื่อชนข้อจำกัดจริง |
| Bot detection, response-ratelimiting, syslog/loggly/datadog sinks | http-log + Prometheus ครอบได้; sink เฉพาะทางเพิ่มเป็น plugin ภายหลังได้เพราะ framework รองรับแล้ว |
| Secrets vault backends (cloud) | เริ่มที่ env-var reference ก่อน (A2) |

---

## 7. ลำดับและ dependency ระหว่าง phase

```
A0 ─► A1 ─► A2 ─► A3 ─► A4
        │     └───► A5 (อิสระจาก A4)
        ├──► A6 (ใช้ TLS backend ที่เปิดใน A0 ข้อ 9)
        └──► A7 (เริ่มหลัง A2 ได้ แต่ certificate CRUD ต้องรอ A6 สร้าง entity ก่อน)

B1 ─► B2 ─► B3 (B3 ไล่ตาม backend phase ที่ปล่อย)
B1 เริ่มได้ทันที ขนานกับ A0/A1
```

จุดตัดสำคัญ: **A1 (plugin framework) คือคอขวดของทุกอย่างใน Track A** — ออกแบบ/review ให้หนักที่สุดตรงนั้น ส่วน **B1 คือคอขวดของ Track B** และไม่พึ่ง backend เลย เริ่มพร้อมกันได้

## 8. Definition of done ต่อ phase (ซ้ำเพื่อความชัด)

1. `make gate` เขียว (fmt + clippy -D warnings + test single-threaded + audit zero-exception)
2. e2e ครอบ happy path + failure path ของ feature นั้นใน tests/ (pattern real-TCP-port เดิม)
3. `docs/CONFIG-WIKI.md` + `docs/CONFIG-PLAYBOOK.md` อัปเดต config surface ใหม่
4. ถ้ามีงาน UI: `npm run build` → `cargo build` → smoke test ผ่าน admin listener แล้วเท่านั้น
