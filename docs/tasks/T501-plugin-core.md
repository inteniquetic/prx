# T501 — Plugin core: trait, registry, chain

**Phase:** 5 · Plugin & WAF
**Status:** todo
**Size:** L (~2d)
**Depends on:** —
**Files:** `src/plugin/mod.rs`, `src/plugin/registry.rs`, `src/proxy.rs`, `src/runtime.rs`, `src/config.rs`, `benches/routing.rs`

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

- [ ] route ที่ไม่มี plugin: `benches/routing.rs` ไม่ถอยเกิน noise ของ harness (แนบ before/after)
- [ ] `RequestCtx` ไม่โตขึ้นสำหรับ request ที่ไม่มี plugin (ยืนยันด้วย `size_of`)
- [ ] plugin ที่คืน `Respond` หยุดเชนและ request จบตรงนั้น โดย `logging()` ยังทำงาน
- [ ] ลำดับใน `plugins = [...]` คือลำดับการทำงานจริง (เทสต์ด้วย plugin ที่ append header คนละตัว)
- [ ] `kind` ที่ไม่รู้จัก → validate error พร้อมเลขบรรทัด ไม่ใช่ panic ตอน boot
- [ ] reload เปลี่ยนชุด plugin ได้โดยไม่ drop connection และ request ที่ค้างอยู่ยังใช้เชนเดิม

## วิธีทดสอบ

- `cargo test` — เชนว่าง, เชนที่มีหลายตัว, การหยุดกลางเชน, state ที่ต้องคืนตอน `on_log`
- `cargo bench --bench routing` — before/after
- `scripts/bench.sh` — ยืนยันว่า RPS/p99 ของ config เดิมไม่ถอย

## Out of scope

- `on_request_body` (T503)
- ย้ายของเดิมมาเป็น plugin (T502) — รอบนี้ของเดิมยังอยู่ที่เดิม เชนแค่ถูกเรียกเพิ่ม
- WASM (T510)
