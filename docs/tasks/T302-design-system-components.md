# T302 — ชุด component พื้นฐานและ design language

**Phase:** 3 · Web UI
**Status:** done
**Size:** M (~1d)
**Depends on:** T301
**Files:** `webui/src/lib/components/ui/**`, `webui/src/lib/styleguide/` (dev only)

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

## ผลลัพธ์ที่ส่งมอบ

**Component จาก shadcn-svelte — เขียนลงโปรเจกต์เอง ไม่ได้ใช้ CLI**

`shadcn-svelte.com` (ที่ CLI ดึง registry) ถูก egress policy ของ environment นี้บล็อก
(`connect_rejected`) และ npm package ของ CLI ไม่ได้แถม source ของ component มาด้วย
จึงเขียนตามโครงเดิมของ shadcn-svelte เอง — bits-ui 2.19 เป็น primitive,
`tailwind-variants` ทำ variant, `data-slot` ครบ, export ทั้งชื่อ `Root` และชื่อเต็ม
ผลลัพธ์คือไฟล์หน้าตาเดียวกับที่ `shadcn-svelte add` จะสร้าง ถ้าวันหลัง network เปิด
ก็รันทับได้เลย

ครบ 22 ตัวตามที่ task ระบุ: `button`, `card`, `input`, `textarea`, `label`, `select`,
`switch`, `checkbox`, `badge`, `table`, `dialog`, `sheet`, `dropdown-menu`, `tabs`,
`tooltip`, `separator`, `skeleton`, `alert`, `sonner`, `command`, `popover`, `form`

สองจุดที่ทำต่างจาก registry แล้วมีเหตุผล:

- **`form`** ของ shadcn-svelte ผูกกับ `formsnap` + `sveltekit-superforms` ซึ่งต้องมี
  SvelteKit (`$app/*`) แต่ UI นี้เป็น Vite SPA เปล่าๆ จึงทำ `Form.Field` ที่รับ
  `errors` เป็น prop แล้วเดินสาย `id` / `aria-describedby` / `aria-invalid` ให้เอง
  ได้ประโยชน์เดียวกันโดยไม่ต้องลาก framework เข้ามา
- **`tooltip`** bits-ui บังคับว่า `Tooltip.Root` ต้องมี `Provider` ครอบ ไม่งั้น throw
  ตอน render — wrapper จึงครอบ `Provider` ให้ในตัว component จะได้มี tooltip ของตัวเอง
  โดยหน้าที่เรียกใช้ไม่ต้องจำ

**Component เฉพาะของ prx** อยู่โฟลเดอร์เดียวกัน (`ui/`) แต่ไม่ได้มาจาก registry
จึงไม่โดน `shadcn-svelte add` เขียนทับ: `status-dot`, `metric-tile` (+ `sparkline`),
`copy-button`, `empty-state`, `confirm-dialog`, `key-value-list`

- `copy-button` มี fallback `execCommand('copy')` เพราะ `navigator.clipboard` มีเฉพาะ
  บน secure origin ส่วน prx มักถูกเปิดผ่าน HTTP บน LAN — ปุ่มจะได้ไม่ตายเงียบตรงนั้น
- `metric-tile` ต้องบอกด้วยว่าค่านี้ "ดีเมื่อขึ้นหรือลง" (`betterWhen`) ไม่งั้น latency
  ที่ลดลงจะถูกระบายเป็นสีแดงเหมือนกับ throughput ที่ตก

**สีเชิงความหมาย: ต้องเพิ่ม token ชุด `*-emphasis`**

token สีพื้นของ T301 (`--success` ฯลฯ) ออกแบบมาให้ตัวอักษรขาว/ดำทับแล้วอ่านได้
แต่พอเอาสีเดียวกันไปเป็น **ตัวอักษร** บนพื้นขาว contrast ไม่ถึง 4.5:1
จึงเพิ่ม `--success-emphasis` / `--warning-emphasis` / `--destructive-emphasis` /
`--circuit-emphasis` เป็นเฉดที่ผ่าน AA บนทั้งพื้นหน้าและ card ทั้งสอง theme
พร้อม `--circuit` (ส้ม, แยกจาก amber ของ degraded ตามที่ task กำหนด),
`--tooltip`, `--switch-track`

