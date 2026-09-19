# T308 — หน้า Settings: server / TLS / observability

**Phase:** 3 · Web UI
**Status:** ✅ done
**Size:** M (~1d)
**Depends on:** T303
**Files:** `src/config_edit.rs`, `src/admin.rs`, `src/acme.rs`, `src/main.rs`,
`webui/src/lib/components/settings/*`, `webui/src/lib/components/config/DraftBar.svelte`,
`webui/src/lib/components/pages/SettingsPage.svelte`, `webui/src/lib/components/pages/TlsPage.svelte`

## เป้าหมาย

ตั้งค่าที่เหลือทั้งหมดได้จาก UI โดยไม่ต้องจำชื่อคีย์ใน TOML

## สิ่งที่ทำ

### แกนของงานนี้: ฟอร์มแก้ "ไฟล์" ไม่ใช่ "โมเดล"

ข้อ 5 ของ task ("ทุกการเปลี่ยนแปลงไปรวมที่ draft เดียวกับ T307") เป็นตัวกำหนดสถาปัตยกรรมทั้งหมด
ของงานนี้ ถ้าฟอร์มแก้ config object แล้วเรนเดอร์ไฟล์ใหม่ทั้งก้อน คอมเมนต์ในไฟล์หายหมด
และ field ที่ฟอร์มไม่รู้จักก็หายไปด้วย (บั๊กที่โปรเจกต์นี้เจอมาแล้วสองรอบใน T304/T305)

จึงเพิ่ม `POST /web/config/edit` (`src/config_edit.rs`): รับ draft + รายการ op
(`{path, action, value}` โดย path ใช้ syntax เดียวกับ error ของ validate เช่น
`server.tls.acme.domains`, `service[0].upstream[1].weight`) แล้วให้ `toml_edit` แก้เฉพาะคีย์ที่ระบุ
ไบต์อื่นในไฟล์ไม่ถูกแตะ — เปลี่ยน `log_level` หนึ่งค่าแล้ว diff ได้บรรทัดเดียวจริงๆ
มี `set` / `remove` / `append` (สำหรับ `[[server.tls.cert]]`) และคืน validation report
มาพร้อมกัน ฟอร์มที่เพิ่งทำให้ config พังจึงบอกได้ทันทีในรอบเดียว

ผลคือหน้า Settings กับ TOML editor เป็นมุมมองสองมุมของ draft เดียวกัน `DraftBar`
(แยกออกมาจาก `ConfigEditor` ของ T307) อยู่เหนือแท็บทั้งหมด ถือสถานะ draft, ปุ่ม
Reload / Revert / Review & apply, dialog review, และ conflict 3 ฝ่าย — ทางออกสู่ proxy
มีทางเดียวไม่ว่าจะแก้ด้วยฟอร์มหรือพิมพ์เอง

### แท็บที่ได้

1. **Server** — listen addresses (เพิ่ม/ลบ/แก้), health/ready path, threads, h2c,
   grace period, shutdown timeout, reload debounce
2. **TLS** — (เป็นหน้า `/tls` ด้วย ไม่ใช่แค่แท็บ) แบ่งเป็น "cert ที่ proxy เสิร์ฟอยู่จริง"
   (อ่านจาก `/web/tls/status` ของ T111 พร้อม badge เหลือ < 30 วัน / หมดอายุแล้ว) กับส่วนที่แก้ draft:
   listener, enable_h2, cert เดี่ยว, `[[server.tls.cert]]` ตาม SNI (เพิ่ม/ลบ/ตั้ง default),
   และ ACME ทั้งชุด พร้อมปุ่ม **"Order a certificate now"** และผลครั้งล่าสุด/ข้อผิดพลาด
3. **Observability** — log level, access log, prometheus listener
4. **Admin API** — ยังไม่มีอะไรให้แก้ (ดูด้านล่าง)
5. **TOML** — editor ของ T307

### ปุ่ม "ขอ cert เดี๋ยวนี้"

