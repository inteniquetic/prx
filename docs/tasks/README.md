# prx — Roadmap & Task Index

แผนงานสำหรับทำให้ `prx` เป็น reverse proxy ที่ **เร็ว/กินแรมน้อยกว่า nginx และ haproxy** ในงานจริง
และมี **Web UI (Svelte + shadcn) สวย ทันสมัย ใช้ง่าย** สำหรับแก้ config

แต่ละไฟล์ในโฟลเดอร์นี้ = 1 task ย่อย ทำจบได้ใน 0.5–2 วัน มี acceptance criteria ชัดเจน
หยิบไปเปิด PR ทีละใบได้เลย

## ตั้งเป้าหมายให้ตรงความจริงก่อน

Cloudflare ไม่ได้บอกว่า Pingora "เร็วกว่า nginx หลายเท่าในทุกเคส" — สิ่งที่เขารายงานคือ
ในงาน production ของเขา Pingora ใช้ **CPU ~1/3 และ memory ~1/3** ของ nginx ที่ทราฟฟิกเท่ากัน
และ **p95/p99 ดีขึ้นมาก** เพราะสถาปัตยกรรม (multi-threaded work-stealing + connection pool ที่แชร์ข้าม thread)
ไม่ใช่เพราะภาษา Rust อย่างเดียว

ดังนั้นเป้าหมายของ prx ที่วัดได้จริง:

| Metric | Target vs nginx/haproxy (same hardware, same upstream) |
|---|---|
| Throughput (RPS) ต่อ 1 core | เท่าหรือดีกว่า |
| p99 / p999 latency | ดีกว่าอย่างน้อย 20% ตอน connection reuse สูง |
| RSS ที่ 10k concurrent conns | ต่ำกว่าอย่างน้อย 30% |
| Config reload | ไม่ drop connection (nginx/haproxy ต้อง spawn worker ใหม่) |
| เวลาแก้ config | ผ่าน Web UI < 30 วินาที ไม่ต้อง ssh |

**กติกาเหล็ก:** ห้ามเคลมว่า "เร็วกว่า" ถ้ายังไม่ผ่าน Phase 0 (harness + baseline)
ทุก optimization ต้องมีตัวเลข before/after แนบใน PR

## Phases

| Phase | ชื่อ | เป้าหมาย | จำนวน task |
|---|---|---|---|
| 0 | Measurement | วัดได้ก่อน ค่อยปรับ | 5 |
| 1 | Data plane | ทำให้ hot path เร็วและกินแรมน้อย + ฟีเจอร์ที่ proxy ระดับ prod ต้องมี | 15 |
| 2 | Control plane | Admin API ปลอดภัย, config มีประวัติ/rollback, มี schema ให้ UI ใช้ | 7 |
| 3 | Web UI | Svelte 5 + shadcn-svelte, modern, ใช้ง่าย | 10 |
| 4 | Observability & Ops | metrics, tracing, packaging, docs | 5 |

## Task list

### Phase 0 — Measurement (ทำก่อนทุกอย่าง)

| ID | Task | Size | Depends | สถานะ |
|---|---|---|---|---|
| [T001](T001-bench-harness.md) | Benchmark harness: prx vs nginx vs haproxy | M | — | ✅ done |
| [T002](T002-baseline-report.md) | Baseline report + `docs/BENCHMARKS.md` | S | T001 | 🟡 micro-bench วัดแล้ว / proxy-vs-proxy รอเครื่องที่มี Docker |
| [T003](T003-profiling-and-microbench.md) | Flamegraph + criterion micro-bench | M | T001 | ✅ done |
| [T004](T004-perf-ci-gate.md) | Perf regression gate ใน CI | M | T002, T003 | ✅ done |
| [T005](T005-dependency-security-upgrade.md) | แก้ช่องโหว่ dependency ให้ `cargo audit` เขียว | L | — | ✅ done (pingora 0.9, ลบ `vendor/` ทิ้ง) |

### Phase 1 — Data plane

