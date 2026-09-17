# T302 — ชุด component พื้นฐานและ design language

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T301
**Files:** `webui/src/lib/components/ui/**`, `webui/src/routes/_styleguide` (dev only)

## เป้าหมาย

มีชุด component มาตรฐานพร้อมใช้ เพื่อให้ทุกหน้าหน้าตาเป็นระบบเดียวกัน ไม่ใช่ต่างคนต่างเขียน Tailwind

## ขอบเขตงาน

1. ดึง component จาก shadcn-svelte เท่าที่ต้องใช้จริง:
   `button`, `card`, `input`, `textarea`, `label`, `select`, `switch`, `checkbox`, `badge`,
   `table`, `dialog`, `sheet`, `dropdown-menu`, `tabs`, `tooltip`, `separator`, `skeleton`,
   `alert`, `sonner` (toast), `command` (สำหรับ command palette), `popover`, `form`
2. กำหนด design language ไว้เป็นเอกสารสั้นๆ ใน `webui/README.md`:
   - spacing scale, radius, ระดับ elevation
   - สีเชิงความหมาย: healthy = green, degraded = amber, down = red, circuit-open = orange, disabled = muted
     (ต้องแยกแยะได้ด้วยไอคอน/ข้อความด้วย ไม่พึ่งสีอย่างเดียว — คนตาบอดสีต้องใช้ได้)
   - รูปแบบตัวเลข: latency เป็น ms, throughput เป็น req/s, bytes เป็น IEC
3. Component เฉพาะของ prx ที่ใช้ซ้ำทุกหน้า:
   - `StatusDot` (สถานะ upstream พร้อม tooltip เหตุผล)
   - `MetricTile` (ค่าใหญ่ + delta + sparkline)
   - `CopyButton`, `EmptyState`, `ConfirmDialog`, `KeyValueList`
4. หน้า styleguide สำหรับ dev (`?styleguide` หรือ route ที่ตัดออกตอน production build) ไว้ดู component ทั้งหมด

## Acceptance criteria

- [ ] ทุก component ใช้ token จาก T301 ไม่มีสี hardcode
- [ ] styleguide แสดงครบทั้ง light/dark และผ่านการตรวจ contrast (WCAG AA)
- [ ] `npm run check` ไม่มี type error
- [ ] bundle ไม่โตเกิน budget

## Out of scope

- นำไปใช้แทน component เดิมในแต่ละหน้า (ทำใน T303–T308)
