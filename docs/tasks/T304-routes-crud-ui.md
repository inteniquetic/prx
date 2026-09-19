# T304 — หน้า Routes: data table + form

**Phase:** 3 · Web UI
**Status:** done
**Size:** L (~2d)
**Depends on:** T303, T205
**Files:** `webui/src/lib/components/pages/RoutesPage.svelte`, `webui/src/lib/components/routes/*`, `webui/src/lib/routeAnalysis.ts`, `webui/src/lib/routeValidation.ts`, `src/admin.rs`, `src/config.rs`, `src/router.rs`

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

## ผลลัพธ์ที่ส่งมอบ

แยกเป็น 2 commit: ฝั่ง server ก่อน แล้วค่อยฝั่ง UI

### Commit 1 — admin API

**payload ของ route เคยมีแค่ 6 field** (name/service/host/path_prefix/methods/is_default)
ทั้งที่ config จริงมี header rules, rate limit, concurrency limit, cache ด้วย
`update_route` เลยต้องเขียนของเก่ากลับเองกันข้อมูลหาย ตอนนี้ payload ขนไปทั้งชุดทั้งขาไปและขากลับ
และ block ที่ request ไม่ได้พูดถึง = "เก็บของเดิมไว้" ไม่ใช่ "ลบทิ้ง"

**เพิ่ม `enabled` ให้ route** — route ที่ปิดอยู่ยังอยู่ในไฟล์พร้อม config ครบ
แต่ไม่ถูกใส่เข้า index เลย: ไม่มีวันถูก match และเป็น default route ไม่ได้
`build` นับ index ก่อนแล้วค่อย skip เพื่อให้ route ที่อยู่หลัง route ที่ปิดไม่โดนเลื่อนเลข
(ทุกอย่างในระบบอ้าง route ด้วย index นี้)

**`POST /web/routes/test`** เรียก `select()` ตัวเดียวกับที่ proxy เรียก บน snapshot เดียวกัน
— tester ที่เขียนกติกาเองคือ tester ที่เริ่มโกหกวันที่กติกาเปลี่ยน
คืน route ที่ชนะ, ชนะด้วยกติกาข้อไหน, service, และ upstream พร้อมสถานะสด

การจะบอกว่า "ตัวไหนจะได้รับ request ถัดไป" ต้องเพิ่ม `peek_upstream` ที่อ่านอย่างเดียว
เพราะถ้าถามผ่าน `next_upstream` cursor ของ round-robin จะขยับ — เปิดหน้า UI แล้วทำให้
ทราฟฟิกจริงเบี้ยวไม่ได้ กลยุทธ์ที่สุ่ม (random / least_conn / p2c_ewma) ตอบตรงๆ ว่าไม่มีคำตอบล่วงหน้า

มี test 9 ตัว: tester เทียบกับ `select()` ทุกเคสในเมทริกซ์, กติกาข้อที่ชนะ, route ที่ปิดแล้ว
ไม่ match / ไม่ทำให้ index เลื่อน / เป็น default ไม่ได้ และ tester ไม่ขยับ load balancer

### Commit 2 — หน้า Routes

**ตาราง** ค้นหา (ชื่อ/host/path/service), filter service + สถานะ, เรียงทุกคอลัมน์, แบ่งหน้า,
เลือกหลายแถว → bulk enable / disable / delete, เมนูต่อแถว (edit, test, duplicate, enable/disable, delete)

**ทำให้อยู่ในงบ 50 ms ที่ 500 routes** — วัดแล้วพบว่าต้นทุนเกือบทั้งหมดคือการ*วาดแถว*
ไม่ใช่การกรอง (25 แถว 56 ms vs 10 แถว 28 ms) เลยไล่ตัดของแพงออกจากแถว:
checkbox เป็น native, ลิงก์ service เป็น `<button>` ธรรมดา, `StatusDot` ใช้โหมด `tooltip={false}`
(ไม่งั้นได้ floating-layer provider ต่อแถว), เมนู action mount เฉพาะแถวที่กดเปิด
และ haystack ของการค้นหาถูก lowercase ไว้ตอน analyze ครั้งเดียว
ผลรวม: 96.6 ms → **32 ms** (วัดจาก production build ไม่ใช่ dev server ซึ่งช้ากว่าจริง)

**ลำดับการ match + คำเตือน route ที่ถูกบัง** (`routeAnalysis.ts`)
route ที่ host+path_prefix ซ้ำกับ route ที่อยู่เหนือกว่าและ method ครอบคลุมกัน = ไม่มีวันถูก match
ขึ้นไอคอนเตือนพร้อมบอกว่าใครบัง; ครอบคลุมบางส่วนก็บอกว่าบางส่วน
รวมถึง default route ตัวที่ 2 ซึ่งถูกเมิน และ route ที่ปิดอยู่

