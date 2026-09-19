# T207 — Live stats stream (SSE) สำหรับ dashboard

**Phase:** 2 · Control plane
**Status:** done (ทำพร้อม T306 เพราะ dashboard ต้องใช้)
**Size:** M (~1d)
**Depends on:** T401
**Files:** `src/admin.rs`, `src/stats.rs` (ใหม่)

## เป้าหมาย

ให้ dashboard เห็นสถานะสดโดยไม่ต้อง polling ถี่ๆ และไม่ต้องบังคับให้ผู้ใช้ติดตั้ง Prometheus ก่อนถึงจะเห็นอะไร

## สถานะปัจจุบัน

UI ดึง `/web/config` และเรียก `/web/health/routes` ซึ่งเปิด TCP connect สดทุกครั้ง (`src/admin.rs:502`)
— แพงและไม่ใช่ข้อมูลเชิงเวลา ไม่มีทางดู RPS/latency จาก UI ได้เลย

## ขอบเขตงาน

1. `src/stats.rs`: ring buffer ในหน่วยความจำ เก็บค่าราย 1 วินาที ย้อนหลัง 5 นาที
   (rps, error rate แยก 4xx/5xx, p50/p95/p99 จาก histogram, active connections, upstream healthy count, cache hit ratio)
   — อ่านจาก metric registry ที่มีอยู่ (`src/metrics.rs`) ไม่ใช่นับซ้ำใน hot path
2. `GET /web/stats` → snapshot ล่าสุด + ประวัติ 5 นาที (ใช้ตอนเปิดหน้าครั้งแรก)
3. `GET /web/stats/stream` → SSE push ทุก 1 วินาที (มี heartbeat, ปิด connection สะอาดตอน client หาย)
4. จำกัดจำนวน SSE client พร้อมกัน (เช่น 16) กัน admin API กลายเป็นภาระ
5. ต้องไม่เพิ่ม allocation หรือ lock ใน request path ของ proxy

## ผลลัพธ์ที่ส่งมอบ

ดูรายละเอียดใน [T306](T306-dashboard-live-metrics.md) — งานนี้เป็น commit แรกของก้อนนั้น
(`src/stats.rs`, `GET /web/stats`, `GET /web/stats/stream`, `prx_inflight_requests`
และการแก้ bucket ของ `prx_request_latency_ms`)

## Acceptance criteria

- [x] เปิดหลาย tab พร้อมกัน → ทุก client อ่าน broadcast เดียวกัน ไม่มี task ต่อ client
      และ sampler ทำงานวินาทีละครั้งไม่ว่าจะมีคนดูกี่คน ส่วน request path เพิ่มแค่
      atomic เดียวต่อ request (ยังไม่ได้รัน bench เทียบ RPS ก่อน/หลัง — ควรทำใน T401
      ตอนวัด cost ของ metric ทั้งชุด)
- [x] ปิด tab → slot ถูกคืนจริง (guard อยู่ใน response body) `e2e_stats` เปิด 16 stream
      ปิดทั้งหมด แล้วยืนยันว่า `stream_clients` กลับเป็น 0 และเปิดใหม่ได้
- [x] ค่าที่ stream ตรงกับ `/metrics` ในช่วงเวลาเดียวกัน — มาจาก registry เดียวกัน
      และ `e2e_stats` ยิงทราฟฟิกจริงแล้วเทียบจำนวน request ของทั้งสองทาง
- [x] client reconnect ได้เอง — SSE ส่ง `retry:` hint ตั้งแต่เฟรมแรก, event มี `id:`,
      และเมื่อ client ตามไม่ทันจะได้ event `lagged` ให้ไปดึง snapshot ใหม่แทนที่จะปล่อยให้กราฟโหว่

## Out of scope

- เก็บ metric ระยะยาว (นั่นคืองานของ Prometheus)
