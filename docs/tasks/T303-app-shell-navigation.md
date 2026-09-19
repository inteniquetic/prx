# T303 — App shell: sidebar, topbar, breadcrumb, command palette

**Phase:** 3 · Web UI
**Status:** done
**Size:** M (~1d)
**Depends on:** T302
**Files:** `webui/src/lib/components/layout/*`, `webui/src/lib/stores/navigation.ts`, `webui/src/lib/stores/connection.ts`, `src/admin.rs`

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

## ผลลัพธ์ที่ส่งมอบ

**Routing: URL จริง ไม่ใช่ hash**

`stores/navigation.ts` เขียนใหม่เป็น history router — `/`, `/routes`, `/routes/:name`,
`/services`, `/services/:name`, `/tls`, `/settings`, `/audit`
รองรับ popstate (back/forward), เคารพ `import.meta.env.BASE_URL`
และปล่อยให้ ctrl/cmd/middle-click เปิดแท็บใหม่ได้ตามปกติ (`isPlainClick`)

**กติกาที่ทำให้ deep link ไม่พัง:** URL เป็นเจ้าของ selection ฝั่งเดียว
หน้าเพจไม่ set `selectedRouteIndex` เอง แต่ `dispatch('select', name)` ให้ shell เปลี่ยน URL
แล้วรับ prop กลับมา — ตอนแรกทำเป็น sync สองทาง แล้ว shell:check จับได้ว่า
`/routes/route-42` เด้งกลับ `/routes` ทันที เพราะ render แรกใช้ config ตั้งต้น
(ซึ่งมี route ตัวอย่าง 1 ตัว) หาชื่อไม่เจอเลยรีบล้าง URL ทิ้งก่อน config จริงจะมาถึง
ทิศทางเดียวปิดช่องนั้นทั้งคลาส

ชื่อที่ไม่มีอยู่จริงใน config จะเด้งกลับ list พร้อม toast บอกว่าไม่เจอ — URL ไม่ค้างชี้ของที่ไม่ได้เปิด

**แก้ SPA fallback ฝั่ง server ด้วย (`src/admin.rs`)**

ของเดิมตัดสินด้วย "path มีจุดไหม" ถ้ามีถือว่าเป็นไฟล์แล้วคืน 404
แต่ route ใน prx มักตั้งชื่อตามโดเมน — `/routes/api.example.com` ซึ่งเป็นลิงก์ที่ UI แจกเอง
จึงโดน 404 ทั้งที่ควรเปิดได้ เปลี่ยนเป็น: path ที่อยู่ในโฟลเดอร์ asset ที่ฝังมาจริง
(`assets/`, `fonts/` — อ่านจาก `WEBUI_DIST.dirs()`) และหาไฟล์ไม่เจอ = 404 เหมือนเดิม
(ตอบ HTML ให้ `.js` ที่หายไปจะกลายเป็น syntax error ที่ตามยากกว่า)
นอกนั้นเป็นของ client router ทั้งหมด — มี unit test 4 ตัวคุมพฤติกรรมนี้

**Sidebar**

เมนู 6 รายการแบ่ง 3 กลุ่ม (Overview / Traffic / Operations) ใช้ไอคอน lucide,
badge บอกจำนวน route ที่ไม่ healthy และ upstream ที่ down (ขึ้นเฉพาะเมื่อ probe รันแล้วจริงๆ —
"ศูนย์" กับ "ยังไม่เคยเช็ค" ไม่ใช่เรื่องเดียวกัน), ยุบเหลือไอคอนได้และจำใน localStorage
ตอนยุบใช้ tooltip เป็น label, ต่ำกว่า `md` กลายเป็น drawer (`Sheet`) ที่ปิดตัวเองหลังเลือกเมนู
TLS กับ Audit ยังเป็น placeholder — ติดป้าย T308 / T206 ไว้บนเมนูตั้งแต่ยังไม่กด

**Topbar**

breadcrumb (prx → หน้า → ชื่อ), ปุ่มเปิด palette, ป้าย "draft ยังไม่ apply"
(เทียบ TOML ปัจจุบันกับ TOML ที่ apply สำเร็จครั้งล่าสุด กดแล้วไป Settings),
สถานะ admin API, เมนู theme, เมนู account

เมนู theme ใช้ radio item (เพิ่ม `dropdown-menu-radio-item` เข้าชุด T302)
ไม่ใช่ item ธรรมดาที่แปะ ✓ เพราะมันคือการเลือกหนึ่งในสาม และ screen reader ควรได้ยินแบบนั้น

