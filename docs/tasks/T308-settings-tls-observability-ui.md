# T308 — หน้า Settings: server / TLS / observability

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T303
**Files:** `webui/src/lib/components/pages/SettingsPage.svelte`

## เป้าหมาย

ตั้งค่าที่เหลือทั้งหมดได้จาก UI โดยไม่ต้องจำชื่อคีย์ใน TOML

## ขอบเขตงาน

1. Tab **Server**: listen addresses (เพิ่ม/ลบได้), threads, health/ready path, grace period,
   tuning ของ T104 (reuse_port, backlog, max_connections) พร้อมคำอธิบายผลกระทบและค่าที่แนะนำ
2. Tab **TLS**: รายการ cert (โดเมน, ผู้ออก, วันหมดอายุพร้อม badge เตือนเมื่อ < 30 วัน), อัปโหลด/ระบุ path,
   ตั้ง ACME (T112) + ปุ่ม "ขอ cert เดี๋ยวนี้" และแสดงผลครั้งล่าสุด/ข้อผิดพลาด
3. Tab **Observability**: log level (เปลี่ยนแล้วมีผลทันทีถ้าทำได้), access log on/off + format,
   prometheus listen, OTel endpoint (T402) พร้อมปุ่มทดสอบการเชื่อมต่อ
4. Tab **Admin**: auth mode, token rotation, allow origins, read-only mode (T201) —
   ต้องเตือนชัดเจนก่อนเปลี่ยนค่าที่อาจทำให้ล็อกตัวเองออกจากระบบ
5. ทุกการเปลี่ยนแปลงไปรวมที่ draft เดียวกับ T307 และต้องผ่าน diff + apply ทางเดียวกัน

## Acceptance criteria

- [ ] เปลี่ยนค่าทุกตัวแล้ว TOML ที่ได้ถูกต้องตาม schema (เทสต์ round-trip: UI → TOML → parse → UI)
- [ ] วันหมดอายุ cert แสดงถูกต้องจาก metric/endpoint ของ T111
- [ ] เปลี่ยน admin auth มีขั้นยืนยันและมีคำเตือนความเสี่ยง
- [ ] ค่าที่ต้องรีสตาร์ทถึงจะมีผล ถูกทำเครื่องหมายไว้ชัดเจน

## Out of scope

- จัดการผู้ใช้หลายคน
