# T206 — Audit log

**Phase:** 2 · Control plane
**Status:** todo
**Size:** S (~0.5d)
**Depends on:** T201
**Files:** `src/admin.rs`, `src/audit.rs` (ใหม่)

## เป้าหมาย

ตอบคำถาม "เมื่อคืนใครแก้ route นี้" ได้ — จำเป็นทันทีที่มีคนใช้ Web UI มากกว่า 1 คน

## ขอบเขตงาน

1. เขียน audit entry ทุกครั้งที่มีการเปลี่ยนแปลง (apply, rollback, service/route CRUD, purge cache):
   เวลา (UTC), actor (token id/basic user จาก T201), source IP, action, target, ผลลัพธ์, config hash ก่อน/หลัง
2. เก็บเป็น JSON Lines ที่ `<config_dir>/.prx-history/audit.jsonl` (append-only, rotate ตามขนาด, 0600)
3. `GET /web/audit?limit=&since=&action=` สำหรับ UI (มี pagination)
4. ห้ามบันทึกค่า secret (token, key path content) ลง audit

## Acceptance criteria

- [ ] ทุก write endpoint สร้าง audit entry (เทสต์ครบทุก endpoint)
- [ ] entry มี config hash ก่อน/หลัง ที่ผูกกับ history ของ T202 ได้
- [ ] rotate ทำงานและไม่ทำให้ write ช้าลงจนกระทบ API
- [ ] ไม่มี secret ใน log (เทสต์ด้วยการค้นหา pattern)

## Out of scope

- ส่ง audit ออก syslog/SIEM (follow-up)
