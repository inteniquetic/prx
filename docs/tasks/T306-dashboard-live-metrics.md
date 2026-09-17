# T306 — Dashboard สถิติสด

**Phase:** 3 · Web UI
**Status:** todo
**Size:** L (~2d)
**Depends on:** T303, T207
**Files:** `webui/src/lib/components/pages/DashboardPage.svelte`, `webui/src/lib/components/dashboard/*`

## เป้าหมาย

เปิดหน้าแรกแล้วรู้ทันทีว่าระบบปกติหรือไม่ และถ้าไม่ปกติ ปัญหาอยู่ที่ route/upstream ไหน

## สถานะปัจจุบัน

`DashboardPage` แสดงสรุป config (`ServerCard`, `StatsCard`, `RouteTableCard`, `ObservabilityCard`)
ซึ่งเป็นข้อมูล "ตั้งค่าอะไรไว้" ไม่ใช่ "ตอนนี้เกิดอะไรขึ้น" — ต้องเพิ่มข้อมูล runtime จาก T207

## ขอบเขตงาน

1. แถวบน `MetricTile`: RPS, p99 latency, error rate (4xx/5xx แยกกัน), active connections,
   upstream healthy / total, cache hit ratio (ถ้าเปิด T109) — แต่ละอันมี sparkline 5 นาทีและ delta เทียบ 1 นาทีก่อน
2. กราฟหลัก: RPS + error rate ตามเวลา และ latency percentile (p50/p95/p99) — ใช้ไลบรารีขนาดเล็ก
   (เช่น `layerchart`/`uPlot`) ที่ self-host ได้, ต้องไม่กระตุกตอนอัปเดตทุกวินาที
3. ตาราง "Top routes" เรียงตาม traffic และ "Worst routes" เรียงตาม error rate / p99
4. แผงสุขภาพ upstream: grid ของจุดสถานะ คลิกแล้วไปหน้า service นั้น
5. แถบเหตุการณ์ล่าสุด: config apply, reload สำเร็จ/ล้มเหลว, circuit breaker trip, cert ใกล้หมดอายุ
   (ดึงจาก audit ของ T206 + event ของ runtime)
6. สถานะ degraded ต้องเด่นชัด: แถบสีด้านบนพร้อมข้อความว่าเกิดอะไรและลิงก์ไปจุดที่ต้องแก้

## Acceptance criteria

- [ ] กราฟอัปเดตสดจาก SSE โดยไม่ memory leak เมื่อเปิดทิ้งไว้ 1 ชั่วโมง (ตรวจด้วย devtools)
- [ ] เมื่อ admin API หลุด UI แสดงสถานะ reconnecting ไม่ใช่กราฟค้างเงียบๆ
- [ ] หน้า render ครั้งแรกใน < 1s บนเครื่องธรรมดา
- [ ] ทุกตัวเลขมี tooltip อธิบายว่ามาจาก metric ไหน

## Out of scope

- แทนที่ Grafana สำหรับข้อมูลย้อนหลังยาว