**เมนู account บอกความจริงว่ายังไม่มี auth** — admin API ยังไม่รับ credential (T201)
จึงไม่มี session ให้ออก รายการ Sign out เลยเป็น disabled พร้อมบอกเหตุผล
ดีกว่ามีปุ่มที่กดแล้วไม่เกิดอะไร

**สถานะการเชื่อมต่อ (`stores/connection.ts`)**

topbar เขียนว่า "Online" ก็ต้องมีอะไรรู้ว่าเมื่อไหร่มันไม่จริง —
ทุก request ผ่าน `$lib/api/admin` รายงานผลเข้า store (ตอบมาแม้เป็น 500 ก็ถือว่าต่อถึง)
บวก heartbeat ทุก 15 วินาที backoff ถึง 60 วินาทีเมื่อล่ม และหยุดเมื่อ tab ไม่ได้อยู่หน้าจอ
พลาด 3 ครั้งติดถึงเรียก offline

**Command palette**

`Cmd/Ctrl + K` ค้น route จากชื่อ/host/path/service, service จากชื่อ/address ของ upstream,
กระโดดไปหน้า, action ด่วน (เพิ่ม route, เช็ค health, apply draft, review draft)

กรองเองแทนที่จะใช้ตัวกรองของ bits-ui เพราะต้อง match host ด้วย และต้องตัดผลลัพธ์
ให้เหลือกลุ่มละ 7 แถว — ต้นทุนการเปิดจึงเป็นการวาดสิบกว่าบรรทัด ไม่ใช่วาดทั้ง config

**`npm run shell:check` — ของแถมที่ทำให้ AC ตรวจได้จริง**

AC ทั้งสี่ข้อเป็นพฤติกรรมที่ type checker มองไม่เห็น เลยเขียนสคริปต์ playwright
ที่ mock admin API ด้วย config 500 route / 50 service แล้วขับจริง 27 ข้อ:
deep link + refresh + back/forward, เมนู topbar, เดินด้วยคีย์บอร์ดล้วนจนพิมพ์แก้ route ได้,
6 หน้าที่ 375px, เวลาที่ palette ใช้เปิด, drawer และ contrast ของ chrome ทั้งสอง theme
(ตัว contrast ใช้ตัววัดตัวเดียวกับ `styleguide:check` — แยกออกมาเป็น `scripts/measure-contrast.mjs`)

มันจับบั๊กจริง 2 ตัวระหว่างทาง: deep link ที่โดนเขียนทับข้างบน
และ `DropdownMenu.Label` ที่อยู่นอก group ซึ่ง bits-ui throw ตอนเปิดเมนู

`npm run smoke` (ที่ขับ binary จริง) อัปเดตตามโครงใหม่ — เมนูหาด้วย `href` ไม่ใช่ข้อความ
เพราะป้าย badge ทำให้ชื่อเปลี่ยน — และเพิ่มเช็ค deep link ที่วิ่งผ่าน server จริง
รันผ่านกับ `./target/debug/prx` ที่ embed dist ใหม่แล้ว

**Bundle**

JS gzip 51.7 → 143.4 KB เพราะคราวนี้ component จาก T302 ถูกใช้จริง
แยกได้เป็น: โค้ดแอป 47, bits-ui 31, dependency ของมัน (floating-ui ฯลฯ) 33,
svelte runtime 23, svelte-sonner 9.5, lucide 3.6 — budget คือ 250 KB

## Acceptance criteria

- [x] Refresh ที่หน้า `/routes/api-v1` แล้วยังอยู่หน้าเดิม — ตรวจทั้งใน `shell:check`
      และใน `smoke` ที่วิ่งผ่าน binary จริง รวมถึง back/forward
- [x] ใช้งานได้ด้วยคีย์บอร์ดล้วนตั้งแต่เปิดหน้าจนแก้ route เสร็จ — skip link → เมนู →
      Enter → แถวในตาราง → พิมพ์ลงฟอร์ม
- [x] จอกว้าง 375px ใช้งานได้ครบ ไม่มี horizontal scroll — วัดทั้ง 6 หน้า overflow = 0px
      และ drawer ใช้แทน sidebar ได้
- [x] Command palette เปิดใน < 100ms กับ config ที่มี 500 routes — วัดได้ ~46ms

## Out of scope

- เนื้อหาในแต่ละหน้า (T304–T308)