ไฟล์นี้คือการเขียนกติกาของ `src/router.rs` ซ้ำ ซึ่งเป็นหนี้ที่รู้ตัว จึงจำกัดไว้แค่การอ่านตาราง
ส่วนคำตอบที่ผู้ใช้เอาไปตัดสินใจมาจาก tester ที่ถาม matcher ตัวจริง

**ฟอร์ม** อยู่ใน Sheet แบ่ง 4 แท็บ (Matching / Headers / Limits / Cache) แท็บที่มี error มีจุดกำกับ
validate ฝั่ง client ตาม `PrxConfig::validate` และ error จาก server ไปแปะที่ field ที่มันพูดถึง
**แต่แสดงเป็น alert ด้านบนด้วยเสมอ** เพราะ field นั้นอาจอยู่คนละแท็บหรือถูกซ่อน
(เจอตอนเขียน test: ข้อความเรื่อง `cache.ttl_ms` หายไปเงียบๆ เมื่อ cache ปิดอยู่)

**Route tester** อยู่ในหน้าเดียวกัน กดจากเมนูของแถวแล้ว prefill host/path ของ route นั้นให้

### สิ่งที่แก้ไปด้วยเพราะมันคือบั๊กข้อมูลหาย

`configCodec.ts` (ตัวที่เขียน TOML ตอนกด Save ในหน้า Settings) เคยเขียน route ออกมาแค่ 6 field
แปลว่า **การกด Save ทั้ง config ลบ header rules / rate limit / cache ของทุก route ทิ้ง**
ตอนนี้ codec + normalizer + type ฝั่ง UI ขนครบ และเขียนเฉพาะค่าที่ต่างจาก default ของ prx
(ไฟล์ไม่บวม และ default ที่เปลี่ยนใน prx รุ่นหลังยังไปถึง route ที่ไม่เคยแสดงความเห็น)

`npm run smoke` เพิ่มการตรวจนี้แบบ end-to-end: อ่าน TOML ที่ UI สร้าง → `PUT /web/config`
ไปที่ binary จริง → อ่านกลับมาเทียบว่า rate limit และ header rules ยังอยู่

**ยังเหลือของฝั่ง service** — `ServiceConfig` ฝั่ง UI ยังไม่มี `health_check`, `sticky`,
`retry_budget`, timeout ต่างๆ ดังนั้นการ Save ทั้ง config ยังลบ field เหล่านั้นทิ้งอยู่
เรื่องนี้เป็นของ T305 (หน้า Services) ไม่ได้แก้ในงานนี้

### ที่ไม่ได้ทำตามขอบเขต

- **client validation จาก JSON Schema** — T205 ยังไม่เสร็จ (ไม่มี `GET /web/schema`)
  `routeValidation.ts` จึงเขียนกฎซ้ำไว้เอง ตั้งใจให้เหลือแค่ wrapper บางๆ เมื่อ T205 มาถึง
- **error code จาก T203** — ยังไม่มี contract ฝั่ง server ตอนนี้จึงอ่านข้อความ
  (`route 'x' cache.ttl_ms must be > 0`) แล้วเดา field จากชื่อที่มันเอ่ยถึง
  ทุกข้อความที่ไม่รู้จักยังแสดงเป็น alert ระดับฟอร์ม ไม่มีอะไรถูกกลืนหาย
- **virtual list** — ไม่จำเป็น การแบ่งหน้าพอสำหรับงบเวลาที่ตั้งไว้

## Acceptance criteria

- [x] สร้าง/แก้/ลบ route แล้วเห็นผลทันที และ `Prx.toml` เปลี่ยนถูกต้อง — CRUD ผ่าน admin API
      (apply ทันที) + smoke ตรวจ round trip ของ TOML ผ่าน binary จริง
- [x] ตารางที่ 500 routes: พิมพ์ค้นหาแล้วตอบสนอง < 50ms — วัดได้ ~32 ms จาก production build
- [x] Route tester ให้ผลตรงกับ matcher จริง — เรียก `select()` ตัวเดียวกัน + test เทียบทุกเคส
- [~] Error จาก server แสดงที่ฟิลด์ถูกต้อง — ทำได้เท่าที่ server พูดได้ตอนนี้ (T203 ยังไม่มี
      error code) จึง map จากข้อความ และ fallback เป็น alert ระดับฟอร์มเสมอ
- [x] แก้แล้วยังไม่ apply → มี indicator "draft" ชัดเจน และเตือนก่อนปิดแท็บ —
      แถบบนจาก T303 + `beforeunload`

## Out of scope

- แก้ TOML ดิบ (T307)
