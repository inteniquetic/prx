# T105 — Upstream connection pool / keepalive / H2 multiplexing

**Phase:** 1 · Data plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T002
**Files:** `src/proxy.rs`, `src/config.rs`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

จุดที่ Pingora ชนะ nginx ชัดที่สุดคือ **connection pool ที่แชร์ข้ามทุก thread** ทำให้ reuse rate สูงกว่ามาก
ต้องเปิดใช้ให้ถูกและตั้งค่าได้ ไม่งั้นก็เสียเปรียบเปล่าๆ

## สถานะปัจจุบัน

`UpstreamConfig` (`src/config.rs:300`) มี `idle_timeout_ms` แต่ไม่มี pool size / reuse policy
`upstream_peer()` (`src/proxy.rs:222`) สร้าง `HttpPeer::new(...)` ใหม่ทุก request และตั้ง option ต่อ peer
ยังไม่มี metric ว่า connection reuse กี่ %

## ขอบเขตงาน

1. เพิ่มต่อ service/upstream:
   - `pool_idle_timeout_ms`, `max_idle_per_upstream`, `max_conns_per_upstream`
   - `upstream_h2: auto|always|never` (H2 ไป upstream = multiplex ลดจำนวน conn มาก)
   - `keepalive_requests` (รีไซเคิล conn หลัง N requests กัน connection เก่าค้าง)
2. ตรวจว่าค่าเหล่านี้ map ลง Pingora peer options ถูกจริง (อ่าน `vendor/pingora-core`) และ reuse เกิดขึ้นจริง
3. เพิ่ม metric: `prx_upstream_connection_reused_total`, `prx_upstream_connection_created_total`,
   `prx_upstream_pool_idle_connections` (gauge)
4. Bench: วัด reuse ratio ใน scenario `h1-keepalive` และ `h2` เทียบ nginx `keepalive N`

## Acceptance criteria

- [ ] reuse ratio ที่ `h1-keepalive` > 95% และเห็นค่าใน `/metrics`
- [ ] เปิด `upstream_h2 = "always"` แล้วจำนวน TCP conn ไป upstream ลดลงชัดเจนใน scenario เดียวกัน
- [ ] ค่า default ปลอดภัย (ไม่ทำให้ upstream ที่ไม่รองรับ h2 พัง — `auto` ต้อง fallback ได้)
- [ ] เอกสารอธิบายวิธีตั้งค่าเทียบเท่า `keepalive` ของ nginx

## Out of scope

- LB algorithm ใหม่ (T114)
