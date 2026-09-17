# T103 — เลิก `load_full()` ต่อ request

**Phase:** 1 · Data plane
**Status:** todo
**Size:** S (~0.5d)
**Depends on:** T101
**Files:** `src/proxy.rs`, `src/runtime.rs`

## เป้าหมาย

ลด atomic refcount churn ต่อ request — `ArcSwap::load_full()` ทำ `Arc::clone` (atomic increment/decrement)
ขณะที่ `load()` คืน `Guard` แบบ hazard-pointer ที่ถูกกว่ามากในเส้นทางที่อ่านอย่างเดียว

## สถานะปัจจุบัน (โค้ดจริง)

- `src/proxy.rs:170` `let snapshot = self.active_config.load_full();`
- `src/proxy.rs:171` แล้ว `.clone()` เก็บใน ctx อีกที
- `src/proxy.rs:228-231` `upstream_peer()` โหลด/clone ซ้ำอีกรอบ

รวมแล้วอย่างน้อย 3–4 atomic ops ต่อ request บน shared cacheline เดียวกันทุก core — เป็น contention ที่วัดได้ตอน core เยอะ

## ขอบเขตงาน

1. ใช้ `load()` + `Guard` ใน `request_filter` และส่ง `route_idx`/`service_idx` ต่อผ่าน ctx
2. เก็บ `Arc` ไว้ใน ctx **ครั้งเดียว** เฉพาะกรณีที่ phase หลัง (`upstream_peer`, `logging`) ต้องใช้จริง
   และต้องเป็น snapshot เดียวกันตลอด lifetime ของ request (ห้ามโหลดใหม่กลาง request — ไม่งั้น reload กลางคันจะได้ route กับ upstream คนละเวอร์ชัน)
3. เขียน comment อธิบายสัญญานี้ไว้บน `RequestCtx`
4. วัด contention ด้วย bench `many-conns` + core เยอะ (T001)

## Acceptance criteria

- [ ] มี snapshot เดียวต่อ request ยืนยันด้วยเทสต์: reload ระหว่าง request ไม่ทำให้ route/service ไม่ match กัน
- [ ] `h1-keepalive` ที่ 8 cores: RPS ดีขึ้นหรืออย่างน้อยเท่าเดิม, CPU per request ลดลง
- [ ] `make gate` ผ่าน

## Out of scope

- เปลี่ยนกลไก reload (ดู T204)