| ID | Task | Size | Depends | สถานะ |
|---|---|---|---|---|
| [T101](T101-zero-alloc-request-path.md) | ตัด allocation ต่อ request ใน `request_filter` | M | T003 | ✅ done |
| [T102](T102-route-matcher-index.md) | เปลี่ยน route matching จาก linear scan เป็น host map + path trie | L | T003 | ✅ done |
| [T103](T103-config-snapshot-access.md) | เลิก `load_full()` ต่อ request | S | T101 | ✅ done |
| [T104](T104-socket-worker-tuning.md) | SO_REUSEPORT, tcp_nodelay, backlog, worker/threads | M | T002 |
| [T105](T105-upstream-pool-keepalive.md) | Upstream connection pool / keepalive / H2 multiplexing | M | T002 | 🟡 `upstream_h2` ทำไปแล้วใน T115 |
| [T106](T106-header-rewrite-rules.md) | Header add/remove/set ต่อ route (precompiled) | M | T102 | ✅ done (เจอบั๊ก WebUI ตายด้วย) |
| [T107](T107-timeout-retry-budget.md) | Request timeout + retry budget กัน retry storm | M | — | ✅ done (แก้บั๊ก POST ถูก retry) |
| [T108](T108-rate-limit.md) | Rate limit + connection limit ต่อ route/IP | L | T102 | ✅ done |
| [T109](T109-micro-cache.md) | In-memory micro-cache สำหรับ GET | L | T102 | ✅ done |
| [T110](T110-compression.md) | gzip/brotli response compression | M | T106 | ✅ done |
| [T111](T111-tls-perf-multicert.md) | TLS: session resumption, ALPN, multi-cert SNI | L | T104 | 🟡 multi-cert SNI done (TLS เคยพังทั้งหมด) |
| [T112](T112-acme-auto-tls.md) | ACME auto TLS (Let's Encrypt) | L | T111 | ✅ done (เทสต์กับ pebble จริง) |
| [T113](T113-active-health-check.md) | Active health check prober (เสริม passive CB) | M | — | ✅ done |
| [T114](T114-lb-least-conn-sticky.md) | LB: least_conn, P2C-EWMA, sticky session | M | T105 | ✅ done |
| [T115](T115-ws-grpc-conformance.md) | เทสต์จริงของ WebSocket + gRPC streaming | M | — | ✅ done (เจอบั๊ก 2 ตัว) |

### Phase 2 — Control plane

| ID | Task | Size | Depends |
|---|---|---|---|
| [T201](T201-admin-auth-hardening.md) | Auth + bind loopback + CORS/CSRF ของ Admin API | M | — |
| [T202](T202-config-history-rollback.md) | เก็บประวัติ config + diff + rollback | M | T201 |
| [T203](T203-validate-dry-run-api.md) | `POST /web/config/validate` คืน error แบบมีพิกัด | M | — | 🟡 endpoint + error/warning ครบพร้อมพิกัด (ทำใน T307); เหลือ error code ของ TOML syntax แยกชนิด |
| [T204](T204-atomic-apply-reload.md) | Apply แบบ atomic + auto-rollback ถ้า reload ไม่ผ่าน | M | T202, T203 | 🟡 atomic write + `If-Match` 409 แล้ว (T307); เหลือ reload ซ้ำจาก watcher และรายละเอียดใน response |
| [T205](T205-openapi-typed-client.md) | OpenAPI + JSON Schema + typed client ให้ WebUI | M | T203 |
| [T206](T206-audit-log.md) | Audit log ใครแก้อะไรเมื่อไหร่ | S | T201 |
| [T207](T207-live-stats-stream.md) | SSE stream สถิติสดให้ dashboard | M | T401 | ✅ done (ทำพร้อม T306; แก้ bucket ของ histogram ที่จบที่ 10 ms) |

### Phase 3 — Web UI (Svelte 5 + shadcn-svelte)

| ID | Task | Size | Depends |
|---|---|---|---|
| [T301](T301-webui-svelte5-shadcn-foundation.md) | อัป Svelte 5 + ติดตั้ง shadcn-svelte + design token | L | — | ✅ done (light mode ไม่เคยมีมาก่อน) |
| [T302](T302-design-system-components.md) | ชุด component พื้นฐาน + dark mode | M | T301 | ✅ done (เขียน component เอง — registry ถูก network บล็อก) |
| [T303](T303-app-shell-navigation.md) | App shell: sidebar, topbar, breadcrumb, command palette | M | T302 | ✅ done (URL จริง + แก้ SPA fallback ฝั่ง server) |
| [T304](T304-routes-crud-ui.md) | หน้า Routes: data table + form validation | L | T303, T205 | ✅ done (เจอบั๊ก Save ลบ header/rate limit/cache ทิ้ง) |
| [T305](T305-services-upstreams-ui.md) | หน้า Services/Upstreams + health badge | L | T304 | ✅ done (ปิดบั๊ก Save ลบ health check/sticky ทิ้ง) |
| [T306](T306-dashboard-live-metrics.md) | Dashboard สถิติสด + กราฟ | L | T303, T207 | ✅ done (ทำ T207 ให้ด้วย; กราฟเขียนเอง ไม่พึ่งไลบรารี) |
| [T307](T307-toml-editor-diff-apply.md) | TOML editor + diff ก่อน apply | M | T303, T203 | ✅ done (ทำแกนของ T203 + `If-Match` ของ T204 ให้ด้วย) |
| [T308](T308-settings-tls-observability-ui.md) | หน้า Settings: server/TLS/observability | M | T303 |
| [T309](T309-ux-polish-i18n.md) | Empty state, skeleton, toast, i18n th/en | M | T304, T305 |
| [T310](T310-webui-testing-and-embed.md) | Vitest + Playwright + embed pipeline ใน CI | M | T309 |

### Phase 4 — Observability & Ops

| ID | Task | Size | Depends |
|---|---|---|---|
| [T401](T401-metrics-expansion.md) | Metrics ต่อ route/upstream + histogram | M | — |
| [T402](T402-otel-tracing.md) | OpenTelemetry tracing (opt-in) | M | T401 |
| [T403](T403-access-log-async-json.md) | Access log JSON แบบ non-blocking + sampling | M | T101 |
| [T404](T404-packaging-deploy.md) | Docker slim, systemd unit, Helm chart | M | T310 |
| [T405](T405-docs-and-benchmark-publication.md) | เอกสารสถาปัตยกรรม + เผยแพร่ผล benchmark | M | T004 |

## ผลจาก Phase 0 (วัดแล้ว)

micro-benchmark รันแล้ว ผลเต็มอยู่ใน [`docs/BENCHMARKS.md`](../BENCHMARKS.md) สรุปสิ่งที่เจอ:

| สิ่งที่เจอ | ตัวเลข | ไปแก้ที่ |
|---|---|---|
| wildcard host matching จัดสรรหน่วยความจำต่อ route ต่อ request | 59.3 µs/request → **81.9 ns (724×)** | ✅ [T101](T101-zero-alloc-request-path.md), [T102](T102-route-matcher-index.md) |
| route matching เป็นเชิงเส้นตามจำนวน route | 3.1–4.3 µs → **55–84 ns คงที่ทุกขนาด (40–74×)** | ✅ [T102](T102-route-matcher-index.md) |
| `normalize_host` จัดสรร String ทุก request | 43 ns → **26.5 ns** (คืน `Cow::Borrowed`) | ✅ [T101](T101-zero-alloc-request-path.md) |
| reload แพงขึ้นเพราะต้องสร้าง index | 1.32 ms → 1.91 ms ที่ 1000 routes (งบ 5 ms) | ยอมรับได้ อยู่นอก request path |

**T101, T102, T103, T115 ทำเสร็จแล้ว**

T115 เจอสองเรื่องที่ไม่มีใครรู้มาก่อนเพราะไม่เคยมีเทสต์:

| สิ่งที่เจอ | ผลกระทบ | สถานะ |
|---|---|---|
| **gRPC ใช้งานไม่ได้เลย** — ALPN ของ upstream ถูกตั้งเป็น H1 เสมอ และ listener ไม่เปิด h2c | เรียก gRPC ผ่าน prx ได้ 502 ทั้งที่ README เคลมว่ารองรับ | ✅ แก้แล้ว (`[server] h2c`, `[[service]] upstream_h2`) |
| **websocket ที่เงียบเกิน `read_timeout_ms` โดนตัด** | connection ที่ idle ตายทุกครั้งที่ตั้ง read timeout | ✅ แก้แล้ว (ไม่ใส่ timeout ให้ connection ที่ upgrade) |

T111 เจอเรื่องที่ใหญ่ที่สุดของโปรเจกต์:

| สิ่งที่เจอ | ผลกระทบ | สถานะ |
|---|---|---|
| **TLS ใช้งานไม่ได้เลย** — pingora ถูก build โดยไม่เปิด TLS feature ทำให้ `TlsSettings::intermediate()` ทิ้ง cert path และ `tls_handshake()` เป็น `unimplemented!()` | ทุก connection ที่เข้าทาง HTTPS ทำให้ worker thread **panic** ทั้งที่ `Prx.toml` ตัวอย่างมี `[server.tls]` อยู่ | ✅ แก้แล้ว (เปิด feature `openssl`) |
| **h2c ทำให้ HTTP/1.1 over TLS พัง** — TLS stream peek ไม่ได้ pingora จึงเข้าโหมด h2 ทั้งที่ ALPN ตกลงเป็น http/1.1 | client HTTP/1.1 ได้ h2 frame เป็น body | ✅ แก้แล้ว (แยก TLS เป็น service ของตัวเอง) |

T106 เจออีกสองเรื่อง:

| สิ่งที่เจอ | ผลกระทบ | สถานะ |
|---|---|---|
| **Web UI + admin API ตายตั้งแต่ start** — `admin_router()` ใช้ path syntax `:name` ของ axum 0.7 แต่ repo ใช้ axum 0.8 | router panic → thread admin ตาย (proxy ยังวิ่ง จึงไม่มีใครสังเกต) | ✅ แก้แล้ว |
| **metrics หายเมื่อปิด access log** — `observe_request` อยู่ใต้ `if !access_log { return; }` | `/metrics` ว่างเปล่าใน production ที่ปิด access log | ✅ แก้แล้ว |

T112 เจอเรื่องหนึ่งที่เทสต์จับได้:

| สิ่งที่เจอ | ผลกระทบ | สถานะ |
|---|---|---|
| **`/.well-known/acme-challenge/` หลุดไป upstream** — เงื่อนไขเดิมเช็คว่า "มี challenge ค้างอยู่ไหม" แต่ระหว่าง order สอง order store ว่าง | token ที่ไม่รู้จักถูก proxy ไปหา backend แทนที่จะเป็น 404 | ✅ แก้แล้ว (เปิด ACME = prx เป็นเจ้าของ prefix เสมอ) |

งานถัดไป: วัด proxy-vs-proxy จริงเมื่อมีเครื่องที่มี Docker (T002) แล้วไล่ T104/T105 ต่อ

และ Phase 0 ยังเจออีกสองเรื่องที่ไม่ได้อยู่ในแผนเดิม:

- **build พังอยู่ก่อนแล้ว** — `vendor/pingora-core` คอมไพล์ไม่ผ่านกับ libc ที่ lock ไว้ (แก้แล้วใน T001,
  แล้ว T005 อัป pingora 0.9 ซึ่ง upstream แก้ให้แล้ว จึงลบ `vendor/` ทิ้ง)
- **`cargo audit` แดง** — pingora 0.7 ลาก `pingora-cache` 0.7.0 (cache poisoning, 8.4 high) และ `h2` 0.4.13 มาด้วย
  → [T005](T005-dependency-security-upgrade.md)

## Milestones ที่แนะนำ

- **M1 — "พิสูจน์ว่าเร็วจริง"** : T001–T004, T101–T105 → ได้ตัวเลขเทียบ nginx/haproxy ที่ reproduce ได้
- **M2 — "UI สวย ใช้ง่าย"** : T201, T203, T205, T301–T307 → แก้ config ผ่านเว็บได้เต็มรูปแบบ
- **M3 — "Production ready"** : T106–T115, T202/T204/T206, T308–T310, T401–T405

## Definition of Done (ทุก task)

- `make gate` ผ่าน (fmt, clippy `-D warnings`, test, cargo audit)
- มี unit test หรือ e2e test ครอบ behavior ใหม่
- ถ้าแตะ hot path: แนบตัวเลข before/after จาก T001 harness
- ถ้าแตะ config schema: อัปเดต `Prx.toml`, `docs/CONFIG-WIKI.md`, และ type ฝั่ง WebUI
- ถ้าแตะ `webui/`: rebuild `webui/dist` (embed ตอน compile) และ commit มาด้วย

## Template ของ task ใหม่

ดู [_TEMPLATE.md](_TEMPLATE.md)
