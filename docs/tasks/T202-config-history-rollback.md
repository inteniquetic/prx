# T202 — Config history, diff, rollback

**Phase:** 2 · Control plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T201
**Files:** `src/admin.rs`, `src/history.rs` (ใหม่)

## เป้าหมาย

แก้ config ผ่านเว็บแล้วพัง ต้องย้อนกลับได้ใน 1 คลิก — นี่คือสิ่งที่ทำให้กล้าใช้ Web UI จริงบน production

## สถานะปัจจุบัน

`apply_config_text()` (`src/admin.rs:69`) เขียนทับ `Prx.toml` ตรงๆ ไม่มีสำเนาเก่าเก็บไว้เลย

## ขอบเขตงาน

1. เก็บเวอร์ชันลง `<config_dir>/.prx-history/<timestamp>-<hash>.toml` (จำกัดจำนวน เช่น 50 เวอร์ชันล่าสุด + ขนาดรวม)
2. Endpoints:
   - `GET /web/config/history` → list (id, เวลา, ผู้แก้ถ้ามีจาก T206, ขนาด, summary ของการเปลี่ยนแปลง)
   - `GET /web/config/history/{id}` → เนื้อไฟล์
   - `GET /web/config/history/{id}/diff` → unified diff เทียบเวอร์ชันปัจจุบัน
   - `POST /web/config/history/{id}/rollback` → apply ย้อนกลับ (ผ่าน path เดียวกับ T204)
3. บันทึกเวอร์ชันทุกครั้งที่ apply สำเร็จ **และ** ตอนตรวจพบว่าไฟล์ถูกแก้จากข้างนอก (มี watcher อยู่แล้วที่ `src/reload.rs`)
4. เขียน metadata คู่กัน (`.json`): เวลา, ที่มา (`webui` / `file-watch` / `api`), ผลลัพธ์ reload

## Acceptance criteria

- [ ] apply 3 ครั้ง → history มี 3 entry และ rollback ไปเวอร์ชันที่ 1 ได้ traffic ทำงานถูกต้อง
- [ ] ไฟล์ history ไม่โตเกินเพดาน (เทสต์ rotate)
- [ ] rollback ผ่าน validate ก่อนเสมอ (rollback ไปเวอร์ชันที่ใช้ไม่ได้กับ binary ใหม่ ต้องถูกปฏิเสธพร้อมเหตุผล)
- [ ] permission ของไฟล์ history = 0600

## Out of scope

- Git-backed config store
