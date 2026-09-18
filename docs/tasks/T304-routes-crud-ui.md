# T304 — หน้า Routes: data table + form

**Phase:** 3 · Web UI
**Status:** todo
**Size:** L (~2d)
**Depends on:** T303, T205
**Files:** `webui/src/lib/components/pages/RoutesPage.svelte`, `webui/src/lib/components/routes/*`

## เป้าหมาย

หน้าที่คนใช้บ่อยที่สุด ต้องจัดการ route ได้เร็วกว่าการแก้ไฟล์ด้วยมืออย่างชัดเจน

## สถานะปัจจุบัน

มี `RoutesPage.svelte`, `RouteFormModal.svelte`, `RouteDetailPanel.svelte` และ API CRUD ครบฝั่ง server
(`src/admin.rs:1224-1490`) — งานนี้คือยกระดับให้เป็น shadcn + เพิ่มความสามารถที่ขาด

## ขอบเขตงาน

1. Data table: ค้นหา (ชื่อ/host/path/service), filter ตาม service และสถานะ, เรียงคอลัมน์, pagination
   (ต้องลื่นที่ 500+ routes → ใช้ virtual list ถ้าจำเป็น)
2. คอลัมน์: ชื่อ, host, path prefix, methods, service, สถานะ upstream (จาก T113/T207), default badge, เมนู action
3. แสดง **ลำดับความสำคัญของการ match** ให้เห็น (ตามกติกาใน T102) และเตือนเมื่อ route ถูกบัง
   ("route นี้ไม่มีวันถูก match เพราะ `/api` ที่อยู่เหนือกว่าครอบคลุมแล้ว")
4. Form (dialog หรือ sheet): validate ฝั่ง client จาก JSON Schema (T205) + validate จริงฝั่ง server (T203)
   แสดง error ที่ฟิลด์ตรงจุด ไม่ใช่ toast รวม
5. ฟิลด์ที่ต้องรองรับ: name, service (combobox + ปุ่มสร้าง service ใหม่ inline), host (รองรับ wildcard),
   path_prefix, methods (multi-select), is_default (พร้อมเตือนว่าจะย้าย default จาก route เดิม),
   header rules (T106), rate limit (T108), cache (T109) — ซ่อนใน section "Advanced"
6. Bulk action: เปิด/ปิด, ลบหลายรายการ, duplicate route
7. Route tester: ใส่ method + host + path แล้วบอกว่าจะ match route ไหน → service ไหน → upstream ตัวไหน
   (ต้องมี endpoint ฝั่ง server `POST /web/routes/test` — เพิ่มใน task นี้)

## Acceptance criteria

- [ ] สร้าง/แก้/ลบ route แล้วเห็นผลทันที และ `Prx.toml` เปลี่ยนถูกต้อง
- [ ] ตารางที่ 500 routes: พิมพ์ค้นหาแล้วตอบสนอง < 50ms
- [ ] Route tester ให้ผลตรงกับ matcher จริง (มีเทสต์ฝั่ง server เทียบ `select_route`)
- [ ] Error จาก server แสดงที่ฟิลด์ถูกต้องทุกเคสของ T203
- [ ] แก้แล้วยังไม่ apply → มี indicator "draft" ชัดเจน และเตือนก่อนปิดแท็บ

## Out of scope

- แก้ TOML ดิบ (T307)
