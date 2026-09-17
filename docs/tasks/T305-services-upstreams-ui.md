# T305 — หน้า Services & Upstreams

**Phase:** 3 · Web UI
**Status:** todo
**Size:** L (~2d)
**Depends on:** T304
**Files:** `webui/src/lib/components/pages/ServicesPage.svelte`, `webui/src/lib/components/services/*`

## เป้าหมาย

จัดการ backend pool ได้ครบ: เพิ่ม/ถอด upstream, ปรับ weight, ดูสุขภาพ, ดู circuit breaker แบบเห็นภาพ

## ขอบเขตงาน

1. Service card/table: ชื่อ, LB strategy, จำนวน upstream (healthy/total), retry setting, CB state, route ที่ใช้ service นี้
2. Upstream editor ในแต่ละ service:
   - ตาราง addr, tls, sni, weight, timeouts, สถานะสุขภาพสด, latency ล่าสุด (จาก T113/T114)
   - ปรับ weight ด้วย slider พร้อมแสดงสัดส่วนทราฟฟิกที่คาดว่าจะได้เป็น %
   - ปุ่ม "ทดสอบการเชื่อมต่อ" ต่อ upstream (ใช้ `POST /web/health/routes` ที่มีอยู่ หรือ endpoint เฉพาะ)
   - ปุ่ม drain (ตั้ง weight 0 / ถอดชั่วคราว) โดยไม่ลบ config
3. แสดง circuit breaker แบบเห็นภาพ: closed / open (นับถอยหลังเวลาที่เหลือ) / half-open พร้อมเหตุผลล่าสุด
4. Form ของ service: LB strategy (อธิบายแต่ละแบบสั้นๆ ให้คนเลือกถูก), max_retries, retry budget (T107),
   health check (T113), pool settings (T105)
5. เตือนเมื่อจะลบ service ที่ยังมี route อ้างอยู่ (บอกว่า route ไหนบ้าง และห้ามลบจนกว่าจะย้าย)

## Acceptance criteria

- [ ] เพิ่ม/ลบ upstream แล้ว traffic เปลี่ยนตามจริง (ทดสอบกับ backend จำลอง)
- [ ] ลบ service ที่มี route อ้างอยู่ → ถูกบล็อกพร้อมรายการ route ที่กระทบ
- [ ] สถานะสุขภาพอัปเดตสดโดยไม่ต้องรีเฟรชหน้า
- [ ] slider weight แสดงสัดส่วนตรงกับที่ LB ทำจริง

## Out of scope

- กราฟ latency ย้อนหลังต่อ upstream (T306)
