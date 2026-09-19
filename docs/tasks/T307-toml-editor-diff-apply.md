# T307 — TOML editor + diff ก่อน apply

**Phase:** 3 · Web UI
**Status:** ✅ done
**Size:** M (~1d)
**Depends on:** T303, T203
**Files:** `src/validate.rs`, `src/config.rs`, `src/admin.rs`,
`webui/src/lib/components/config/*`, `webui/src/lib/configDiff.ts`,
`webui/src/lib/configChanges.ts`, `webui/src/lib/stores/configDraft.ts`,
`webui/src/lib/api/configText.ts`

## เป้าหมาย

คนที่ถนัดแก้ไฟล์เองต้องทำได้จาก UI และทุกคนต้องเห็น diff ก่อนกด apply เสมอ

## สถานะเดิม

แท็บ "TOML Config" ใน `SettingsPage.svelte` แสดง TOML ที่ `configCodec.ts` / `configNormalize.ts`
เรนเดอร์ออกมา อ่านอย่างเดียว (คัดลอกได้) ยังแก้ไม่ได้ ไม่มี diff ไม่มี syntax highlight

และที่แย่กว่านั้น: สิ่งที่แสดงไม่ใช่ไฟล์จริง เป็นไฟล์ที่ UI เรนเดอร์ขึ้นใหม่จาก model
คอมเมนต์ทั้งไฟล์หายหมด

## สิ่งที่ทำ

### ฝั่ง server — validate ที่มีพิกัด (แกนของ T203)

`PrxConfig::validate()` เคย `bail!` หยุดที่ error แรก แก้ให้เป็น `PrxConfig::check()`
ที่เก็บ **ทุกปัญหาในรอบเดียว** เป็น `Vec<ConfigIssue>` (severity, code ที่เสถียร, field path,
message, hint) ส่วน `validate()` กลายเป็น wrapper ที่คืน error ตัวแรก — ทางเรียกเดิมทั้งหมด
รวมถึงตอนโหลดไฟล์ จึงไม่เปลี่ยนพฤติกรรม

`src/validate.rs` ใหม่แปลง field path เป็นบรรทัด/คอลัมน์จริงในไฟล์ ด้วย span ของ `toml_edit`
(field ที่ไม่ได้เขียนในไฟล์จะถอยไปใช้ table แม่ แทนที่จะชี้บรรทัด 1) คอลัมน์นับเป็น "ตัวอักษร"
ไม่ใช่ byte ไฟล์ที่มีคอมเมนต์ภาษาไทยจึงยังชี้ตำแหน่งถูก

- `POST /web/config/validate` — คืน `{valid, errors[], warnings[], config?, current_etag}`
  โดยไม่เขียนไฟล์ ถ้า valid จะแนบ config ที่ parse แล้วมาด้วย UI จึงสรุปความหมายของ diff ได้
  โดยไม่ต้องมี TOML parser ในเบราว์เซอร์
- `PUT /web/config?dry_run=true` — คืนรายงานเดียวกัน ไม่เขียนไฟล์
- warning ที่เพิ่มเข้ามา: ไม่มี default route, service ที่ไม่มี route ไหนใช้, upstream ซ้ำ,
  route ที่ทับกันจนไม่มีวันถูก match, route ที่ถูกปิด, service ที่ upstream ถูก drain หมด,
  health check ที่ timeout ≥ interval, และ upstream TLS ที่ปิด verify_cert ไว้

### ฝั่ง server — optimistic concurrency (ข้อ 4 ของ T204)

`GET /web/config` แนบ `ETag` ของไฟล์ (ทั้งแบบ text และ `?format=json`) `PUT` รับ `If-Match`
ถ้าไฟล์ถูกแก้ไปแล้วจะคืน `409` พร้อม `current_etag` + `current_toml` — UI จึงกาง diff สามฝ่าย
ได้ทันทีโดยไม่ต้องยิงซ้ำ และ **ไม่มีทางเขียนทับเงียบๆ** ส่วน `HEAD /web/config` ใช้เฝ้าไฟล์
ระหว่างที่ draft เปิดอยู่ โดยไม่ต้องดึงไฟล์ทั้งก้อนมาเทียบ

### ฝั่ง UI

