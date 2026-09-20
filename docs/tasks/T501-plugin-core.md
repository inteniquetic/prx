# T501 — Plugin core: trait, registry, chain

**Phase:** 5 · Plugin & WAF
**Status:** ✅ done
**Size:** L (~2d)
**Depends on:** —
**Files:** `src/plugin/{mod,registry}.rs`, `src/plugin/builtin/echo_header.rs`, `src/proxy.rs`,
`src/runtime.rs`, `src/config.rs`, `src/config_edit.rs`, `src/admin.rs`, `src/metrics.rs`,
`tests/e2e_plugin.rs`, `webui/src/lib/{types/config.ts,configNormalize.ts,configCodec.ts}`,
`Prx.toml`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

มีที่ทางให้เสียบความสามารถใหม่เข้า request path โดยไม่ต้องแก้ `request_filter` อีก
และโดยที่ route ที่ไม่ได้เปิด plugin ไม่จ่ายอะไรเลย

## สถานะปัจจุบัน

`request_filter` ยาว ~160 บรรทัด ทำ 6 อย่างเรียงกันแบบ hardcode: ACME challenge, health, ready,
route match, rate limit, concurrency, cache lookup ส่วน `response_filter` ทำ sticky cookie,
cache store, response header ลำดับทั้งหมดถูกกำหนดโดยลำดับของ `if` ในไฟล์

`RouteRuntime` มี `request_headers`, `response_headers`, `cache`, `rate_limit`, `concurrency`
เป็นฟิลด์ตายตัว การเพิ่มความสามารถที่เจ็ดแปลว่าแก้ 4 ไฟล์ทุกครั้ง

## ขอบเขตงาน

1. `PluginDecision { Continue, Respond { status, body } }` และ trait `Plugin` ที่มี default impl
   ทุก phase เพื่อให้ plugin เขียนเฉพาะ phase ที่สนใจ
2. Phase ตาม §4.1 ของ [`docs/PLUGIN-WAF-PLAN.md`](../PLUGIN-WAF-PLAN.md) — รอบนี้ทำเฉพาะ phase
   ที่ pingora มี hook อยู่แล้ว (`on_request_head`, `on_upstream_request`, `on_response_head`,
   `on_response_body`, `on_log`) ส่วน `on_request_body` เป็นของ T503
3. `PhaseMask` (bitset) ต่อ plugin และ OR รวมเป็นของ chain — `request_filter` เช็คบิตก่อนแตะเชน
4. `PluginChain` คอมไพล์ใน `RuntimeConfig::from_config` เก็บเป็น `Box<[Arc<dyn Plugin>]>`
   ใน `RouteRuntime` แบบเดียวกับที่ `CompiledHeaderRules` ทำอยู่
5. Registry: `kind` ใน config → constructor ที่คืน `Arc<dyn Plugin>` และรายงาน error
   แบบมีพิกัดบรรทัดผ่าน `validate.rs` ได้ (kind ที่ไม่รู้จักต้องเป็น error ตอน validate ไม่ใช่ตอน boot)
6. `PluginState` — slab ต่อ request ใน `RequestCtx`, เป็น `None` จนกว่าจะมี plugin ที่ประกาศว่าต้องใช้
7. Config: `[[plugin]]` ระดับ global + `plugins = [...]` ต่อ route ตาม §4.3 พร้อม
   `Prx.toml`, `CONFIG-WIKI.md`, type ฝั่ง WebUI, และ path ของ `config_edit.rs`
8. Plugin ตัวอย่างหนึ่งตัวที่ไม่มีประโยชน์แต่ทดสอบได้ (`kind = "echo-header"`) เพื่อให้ test
   ไม่ต้องรอ WAF

## Acceptance criteria

- [x] route ที่ไม่มี plugin: `benches/routing.rs` ไม่ถอยเกิน noise ของ harness —
      รัน before/after แล้วได้ผลกระจายทั้งสองทาง (8 เร็วขึ้น, 2 ช้าลง) จึงวัด **noise floor**
      ด้วยการรันซ้ำโดยไม่แก้โค้ดเลย: ได้ −11.5% ถึง +3.3% ซึ่งกว้างกว่าผลของการเปลี่ยนแปลงเอง
      แปลว่าตัวเลขที่เห็นอยู่ในระดับสัญญาณรบกวน และตรงกับที่ควรเป็น เพราะ `select_route`
      เดินผ่าน `RouteIndex` ไม่เคยแตะ `RouteRuntime.plugins`
