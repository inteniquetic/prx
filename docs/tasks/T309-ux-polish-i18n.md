# T309 — UX polish + i18n (ไทย/อังกฤษ)

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T304, T305
**Files:** `webui/src/lib/i18n/*`, ทุกหน้า

## เป้าหมาย

ทำให้ "ใช้ง่าย" เป็นจริง ไม่ใช่แค่สวย — คนที่ไม่เคยเขียน config ของ nginx ก็ต้องตั้ง route ได้

## ขอบเขตงาน

1. i18n: `th` / `en` สลับได้ เก็บ preference ไว้ ตรวจว่าไม่มีข้อความ hardcode เหลือ
   (แปล error code จาก T203 เป็นข้อความที่คนอ่านรู้เรื่อง)
2. Empty state ทุกหน้า: ยังไม่มี route/service → อธิบายและมีปุ่มสร้างพร้อม template ตัวอย่าง
   (จาก `docs/CONFIG-PLAYBOOK.md`: single backend, blue-green, path-based API gateway)
3. Loading: skeleton แทน spinner, optimistic update สำหรับ action เล็ก, ปุ่มมีสถานะ loading กันกดซ้ำ
4. Error: toast (sonner) + inline error ที่ฟิลด์ + หน้า error ที่บอกวิธีแก้ ไม่ใช่แค่ stack trace
5. Onboarding: wizard 3 ขั้นตอนตอนเปิดครั้งแรกกับ config เปล่า (ตั้งชื่อ service → ใส่ upstream → ผูก route) แล้ว apply
6. A11y: ผ่าน axe โดยไม่มี critical issue, focus trap ใน dialog ถูกต้อง, `aria-live` สำหรับสถานะที่อัปเดตเอง,
   contrast ผ่าน AA ทั้ง 2 ธีม
7. ตรวจ copy ทั้งหมดให้เป็นภาษาที่คนทั่วไปเข้าใจ (เช่น อธิบาย "circuit breaker" ด้วยประโยคเดียวใน tooltip)

## Acceptance criteria

- [ ] สลับภาษาแล้วไม่มีข้อความตกหล่น (มีเทสต์ตรวจ key ที่ขาด)
- [ ] axe-core ไม่มี critical/serious violation
- [ ] Wizard: จาก config เปล่า ตั้งค่าจนทราฟฟิกวิ่งได้ใน < 2 นาที (ทดสอบกับคนที่ไม่เคยใช้ prx)
- [ ] ไม่มี layout shift ตอนโหลดข้อมูล

## Out of scope

- ภาษาที่สามขึ้นไป