ลูป ACME ตื่นทุก 12 ชั่วโมง ซึ่งเป็นช่วงที่ถูกสำหรับการต่ออายุ แต่ผิดสำหรับคนที่เพิ่งแก้ DNS เสร็จ
จึงเพิ่ม `RenewRequest` (`AtomicBool` + `Notify`) ที่ `spawn_acme` ถือร่วมกับ admin API
`POST /web/tls/acme/renew` ปลุกลูปและสั่ง order โดยไม่สนว่าใกล้หมดอายุหรือยัง
ถ้า ACME ไม่ได้เปิดอยู่จะได้ 409 พร้อมบอกว่าต้องเปิดใน config ก่อน

### "ค่านี้ต้องรีสตาร์ท"

ไล่ดูโค้ดจริงว่าอะไร reload แล้วมีผล อะไรอ่านตอน start เท่านั้น: `RuntimeConfig` (route, service,
header, cache, rate limit) มีผลทันทีที่ reload ส่วน listen, threads, health/ready path, h2c,
grace period, debounce, log level, access log, prometheus listener และ TLS listener
ถูกอ่านครั้งเดียวตอน start ทั้งหมด ทุก field ในหน้านี้จึงติดป้าย "needs a restart"
พร้อมอธิบายไว้ด้านบนแท็บ Server ว่าทำไม

## ที่ตัดสินใจต่างจากที่ task เขียนไว้

- **ข้อ 1 ส่วน tuning ของ T104 (reuse_port, backlog, max_connections) ยังไม่มี** — ยังไม่มีใน
  config schema เพราะ T104 ยังไม่ได้ทำ จะเพิ่มในหน้านี้ตอน T104 ลง
- **ข้อ 3 (access log format, OTel endpoint) ยังไม่มี** — เป็นของ T403 และ T402 ตามลำดับ
  หน้าเขียนบอกไว้ตรงนั้นว่ารออะไรอยู่
- **ข้อ 4 (แท็บ Admin) แสดงความจริงแทนที่จะแสดงสวิตช์** — T201 ยังไม่ได้ทำ admin API
  จึงยังไม่มี auth, allow-origins หรือ read-only mode ใน schema เลย
  การใส่สวิตช์ "require a token" ที่ไม่ได้ต่อกับอะไรคือช่องโหว่ที่มีป้ายปลอบใจติดอยู่
  แท็บนี้จึงบอกตรงๆ ว่า API ยังไม่มี auth, ป้องกันด้วยการ bind loopback อย่างเดียว,
  และ `PRX_ADMIN_LISTEN` เป็น env var ไม่ใช่ config key
- **TLS เป็นหน้าแยกด้วย** — `/tls` มีอยู่ใน sidebar ตั้งแต่ T303 (ขึ้น placeholder ว่ารอ T308)
  คำถาม "cert หมดอายุเมื่อไหร่" เป็นคำถามที่คนเปิด UI มาถาม ไม่ใช่สิ่งที่ควรต้องไปขุดใน Settings
  จึงใช้ component เดียวกันทั้งสองที่
- **ปุ่ม Save หายไป** — ไม่มี "save" ที่เขียนไฟล์จากโมเดลอีกแล้ว มีแต่ draft + review + apply
  ทางเดียว ส่วน Import JSON ยังเรนเดอร์ไฟล์ใหม่ทั้งก้อน (config จากเครื่องอื่นไม่มีคอมเมนต์ของเราอยู่แล้ว)
  แต่ลงไปที่ draft เพื่อให้ดู diff ก่อน apply เหมือนทางอื่น

## ผลข้างเคียงที่แก้ไปด้วย

- `/web/config?format=json` เคยไม่คืน `h2c` และไม่คืน `[server.tls.acme]` เลย
  (UI จึงมองไม่เห็นค่าที่ตั้งไว้) — เพิ่มทั้งสองอย่าง
- `certs` ใน payload เคยเป็นรายการที่ผสม cert เดี่ยวเข้าไปด้วย ตอนนี้เป็น `[[server.tls.cert]]`
  ตามที่ไฟล์เขียนจริง เพราะฟอร์มแก้ทีละ index