- [x] ~~`RequestCtx` ไม่โตขึ้น~~ **เกณฑ์นี้เขียนผิดตั้งแต่แรก** — `size_of` เป็นค่าคงที่ต่อชนิด
      วัดต่อ request ไม่ได้ และ `Option<Box<_>>` ย่อมกินอีก 8 ไบต์เสมอ สิ่งที่ทำได้จริงและทำแล้วคือ
      **ไม่มีการจัดสรรหน่วยความจำ** สำหรับ request ที่ไม่เจอ plugin: `plugin_state` เป็น `None`
      ตลอด และเชนที่ไม่มีใครขอ state ก็ใช้ช่องบน stack แทน slab
      (`the_plugin_system_costs_a_request_three_empty_fields`,
      `state_is_not_allocated_when_no_plugin_wants_any`)
- [x] plugin ที่คืน `Respond` หยุดเชนและ request จบตรงนั้น โดย `logging()` ยังทำงาน —
      `a_plugin_can_answer_the_request_itself` ยืนยันว่า upstream ไม่เคยเห็น request นั้น
- [x] ลำดับใน `plugins = [...]` คือลำดับการทำงานจริง — `plugins_run_in_the_order_the_route_lists_them`
      จงใจสลับลำดับใน route ให้ต่างจากลำดับของ `[[plugin]]` เพื่อไม่ให้เทสต์ผ่านได้ด้วยความบังเอิญ
- [x] `kind` ที่ไม่รู้จัก → validate error ที่ `plugin[i].kind` พร้อม hint บอก kind ที่มี
- [x] reload เปลี่ยนชุด plugin ได้ — เชนอยู่ใน `RuntimeConfig` ที่ `ArcSwap` สลับทั้งก้อน และ
      request ที่วิ่งอยู่ถือ snapshot เดิมไว้ตาม T103 (`request_keeps_its_snapshot_across_a_reload`)

## วิธีทดสอบ

- `cargo test` — 20 เทสต์ใหม่: chain/phase mask/state (7), validation (8), การคอมไพล์เชน (5)
- `tests/e2e_plugin.rs` — 5 เทสต์ผ่านไบนารีจริง: plugin ถึง upstream, ลำดับ, การตอบแทน upstream,
  response phase, และ route ที่ไม่ได้ลิสต์ plugin ต้องไม่โดนแตะ
- `cargo bench --bench routing` — before/after บวกการวัด noise floor
- WebUI encoder → parser จริง: สร้าง TOML จาก `encodeToml` แล้วให้ `PrxConfig::from_toml_str` อ่าน
  ได้ `[[plugin]]` และ `route.plugins` ครบ (บั๊ก field หายที่โปรเจกต์นี้เจอมาแล้วสองรอบ)

## ที่ตัดสินใจต่างจากที่ task เขียนไว้

- **`on_response_head` คืน `PluginDecision` ด้วย ไม่ใช่แค่ `Result<()>`** — ตอนแรกตั้งใจให้
  เฉพาะ request phase บล็อกได้ แต่นั่นคือกับดักที่เขียนเตือนตัวเองไว้ใน `PLUGIN-WAF-PLAN.md`
  พอดี: WAF ต้องตรวจ response ได้ (data leak, outbound rule ของ CRS) ถ้าปล่อยไว้ API
  จะถูกดัดให้เข้ากับ WAF ทีหลังแทนที่จะออกแบบให้ครบตั้งแต่แรก จึงทำให้ครบเลย —
  plugin ที่ตอบจาก response phase จะแทน status + body และ body ของ upstream ถูกทิ้ง
- **response ที่ plugin แทน จะไม่ถูก cache** — `ctx.cache_pending` ถูกล้างทิ้ง เพราะของที่
  upstream ไม่ได้ผลิตไม่ควรถูกเก็บเหมือนว่ามันผลิต
- **`[[plugin]]` หนึ่งบล็อก = instance เดียว** ที่ทุก route ที่เรียกชื่อมันแชร์กัน ไม่ใช่ก๊อปปี้ต่อ route
  — สำคัญกับ plugin ที่ถือ limiter หรือ ruleset ที่คอมไพล์แล้ว
- **`metrics::inc_plugin_response`** แยกจาก `prx_rate_limited_total` เพราะ WAF block กับ
  rate limit เป็นคนละคำถามตอนไล่ปัญหา

## Out of scope

- `on_request_body` (T503)
- ย้ายของเดิมมาเป็น plugin (T502) — รอบนี้ของเดิมยังอยู่ที่เดิม เชนแค่ถูกเรียกเพิ่ม
- WASM (T510)
