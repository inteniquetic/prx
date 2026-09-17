# T207 — Live stats stream (SSE) สำหรับ dashboard

**Phase:** 2 · Control plane
**Status:** todo
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

## Acceptance criteria

- [ ] เปิด 5 tab พร้อมกัน → CPU ของ admin ไม่เกินเพดานที่ตั้ง และ proxy RPS ไม่ตก (วัดด้วย bench)
- [ ] ปิด tab → task ฝั่ง server ถูกเก็บกวาดจริง (เทสต์นับ task)
- [ ] ค่าที่ stream ตรงกับ `/metrics` ในช่วงเวลาเดียวกัน
- [ ] client reconnect ได้เอง (`Last-Event-ID` หรือ retry hint)

## Out of scope

- เก็บ metric ระยะยาว (นั่นคืองานของ Prometheus)
