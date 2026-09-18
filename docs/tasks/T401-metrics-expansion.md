# T401 — Metrics ต่อ route/upstream + histogram

**Phase:** 4 · Observability & Ops
**Status:** todo
**Size:** M (~1d)
**Depends on:** —
**Files:** `src/metrics.rs`, `src/proxy.rs`, `ops/alerts/prx-alerts.yml`

## เป้าหมาย

มี metric พอที่จะตอบได้ว่า "ช้าที่ route ไหน upstream ตัวไหน" โดยไม่ต้องเดา และเป็นฐานของ dashboard (T207/T306)

## สถานะปัจจุบัน

`src/metrics.rs` (~80 บรรทัด) มี metric พื้นฐานอยู่บ้าง — ต้องสำรวจว่ามีอะไรแล้ว และเติมส่วนที่ขาด
โดยไม่ทำให้ cardinality ระเบิด

## บั๊กที่ต้องแก้ในงานนี้ (เจอตอนทำ T101)

`logging()` (`src/proxy.rs`) เริ่มด้วย `if !self.access_log { return; }` แต่ `metrics::observe_request`
ถูกเรียก **หลัง** บรรทัดนั้น → ตั้ง `access_log = false` (ซึ่งเป็นค่าที่ใช้ตอน benchmark และที่หลายคนใช้ใน production)
แล้ว metric ของ request จะไม่ถูกบันทึกเลย ทั้งที่ `/metrics` ยังเปิดอยู่
ต้องแยกการบันทึก metric ออกจากเงื่อนไขของ access log

## ขอบเขตงาน

1. Metric ที่ควรมี:
   - `prx_requests_total{route,method,status_class}` (ใช้ status_class `2xx/3xx/4xx/5xx` ไม่ใช่ status เต็ม)
   - `prx_request_duration_seconds{route}` (histogram, bucket ที่เหมาะกับ proxy: 1ms–10s)
   - `prx_upstream_duration_seconds{service}` (แยก time-to-first-byte กับ total ถ้าทำได้)
   - `prx_upstream_requests_total{service,addr,result}`
   - `prx_active_connections`, `prx_connections_total{state}`
   - `prx_config_reload_total{result}`, `prx_config_reload_duration_seconds`, `prx_config_version` (gauge = timestamp)
   - `prx_circuit_breaker_state{service,addr}`
2. คุม cardinality: route/service label ต้องมาจากชื่อที่ config ไว้เท่านั้น (ห้ามใช้ path ดิบ)
   มี `max_label_values` และ fallback เป็น `"other"` เมื่อเกิน
3. ต้นทุน: ใช้ counter ที่ resolve label ไว้ล่วงหน้าตอน build runtime (เก็บ handle ใน `RouteRuntime`)
   ไม่ใช่ lookup ด้วย string ทุก request
4. อัปเดต `ops/alerts/prx-alerts.yml` + `ops/SLO.md` ให้ใช้ metric ชุดใหม่
5. เอกสาร `docs/METRICS.md`: รายการ metric, ความหมาย, ตัวอย่าง PromQL

## Acceptance criteria

- [ ] bench: เปิด metric ครบแล้ว RPS ตกไม่เกิน 2% เทียบตอนปิด
- [ ] ที่ 1000 routes จำนวน time series ไม่เกินเพดานที่ระบุใน `docs/METRICS.md`
- [ ] `/metrics` scrape ได้ < 100ms
- [ ] alert rule ผ่าน `promtool check rules`

## Out of scope

- Exemplar / tracing linkage (T402)
