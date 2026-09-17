# T107 — Request timeout + retry budget

**Phase:** 1 · Data plane
**Status:** todo
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