- **Editor**: CodeMirror 6 โหลดแบบ dynamic import — เข้าแท็บนี้เท่านั้นถึงจะโหลด
  มี syntax highlight (TOML mode เขียนเอง เพราะ `@codemirror/legacy-modes` ให้แค่ 3 token
  อ่าน config ไม่ออก), เลขบรรทัด, พับ section (`[[service]]` พับลูก `[[service.upstream]]` ไปด้วย),
  ค้นหา/แทนที่, undo/redo, ขีดเส้นใต้ error/warning ตรงบรรทัดที่ API บอก และแถบสีข้างบรรทัด
  ที่ draft แก้ไป สีทั้งหมดมาจาก design token เดิม dark mode จึงได้มาฟรี
  ถ้า chunk โหลดไม่สำเร็จจะตกไปใช้ textarea ที่ยัง validate/diff/apply ได้ครบ
- **Validate สด**: debounce 400 ms ยิง `POST /web/config/validate` คำตอบที่มาช้ากว่า keystroke
  ล่าสุดถูกทิ้ง (token) รายการปัญหาอยู่ใต้ editor คลิกแล้วกระโดดไปบรรทัดนั้น
- **Diff**: เขียนเอง (`configDiff.ts`) LCS + canonicalisation แบบเดียวกับ `diff`
  มีทั้ง unified (มี `@@` header จริง) และ side-by-side พร้อมสรุปเป็นภาษาคน
  ("service "api" changed · upstream 127.0.0.1:3001: weight 1 → 5") และป้าย
  "affects traffic" บนรายการที่เปลี่ยนเส้นทางของทราฟฟิกจริง
- **Apply flow**: ปุ่ม Review & apply → validate อีกรอบ → กาง diff + สรุป + warning →
  ยืนยัน → `PUT` พร้อม `If-Match` → สำเร็จ/ล้มเหลว/conflict
- **Conflict**: กาง 3 ฝ่าย (ของเขา, ของเรา, ผลถ้ากด apply ต่อ) แล้วให้เลือกว่า
  จะ rebase ทับของเขาหรือทิ้งของเราแล้วแก้จากของเขา
- **Draft ไม่หาย**: เก็บใน `localStorage` พร้อม etag ของฐานที่แก้มา เปิดแท็บใหม่แล้วได้คืน
  พร้อมป้ายบอกว่านี่คือ draft ที่ยังไม่ได้ apply และเฝ้าไฟล์ทุก 10 วินาที
  ถ้ามีคนแก้จากที่อื่นจะขึ้นเตือนทันที ไม่ต้องรอตอนกด apply

## ที่ตัดสินใจต่างจากที่ task เขียนไว้

- **ข้อ 5 (ปุ่มดูประวัติ + rollback) ยังไม่ได้ทำ** — ต้องมี T202 ก่อน เพราะ server
  ยังไม่เก็บประวัติ config ไว้ที่ไหนเลย ทำ UI ปลอมขึ้นมาไม่ได้ ที่ทำไว้ให้แล้วคือส่วนที่ T202
  จะใช้ต่อได้: diff engine, ตัวสรุปการเปลี่ยนแปลง และ apply flow ที่รับ TOML ก้อนไหนก็ได้
- **งบ bundle แยกเป็นสองก้อน** — CodeMirror ที่ตัดจนเล็กที่สุดแล้วยังกิน 118 KB gzip
  ถ้ารวมกับ bundle หลัก (196 KB) จะทะลุเพดาน 250 KB ที่ตั้งไว้ตั้งแต่ T301
  แต่ผู้ใช้ที่ไม่ได้เปิดแท็บนี้ไม่ได้จ่ายค่ามันเลย จึงแยกเป็น chunk `editor-*.js`
  แล้วให้ `scripts/bundle-budget.mjs` ตั้งงบของมันเองที่ 150 KB
  (เพดานเดิมของ bundle แรกยังเท่าเดิม และ `config:check` พิสูจน์ว่าหน้าอื่นไม่โหลดมัน)
- **Diff เขียนเองแทนที่จะลงไลบรารี** — เป็นโค้ด 200 บรรทัดที่เทสต์เทียบกับ `diff` ของระบบได้ตรงๆ
  และไม่ต้องเพิ่มอะไรเข้า bundle อีก