ระหว่างตรวจเจอของเดิมไม่ผ่านสามจุดแล้วแก้ที่ token ไม่ใช่ลดเกณฑ์:

- `--primary` (light) 0.59 → 0.50 — `text-primary` ถูกใช้อยู่ 72 จุดและได้แค่ 3.75:1
- `--success` (light) 0.63 → 0.53 — ตัวอักษรขาวบน badge เขียวได้แค่ 3.18:1
- `--warning` (light) 0.75 → 0.64 — จุดสถานะสีเหลืองบน card ได้แค่ 2.27:1 (ต้องการ 3:1)
- `--input` (light 0.91 → 0.63, dark 0.32 → 0.55) — ขอบช่องกรอกเดิมแทบมองไม่เห็น
  (1.2:1) ซึ่ง WCAG 1.4.11 บังคับ 3:1 เพราะมันคือสิ่งที่บอกว่าช่องกรอกอยู่ตรงไหน

**การตรวจ contrast สองชั้น**

- `npm run contrast` (อยู่ใน `npm run check` ด้วย) — อ่าน token จาก `app.css` ตรงๆ
  แปลง oklch → sRGB แล้วคำนวณ 82 คู่ที่ component ใช้จริง ทั้ง light/dark
  ข้อความ 4.5:1 ส่วนที่บอกตัวตน/สถานะของ control 3:1 รวมพื้นแบบ tint (`success/10`
  ทับ card) ที่ alert กับ toast ใช้จริงด้วย
- `npm run styleguide:check` — เปิด styleguide ใน Chromium จริง ตั้ง theme ทั้งสองแบบ
  เปิด popover / menu / tooltip / dialog / sheet / confirm ทีละอัน แล้ววัดทุกก้อนข้อความ
  จาก `getComputedStyle` (composite พื้นโปร่งแสงตามลำดับชั้นจริง) — 1,720 จุด ผ่านหมด
  ตัวนี้จับสิ่งที่ตัวแรกจับไม่ได้: token ถูกแต่ component หยิบคู่ผิด หรือมี layer ซ้อนกัน
  และมันจับบั๊กจริงไปแล้วหนึ่งตัว — `DropdownMenu.Label` ที่ไม่ได้อยู่ใน `Group`
  ทำให้ bits-ui throw ตอนเปิดเมนู

**Styleguide** `npm run styleguide` เปิด `/?styleguide`

อยู่หลัง `import.meta.env.DEV` ใน `main.ts` — production build แทนเป็น `false`
Rollup จึงตัดทั้ง branch พร้อม dynamic import ทิ้ง ยืนยันด้วย `grep -r styleguide dist/`
ที่ไม่เจออะไรเลย (ทำแบบนี้แทนการเช็คใน `App.svelte` เพราะเงื่อนไขที่อยู่ใน component
จะเหลือเป็นโค้ดที่ Rollup ตัดไม่ได้)

**Bundle**

JS gzip 51.7 KB เท่าเดิม (component ยังไม่ถูกหน้าไหนเรียกใช้ จึงถูก tree-shake)
CSS gzip 8.9 → 11.9 KB จาก utility ของ component library เอง
วัดแล้ว styleguide มีส่วนแค่ 0.1 KB ใน 3 KB นั้น — budget คือ 60 KB

**เอกสาร design language** อยู่ใน `webui/README.md`: spacing scale, radius 4 ระดับ,
elevation 5 ชั้น, ตารางสีเชิงความหมายพร้อมไอคอนประจำสถานะ, รูปแบบตัวเลข
(latency = ms, throughput = req/s, bytes = IEC) ซึ่งบังคับใช้จริงผ่าน `src/lib/format.ts`

## Acceptance criteria

- [x] ทุก component ใช้ token จาก T301 ไม่มีสี hardcode — เพิ่ม token ใหม่แทนการ hardcode
- [x] styleguide แสดงครบทั้ง light/dark และผ่านการตรวจ contrast (WCAG AA) — 82 คู่ token + 1,720 จุดที่ render จริง
- [x] `npm run check` ไม่มี type error — 706 ไฟล์ 0 error 0 warning
- [x] bundle ไม่โตเกิน budget — JS 51.7/250 KB, CSS 11.9/60 KB, font 93.3/200 KB

## Out of scope

- นำไปใช้แทน component เดิมในแต่ละหน้า (ทำใน T303–T308)
