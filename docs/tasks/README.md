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
| 0 | Measurement | วัดได้ก่อน ค่อยปรับ | 4 |
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

### Phase 1 — Data plane

| ID | Task | Size | Depends |
|---|---|---|---|
| [T101](T101-zero-alloc-request-path.md) | ตัด allocation ต่อ request ใน `request_filter` | M | T003 |
| [T102](T102-route-matcher-index.md) | เปลี่ยน route matching จาก linear scan เป็น host map + path trie | L | T003 |
| [T103](T103-config-snapshot-access.md) | เลิก `load_full()` ต่อ request | S | T101 |
| [T104](T104-socket-worker-tuning.md) | SO_REUSEPORT, tcp_nodelay, backlog, worker/threads | M | T002 |
| [T105](T105-upstream-pool-keepalive.md) | Upstream connection pool / keepalive / H2 multiplexing | M | T002 |
| [T106](T106-header-rewrite-rules.md) | Header add/remove/set ต่อ route (precompiled) | M | T102 |
| [T107](T107-timeout-retry-budget.md) | Request timeout + retry budget กัน retry storm | M | — |
| [T108](T108-rate-limit.md) | Rate limit + connection limit ต่อ route/IP | L | T102 |
| [T109](T109-micro-cache.md) | In-memory micro-cache สำหรับ GET | L | T102 |
| [T110](T110-compression.md) | gzip/brotli response compression | M | T106 |
| [T111](T111-tls-perf-multicert.md) | TLS: session resumption, ALPN, multi-cert SNI | L | T104 |
| [T112](T112-acme-auto-tls.md) | ACME auto TLS (Let's Encrypt) | L | T111 |
| [T113](T113-active-health-check.md) | Active health check prober (เสริม passive CB) | M | — |
| [T114](T114-lb-least-conn-sticky.md) | LB: least_conn, P2C-EWMA, sticky session | M | T105 |
| [T115](T115-ws-grpc-conformance.md) | เทสต์จริงของ WebSocket + gRPC streaming | M | — |

### Phase 2 — Control plane

| ID | Task | Size | Depends |
|---|---|---|---|
| [T201](T201-admin-auth-hardening.md) | Auth + bind loopback + CORS/CSRF ของ Admin API | M | — |
| [T202](T202-config-history-rollback.md) | เก็บประวัติ config + diff + rollback | M | T201 |
| [T203](T203-validate-dry-run-api.md) | `POST /web/config/validate` คืน error แบบมีพิกัด | M | — |
| [T204](T204-atomic-apply-reload.md) | Apply แบบ atomic + auto-rollback ถ้า reload ไม่ผ่าน | M | T202, T203 |
| [T205](T205-openapi-typed-client.md) | OpenAPI + JSON Schema + typed client ให้ WebUI | M | T203 |
| [T206](T206-audit-log.md) | Audit log ใครแก้อะไรเมื่อไหร่ | S | T201 |
| [T207](T207-live-stats-stream.md) | SSE stream สถิติสดให้ dashboard | M | T401 |

### Phase 3 — Web UI (Svelte 5 + shadcn-svelte)

| ID | Task | Size | Depends |
|---|---|---|---|
| [T301](T301-webui-svelte5-shadcn-foundation.md) | อัป Svelte 5 + ติดตั้ง shadcn-svelte + design token | L | — |
| [T302](T302-design-system-components.md) | ชุด component พื้นฐาน + dark mode | M | T301 |
| [T303](T303-app-shell-navigation.md) | App shell: sidebar, topbar, breadcrumb, command palette | M | T302 |
| [T304](T304-routes-crud-ui.md) | หน้า Routes: data table + form validation | L | T303, T205 |
| [T305](T305-services-upstreams-ui.md) | หน้า Services/Upstreams + health badge | L | T304 |
| [T306](T306-dashboard-live-metrics.md) | Dashboard สถิติสด + กราฟ | L | T303, T207 |
| [T307](T307-toml-editor-diff-apply.md) | TOML editor + diff ก่อน apply | M | T303, T203 |
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
| wildcard host matching จัดสรรหน่วยความจำต่อ route ต่อ request | 1000 wildcard routes = **59.3 µs/request** (เพดาน ~16.8k rps/core) | [T101](T101-zero-alloc-request-path.md), [T102](T102-route-matcher-index.md) |
| route matching เป็นเชิงเส้นตามจำนวน route | 1000 exact routes = 3.1–4.3 µs/request | [T102](T102-route-matcher-index.md) |
| `normalize_host` จัดสรร String ทุก request | 43 ns (68 ns เมื่อมี port) | [T101](T101-zero-alloc-request-path.md) |
| reload ไม่ใช่ปัญหา | `RuntimeConfig::from_config` ที่ 1000 routes = 1.32 ms (งบ 5 ms) | — |

ลำดับความสำคัญจึงชัดแล้ว: **T101 + T102 คืองานถัดไป** ไม่ใช่ฟีเจอร์ใหม่

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
