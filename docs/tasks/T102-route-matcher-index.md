# T102 — Route matching: host map + path trie แทน linear scan

**Phase:** 1 · Data plane
**Status:** done
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

## ผลลัพธ์ที่ส่งมอบ

- `src/router.rs`: exact host map (FxHash) → wildcard suffix map → any-host bucket,
  แต่ละ bucket เป็น radix trie ของ path prefix เก็บ node ไว้ใน `Vec` เดียว
- wildcard lookup ไล่ตัด label ทีละชั้น (`a.b.example.com` → `b.example.com` → `example.com` → `com`)
  = hash ตามจำนวน label ไม่ขึ้นกับจำนวน route
- method matching ด้วย bitmask + คืน 405 เมื่อ path ตรงแต่ method ไม่ตรง
- `config.rs` validate ชื่อ method (เดิมใส่ค่าที่ไม่มีอยู่จริงได้ แล้ว route นั้นจะไม่มีวันถูก match)
- routes ไม่ถูก sort แล้ว — index เป็นตัวกำหนดลำดับความสำคัญ ทำให้ index ของ route ตรงกับลำดับในไฟล์
- กติกาการ match เขียนไว้ทั้งใน rustdoc ของ `src/router.rs` และ `docs/CONFIG-WIKI.md`

## ผลวัด (median, container 4 vCPU)

| เคส @ 1000 routes | ก่อน | หลัง | เร็วขึ้น |
|---|---|---|---|
| route แรก | 3,111 ns | 67.8 ns | 45.9× |
| route สุดท้าย | 4,268 ns | 58.0 ns | 73.6× |
| default fallback | 3,143 ns | 55.2 ns | 56.9× |
| host มี port | 3,148 ns | 78.6 ns | 40.1× |
| wildcard | 59,341 ns | 81.9 ns | 724× |

เวลาที่ใช้คงที่ 55–84 ns ตั้งแต่ 10 ถึง 1000 routes

## Acceptance criteria

- [x] ที่ 1000 routes เร็วกว่าเดิม ≥ 10× (ได้ 40–724×)
- [~] ไม่แย่ลงที่ 10 routes — **ไม่ผ่านบางเคส**: "route แรกจาก 10" 52 ns → 56–68 ns
      เพราะต้อง hash host ก่อนแทนที่จะเทียบ route แรกแล้วจบทันที
      เคสอื่นที่ 10 routes เร็วขึ้นหมด (route สุดท้าย 1.7×, default 1.5×, wildcard 8.2×)
      ตัดสินใจไม่เพิ่ม code path พิเศษสำหรับตารางเล็กเพื่อไล่ ~10 ns
- [x] เทสต์ครอบ exact ชนะ wildcard, wildcard ยาวชนะสั้น, longest path prefix, default fallback, method mismatch → 405
      (15 unit tests ใน `src/router.rs` + 2 e2e ใน `tests/e2e_proxy.rs`)
- [x] build index ที่ 1000 routes < 5 ms — วัดได้ 1.91 ms (เดิม 1.32 ms, แพงขึ้น 0.59 ms)
- [x] เทสต์เดิมทั้งหมดยังผ่าน

## หมายเหตุพฤติกรรมที่เปลี่ยน

1. `host = ""` เดิมทำให้ route นั้นไม่มีวันถูก match (เทียบ `"" == host` เสมอเป็นเท็จ)
   ตอนนี้แปลว่า "ทุก host" เหมือนการไม่ใส่ `host` เลย
2. `methods` เดิมถูกประกาศได้แต่ไม่เคยถูกใช้ match — ตอนนี้ใช้จริงและคืน 405
3. method mismatch จะไม่ตกไป route ที่กว้างกว่าหรือ default route เพราะจะเป็นการพา request
   ข้ามข้อจำกัดที่ผู้ใช้ตั้งไว้
