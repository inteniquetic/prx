# T303 — App shell: sidebar, topbar, breadcrumb, command palette

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T302
**Files:** `webui/src/lib/components/layout/*`, `webui/src/lib/stores/navigation.ts`

## เป้าหมาย

โครงหน้าเว็บที่ใช้ง่ายและเร็ว: หาอะไรก็เจอใน 2 คลิกหรือ 1 คีย์ลัด

## สถานะปัจจุบัน

มี `AppLayout.svelte` + `Sidebar.svelte` และ store `navigation.ts` อยู่แล้ว (เป็น client-side routing แบบง่าย)
ยังไม่มี breadcrumb, ไม่มี command palette, ยังไม่ responsive สำหรับจอเล็ก

## ขอบเขตงาน

1. Sidebar: กลุ่มเมนู Dashboard / Routes / Services / TLS / Settings / Audit
   - แสดง badge สถานะ (เช่น จำนวน upstream ที่ down) ข้างเมนู
   - ยุบเป็นไอคอนได้ จำสถานะไว้ใน localStorage
   - บนจอเล็กเปลี่ยนเป็น `Sheet` (drawer)
2. Topbar: breadcrumb, ตัวบอกสถานะการเชื่อมต่อ admin API (online/reconnecting),
   ตัวบอกว่ามี draft ที่ยังไม่ apply, theme toggle, เมนู account (logout ตาม T201)
3. Command palette (`Cmd/Ctrl + K`): ค้นหา route/service ตามชื่อและ host, กระโดดไปหน้า, action ด่วน
   (เพิ่ม route ใหม่, apply draft, ดู diff)
4. Routing: ใช้ URL จริง (hash หรือ history) ให้ refresh แล้วอยู่หน้าเดิมและแชร์ลิงก์ได้
   — ต้องทำงานกับ static SPA fallback ที่ `src/admin.rs:856` (`GET /{*path}`)
5. Keyboard navigation + focus ring ครบทุกจุด

## Acceptance criteria

- [ ] Refresh ที่หน้า `/routes/api-v1` แล้วยังอยู่หน้าเดิม (ไม่ 404, ไม่เด้งกลับ dashboard)
- [ ] ใช้งานได้ด้วยคีย์บอร์ดล้วนตั้งแต่เปิดหน้าจนแก้ route เสร็จ
- [ ] จอกว้าง 375px ใช้งานได้ครบ ไม่มี horizontal scroll
- [ ] Command palette เปิดใน < 100ms กับ config ที่มี 500 routes

## Out of scope

- เนื้อหาในแต่ละหน้า (T304–T308)
