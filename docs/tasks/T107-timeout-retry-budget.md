# T107 — Request timeout + retry budget

**Phase:** 1 · Data plane
**Status:** done
**Size:** M (~1d)
**Depends on:** —
**Files:** `src/config.rs`, `src/proxy.rs`, `src/runtime.rs`

## เป้าหมาย

กัน request ค้างและกัน retry storm ที่จะซ้ำเติม upstream ตอนมันเริ่มแย่

## สถานะปัจจุบัน

มี `max_retries` / `retry_backoff_ms` ต่อ service (`src/config.rs:210`) และ `should_retry()` (`src/proxy.rs:37`)
แต่:
- ไม่มี **total request timeout** (มีแค่ connect/read/write ต่อ upstream)
- retry ไม่มี budget รวม — ถ้า upstream ล่มพร้อมกัน ทุก request จะ retry เต็มโควต้า = โหลดเพิ่ม 3 เท่าตอนที่แย่ที่สุด
- ไม่มีเงื่อนไขว่า retry เฉพาะ method ที่ idempotent

## ขอบเขตงาน

1. `[[service]] request_timeout_ms` (นับตั้งแต่รับ request จนตอบเสร็จ รวมทุก retry) → 504 เมื่อเกิน
2. Retry budget แบบ ratio (เลียนแบบ gRPC/Envoy): `retry_budget_ratio = 0.1`
   = retry ได้ไม่เกิน 10% ของ request ที่สำเร็จในหน้าต่างเวลาล่าสุด; เกินแล้วหยุด retry ทันที
3. `retry_on = ["connect_failure", "5xx", "reset"]` และ `retry_idempotent_only = true` (default)
   — ห้าม retry POST อัตโนมัติถ้าผู้ใช้ไม่สั่ง
4. Jitter ใน backoff (ตอนนี้เป็น fixed backoff → เกิด thundering herd)
5. Metrics: `prx_retry_total{reason}`, `prx_retry_budget_exhausted_total`, `prx_request_timeout_total`

## Acceptance criteria

- [ ] e2e: upstream ตอบช้ากว่า `request_timeout_ms` → client ได้ 504 และ connection ถูกปิดอย่างถูกต้อง
- [ ] เทสต์ budget: ยิง 1000 req ที่ upstream ล่มทั้งหมด จำนวน retry จริงต้อง ≤ budget ไม่ใช่ 1000×max_retries
- [ ] POST ไม่ถูก retry เมื่อ `retry_idempotent_only = true`
- [ ] backoff มี jitter จริง (เทสต์ด้วย seed)

## Out of scope

- Hedged requests (ส่งซ้ำก่อน timeout) — follow-up

## ผลลัพธ์ที่ส่งมอบ

- `[[service]] request_timeout_ms` — งบเวลารวมทั้ง request นับจากที่ prx รับเข้ามา
  หมดแล้วคืน **504** ไม่เริ่ม attempt ใหม่ และ timeout ของแต่ละ attempt (connect/read/write)
  ถูกหดให้ไม่เกินเวลาที่เหลือ
- `retry_budget_ratio` / `retry_budget_min_per_window` / `retry_budget_window_ms` —
  budget แบบ sliding window (`RetryBudget` ใน `src/runtime.rs`) ใช้ atomic ล้วน ไม่มี lock
- `retry_idempotent_only` (default `true`) — แยกกรณี connect failure (ยังไม่ได้ส่งอะไร รีทรายได้ทุก method)
  ออกจาก failure กลางคัน (รีทรายเฉพาะ method ที่ idempotent)
- backoff มี jitter แบบ full jitter (`retry_backoff_ms` กลายเป็นเพดาน ไม่ใช่ค่าคงที่)
- metrics ใหม่: `prx_retry_total{route,reason}`, `prx_retry_denied_total{route,reason}`,
  `prx_request_timeout_total{route}`
- `ServiceConfig`/`RouteConfig` มี `Default` แล้ว — การเพิ่ม field ใหม่ไม่ต้องไล่แก้ constructor ทุกที่อีก

## บั๊กความปลอดภัยที่แก้ไปด้วย

เดิม `should_retry()` ไม่ดู method เลย **POST ที่ล้มเหลวกลางคันจึงถูกส่งซ้ำได้**
ถ้า upstream รับ request ไปแล้วแต่ตอบไม่ทัน ลูกค้าอาจถูกตัดเงินสองรอบ
ตอนนี้ default คือไม่ส่งซ้ำ และมี e2e ยืนยัน (POST ไปถึง upstream ครั้งเดียว ส่วน GET ถูก retry)

## Acceptance criteria

- [x] e2e: upstream ตอบช้ากว่า `request_timeout_ms` → client ได้ 504 (และจบภายในงบ ไม่รอ 5 วินาที)
- [x] เทสต์ budget: ยิงจนเกิน budget แล้วจำนวน attempt จริงต่ำกว่า `requests × (1 + max_retries)`
- [x] POST ไม่ถูก retry เมื่อ `retry_idempotent_only = true` (unit + e2e)
- [x] backoff มี jitter จริง

## หมายเหตุขอบเขต

- `retry_on = ["5xx", ...]` ยังไม่ได้ทำ — pingora คืน 5xx ของ upstream เป็น response ปกติ
  การ retry ตรงนั้นต้องแตะ response path ซึ่งควรทำคู่กับ [T109](T109-micro-cache.md)/[T110](T110-compression.md)
- `request_timeout_ms` ไม่ตัด response ที่กำลังไหลช้าๆ กลางคัน (ต้องรอ pingora เปิด hook)
  per-read timeout ยังคุมกรณีนั้นอยู่