- `encodeToml` (ทางที่ Import JSON ใช้) เคยทิ้ง `h2c`, `[[server.tls.cert]]` และ `[server.tls.acme]`
  ทั้งหมด และเขียน `cert_path = ""` ออกมาเป็น config ที่ prx โหลดไม่ได้ — แก้ครบ

## ทดสอบ

- `cargo test` — `src/config_edit.rs`: แก้ค่าแล้วคอมเมนต์และจำนวนบรรทัดเท่าเดิม, สร้าง table
  ที่ยังไม่มีระหว่างทาง, เข้าถึง element ด้วย index, เคลียร์ค่าแล้วได้ไฟล์เดิมเป๊ะ, ลบทั้ง table,
  append `[[server.tls.cert]]`, ลบตัวสุดท้ายแล้ว array หายไปด้วย, และ path ที่ชี้ไปที่ไม่มีอยู่จริง
  ถูกปฏิเสธแทนที่จะเดา · `src/admin.rs`: `/web/config/edit` ไม่เขียนไฟล์และคืน report มาด้วย,
  draft ที่ยัง parse ไม่ได้ถูกปฏิเสธ, `/web/tls/acme/renew` ได้ 409 เมื่อไม่ได้เปิด ACME
  และถึงลูปจริงเมื่อเปิด
- `npm run settings:check` — ขับหน้าจริงในเบราว์เซอร์: field ที่ต้องรีสตาร์ทติดป้ายครบ,
  แก้ฟอร์มแล้วส่ง op ที่ถูกต้อง, draft bar นับ +1, editor เห็นค่าที่ฟอร์มแก้พร้อมคอมเมนต์เดิม,
  review → apply เขียนไฟล์ที่ยังมีคอมเมนต์, badge cert ใกล้หมด/หมดแล้ว, ปุ่ม order cert
  ถึง admin API, ปิด TLS ต้องยืนยัน, และแท็บ Admin บอกความจริงเรื่อง auth
- `npm run smoke` กับไบนารีจริง — round trip ครบวง: editor แสดงไฟล์ที่รันอยู่,
  แก้ field จากฟอร์ม → apply → parser จริงรับ → อ่านกลับมาแล้วค่าตรง, คอมเมนต์ในไฟล์ยังอยู่,
  และ route/service ไม่สูญเสีย field ไหนเลย
- `shell:check`, `routes:check`, `services:check`, `dashboard:check`, `config:check` ผ่านทั้งหมด
  (ทุก check ต้อง mock endpoint ของ draft เพิ่ม เพราะตอนนี้ทุกหน้าโหลด draft — topbar
  บอกว่ามี draft ค้างอยู่ไหม)

## Acceptance criteria

- [x] เปลี่ยนค่าทุกตัวแล้ว TOML ที่ได้ถูกต้องตาม schema (เทสต์ round-trip: UI → TOML → parse → UI) —
      `npm run smoke` แก้ field จาก UI แล้วอ่านกลับผ่าน parser จริง; `config_edit.rs`
      เทสต์ว่าไฟล์ที่ออกมายัง `PrxConfig::from_toml_str` ได้ทุกเคส
- [x] วันหมดอายุ cert แสดงถูกต้องจาก metric/endpoint ของ T111 — อ่านจาก `/web/tls/status`
      พร้อม badge < 30 วัน และ "expired N days ago"
- [x] เปลี่ยน admin auth มีขั้นยืนยันและมีคำเตือนความเสี่ยง — ยังไม่มี admin auth ให้เปลี่ยน (T201)
      หน้าจึงเตือนว่า API ยังไม่มี auth เลย ส่วนขั้นยืนยันสำหรับของที่อันตรายทำไว้แล้วกับการปิด
      TLS listener และการทิ้ง draft
- [x] ค่าที่ต้องรีสตาร์ทถึงจะมีผล ถูกทำเครื่องหมายไว้ชัดเจน — ป้าย "needs a restart" ต่อ field
      พร้อมคำอธิบายว่าอะไร reload แล้วมีผลทันที และ `settings:check` ตรวจว่าป้ายอยู่ครบ

## Out of scope

- จัดการผู้ใช้หลายคน
- Tuning ของ T104, access log format (T403), OTel (T402), admin auth (T201) — รอ task นั้นๆ
