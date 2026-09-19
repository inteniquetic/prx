# T307 — TOML editor + diff ก่อน apply

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T303, T203
**Files:** `webui/src/lib/components/config/*`, `webui/src/lib/configCodec.ts`

## เป้าหมาย

คนที่ถนัดแก้ไฟล์เองต้องทำได้จาก UI และทุกคนต้องเห็น diff ก่อนกด apply เสมอ

## สถานะปัจจุบัน

แท็บ "TOML Config" ใน `SettingsPage.svelte` แสดง TOML ที่ `configCodec.ts` / `configNormalize.ts`
เรนเดอร์ออกมา อ่านอย่างเดียว (คัดลอกได้) ยังแก้ไม่ได้ ไม่มี diff ไม่มี syntax highlight

## ขอบเขตงาน

1. Editor: CodeMirror 6 + TOML mode (self-host, tree-shake ให้เล็ก) — syntax highlight, พับ section,
   เลขบรรทัด, ค้นหา/แทนที่
2. Validate สด (debounce ~400ms) ยิง `POST /web/config/validate` (T203) แล้ว mark error/warning บนบรรทัดนั้นๆ
3. Diff view: เทียบ draft กับ config ที่ใช้อยู่ (side-by-side และ unified) ก่อนกด Apply ทุกครั้ง
   พร้อมสรุปภาษามนุษย์ ("เพิ่ม 1 route, แก้ weight ของ upstream 127.0.0.1:3002, ลบ 1 service")
4. Apply flow: ยืนยัน → ส่ง `If-Match` (T204) → แสดงผลลัพธ์ (สำเร็จ/ล้มเหลว/rollback อัตโนมัติ)
5. ปุ่มดูประวัติ (T202): เลือกเวอร์ชันเก่า ดู diff และ rollback ได้จากที่นี่
6. Draft เก็บใน localStorage กันแท็บปิดแล้วงานหาย และแจ้งเตือนถ้าไฟล์ถูกแก้จากข้างนอกระหว่างที่แก้อยู่ (conflict)

## Acceptance criteria

- [ ] พิมพ์ TOML ผิด → เห็นขีดแดงที่บรรทัดถูกต้องภายใน 1 วินาที
- [ ] Apply ที่ conflict (มีคนอื่นแก้ก่อน) → แสดง diff ของ 3 ฝ่ายและไม่เขียนทับเงียบๆ
- [ ] Diff ถูกต้องเทียบกับ `diff` ของระบบ (มีเทสต์)
- [ ] Editor โหลดเพิ่มไม่เกิน budget bundle (lazy load เมื่อเข้าหน้านี้เท่านั้น)

## Out of scope

- แก้ config หลายไฟล์ / include
