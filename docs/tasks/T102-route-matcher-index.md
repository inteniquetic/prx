# T102 — Route matching: host map + path trie แทน linear scan

**Phase:** 1 · Data plane
**Status:** todo
**Size:** L (~2d)
**Depends on:** T003
**Files:** `src/runtime.rs`, `src/router.rs` (ใหม่), `benches/routing.rs`

## เป้าหมาย

ทำให้เวลา match route เป็น O(log n) หรือ O(len(path)) ไม่ใช่ O(จำนวน route)
เพราะจุดขายของ prx คือ "ตั้ง route เยอะๆ ผ่าน Web UI" — ถ้ามี 500 routes แล้วช้าลง 500 เท่า แผนนี้พัง

## สถานะปัจจุบัน (โค้ดจริง)

`src/runtime.rs:56` `select_route()` วน `for (idx, route) in self.routes.iter().enumerate()` ทุก request
แต่ละรอบเรียก `matches_host()` (`src/runtime.rs:138`, มี `format!` ตอน wildcard)
แล้ว `path.starts_with(&route.path_prefix)` → worst case O(routes × path_len)
และยังไม่รองรับ `methods` ที่ประกาศไว้ใน `RouteConfig` (`src/config.rs:238`) แต่ไม่ถูกใช้ match เลย

## ขอบเขตงาน

1. สร้าง `src/router.rs` เก็บโครงสร้าง index ที่ build ครั้งเดียวตอน reload:
   - `exact_hosts: HashMap<Box<str>, HostBucket>` (ใช้ hasher เร็ว เช่น `rustc-hash`/`ahash`)
   - `wildcard_hosts: Vec<(Box<str> /* ".example.com" */, HostBucket)>` เรียงตามความยาว suffix มาก→น้อย
   - `any_host: HostBucket` สำหรับ route ที่ไม่ระบุ host
   - แต่ละ `HostBucket` มี path prefix trie (radix) → คืน route ที่ prefix ยาวสุดที่ match
2. กติกาความชัดเจนของลำดับ (ต้องเขียนเป็นเอกสาร + เทสต์):
   exact host > wildcard host (suffix ยาวกว่าชนะ) > any host; ภายใน host เดียวกัน path prefix ยาวกว่าชนะ;
   เสมอกันใช้ลำดับในไฟล์; `is_default` เป็น fallback สุดท้าย
3. รองรับ `methods` ใน match (ว่าง = ทุก method) และคืน 405 เมื่อ path ตรงแต่ method ไม่ตรง
4. Bench: 10 / 100 / 1000 routes เทียบของเดิม

## Acceptance criteria

- [ ] `select_route` ที่ 1000 routes เร็วกว่าเดิม ≥ 10× และไม่แย่ลงที่ 10 routes
- [ ] เทสต์ครอบ: exact ชนะ wildcard, wildcard ยาวชนะสั้น, longest path prefix ชนะ, default fallback, method mismatch → 405
- [ ] เวลา build index ของ 1000 routes < 5ms (สำคัญเพราะ reload ทุกครั้งที่ผู้ใช้กด Save ใน UI)
- [ ] เทสต์เดิมทั้งหมดใน `src/runtime.rs` ยังผ่าน

## Out of scope

- regex / path parameter matching (บันทึกเป็น follow-up ถ้าจำเป็น)