- **สรุปการเปลี่ยนแปลงเป็นภาษาอังกฤษ** ตามข้อความอื่นทั้ง UI (i18n ไทย/อังกฤษเป็นงานของ T309)

## ทดสอบ

- `cargo test` — `src/validate.rs`: error หลายตัวในรอบเดียว, บรรทัด/คอลัมน์ตรงกับไฟล์ตัวอย่าง,
  คอลัมน์นับตัวอักษรไม่ใช่ byte, syntax error รายงานตัวเดียวและอยู่ตรงตำแหน่ง,
  warning ชี้ไปที่ service ที่มันพูดถึง · `src/admin.rs`: ETag เปลี่ยนตามไฟล์และเท่ากันทั้งสอง
  รูปแบบ, validate คืน config + warning, apply ที่ etag ค้างได้ 409 พร้อมไฟล์ปัจจุบัน
  และไฟล์ไม่ถูกแตะ, apply ที่ etag ตรงผ่านและได้ etag ใหม่, dry-run ไม่เขียนไฟล์,
  config ที่ผิดบอกบรรทัดที่ผิด
- `npm run diff:check` — เทียบกับ `diff -u` ของระบบ 2000 เคสสุ่ม (ไฟล์ที่มีบรรทัดซ้ำเยอะๆ
  แบบ config จริง): edit distance เท่ากันทุกเคส, diff สร้างไฟล์เดิมและไฟล์ใหม่กลับมาได้ครบ,
  เลขบรรทัดตรง, และ output เหมือน `diff -u` ตัวต่อตัว ≥ 95% (ที่เหลือคือ diff ที่สั้นเท่ากัน
  แต่จัดวางคนละแบบ) บวกเคสของ config จริงอีก 7 เคสที่ต้องตรงตัวต่อตัว รวมถึงไฟล์ที่ไม่มี
  newline ปิดท้าย
- `npm run config:check` — ขับ editor จริงในเบราว์เซอร์: dashboard ไม่โหลด chunk ของ editor
  แต่เปิดแท็บ TOML แล้วโหลด, editor แสดงไฟล์จริงพร้อมคอมเมนต์, พิมพ์ผิดแล้วขีดแดงขึ้นที่
  บรรทัดถูกต้องใน 491 ms, apply ถูกปิดตอนมี error, draft รอด reload, review แสดง diff + สรุป,
  และ conflict กาง 3 ฝ่ายโดยไม่เขียนทับของคนอื่น
- `npm run smoke` กับไบนารีจริง — editor แสดงไฟล์ที่ proxy รันอยู่จริง (เทียบกับ
  `GET /web/config` ตรงๆ) และ TOML ที่ฟอร์มเขียนยัง round-trip ผ่าน parser จริงโดยไม่ตกอะไร

## Acceptance criteria

- [x] พิมพ์ TOML ผิด → เห็นขีดแดงที่บรรทัดถูกต้องภายใน 1 วินาที — debounce 400 ms + validate
      ฝั่ง server, วัดได้ 491 ms ใน `config:check` และเช็คว่าขีดอยู่บน field ที่ผิดจริง
- [x] Apply ที่ conflict (มีคนอื่นแก้ก่อน) → แสดง diff ของ 3 ฝ่ายและไม่เขียนทับเงียบๆ —
      `If-Match` → 409 + ไฟล์ปัจจุบัน, เทสต์ทั้งฝั่ง server (ไฟล์ไม่ถูกแตะ) และฝั่ง UI
- [x] Diff ถูกต้องเทียบกับ `diff` ของระบบ (มีเทสต์) — `npm run diff:check`
- [x] Editor โหลดเพิ่มไม่เกิน budget bundle (lazy load เมื่อเข้าหน้านี้เท่านั้น) —
      chunk แยก 118 KB gzip ใต้งบ 150 KB, bundle แรกยัง 196 KB ใต้งบ 250 KB เท่าเดิม,
      และ `config:check` ยืนยันว่าหน้า dashboard ไม่ดาวน์โหลดมัน

## Out of scope

- แก้ config หลายไฟล์ / include
- ประวัติ + rollback (ข้อ 5) — รอ [T202](T202-config-history-rollback.md)
