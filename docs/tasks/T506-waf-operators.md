# T506 — Operators, transformations, variable extraction

**Phase:** 5 · Plugin & WAF
**Status:** todo (รอผล T504)
**Size:** L (~2d)
**Depends on:** T505
**Files:** `src/waf/operator.rs`, `src/waf/transform.rs`, `src/waf/vars.rs`

## เป้าหมาย

ทำให้กฎที่ parse แล้วตัดสินใจได้จริง กับ request จริง

## ขอบเขตงาน

1. **Operators ที่ CRS ใช้จริง** — `@rx`, `@pm`, `@pmFromFile`, `@detectSQLi`, `@detectXSS`,
   `@eq`, `@ge`, `@gt`, `@lt`, `@le`, `@streq`, `@contains`, `@beginsWith`, `@endsWith`,
   `@within`, `@ipMatch`, `@validateByteRange`, `@rbl` (หรือประกาศว่าไม่รองรับ)
   จัดลำดับตามความถี่ที่ปรากฏใน CRS ซึ่ง T504 นับไว้แล้ว
2. `@detectSQLi` / `@detectXSS` → ใช้ [`barbacane-libinjection`](https://crates.io/crates/barbacane-libinjection)
   หรือ [`libinjectionrs`](https://crates.io/crates/libinjectionrs) ไม่เขียนเอง
   ทั้งคู่เป็น port ที่ differential-test กับ C แล้ว — **แต่ T504 ต้องยืนยันคุณภาพก่อน**
3. **Transformations** — `t:none`, `lowercase`, `urlDecode`, `urlDecodeUni`, `htmlEntityDecode`,
   `removeWhitespace`, `compressWhitespace`, `removeNulls`, `replaceComments`, `normalizePath`,
   `normalizePathWin`, `base64Decode`, `hexDecode`, `length`, `utf8toUnicode`, `cmdLine`
4. **Cache ผลต่อ (ตัวแปร, ชุด transformation) ต่อ request** — ไม่ใช่ต่อกฎ นี่คือ optimization
   ที่ต้องอยู่ในดีไซน์ตั้งแต่แรกเพราะ CRS ใช้ชุด transformation ซ้ำกันมหาศาล
   ทำทีหลังแปลว่าต้องรื้อ
5. **Variable extraction แบบ lazy** — `ARGS`, `ARGS_GET`, `ARGS_POST`, `ARGS_NAMES`,
   `REQUEST_HEADERS`, `REQUEST_COOKIES`, `REQUEST_URI`, `REQUEST_BODY`, `FILES`,
   `RESPONSE_HEADERS`, `RESPONSE_BODY`, `TX`, `MATCHED_VAR` ฯลฯ
   ดึงเฉพาะที่ ruleset ที่เปิดอยู่ต้องการ (T505 คำนวณไว้ให้แล้ว)
6. **แกะ body เป็นตัวแปร** — urlencoded, multipart, JSON ต่อยอดจาก bytes ที่ T503 ส่งมา
   multipart คือจุดที่ WAF มักโดน bypass: boundary แปลก ๆ, filename ที่ encode ซ้อน,
   header ซ้ำ — ต้องมีเทสต์ของ bypass พวกนี้โดยเฉพาะ
7. `TX` collection สำหรับให้กฎคุยกัน (CRS พึ่งมันหนักมากในการทำ anomaly scoring)

## Acceptance criteria

- [ ] CRS regression suite ผ่านตามสัดส่วนที่ T504 คาดไว้
- [ ] transformation cache ทำงานจริง: วัดว่า urldecode ถูกเรียกกี่ครั้งต่อ request (ต้องเท่ากับ
      จำนวนตัวแปรที่ต่างกัน ไม่ใช่จำนวนกฎ)
- [ ] ตัวแปรที่ไม่มีกฎไหนขอ ไม่ถูกดึง (ยืนยันด้วย counter ใน test)
- [ ] multipart bypass suite: boundary ผิดรูป, encoding ซ้อน, header ซ้ำ — ตรวจจับได้ครบ
- [ ] input ขยะทุกแบบไม่ panic (fuzz)

## วิธีทดสอบ

- `cargo test` + CRS regression suite ของ upstream
- fuzz บน body parser ทั้งสามชนิด

## Out of scope

- Anomaly scoring (T507)
- Persistent collection (`IP`, `SESSION` ที่ต้องมี storage ข้าม request) — ประเมินใน T507

## ส่งต่อจาก T505 (ผลจาก review อิสระของ parser)

ของที่ T505 ตั้งใจปล่อยผ่านตอนโหลด เพราะคำศัพท์อยู่ใน task นี้ — **ต้องปิดที่นี่ ไม่ใช่ตอน request**:

- **ชื่อ operator และ transformation ที่ไม่รู้จักต้องเป็น issue ตอนโหลด** (`@thisIsNotAnOperator`, `t:lowercse`)
  ตอนนี้โหลดผ่านเงียบ ๆ transformation ที่ถูกข้ามตอน request = กฎตรวจข้อมูลที่ยังไม่ได้แปลง = หลุด
- **`@rx` ที่ argument เป็นช่องว่างล้วน** ถูก trim เหลือ pattern ว่าง (แมตช์ทุกอย่าง) — ควรเป็น error
- **`SecDefaultAction` กับ chain:** T505 ให้ทุก link ใน chain รับ `t:` จาก default; ModSecurity v2 ให้เฉพาะหัว chain
  ยังไม่ได้ยืนยันกับ v3/Coraza — CRS ไม่กระทบ (default ของ CRS ไม่มี `t:`) แต่ต้องเช็กก่อนถึงประตู verdict-equivalence
- **MULTILINE ทำให้กฎ anchored ที่ negate อ่อนลง** (`!@rx ^(?:GET|POST)$` ผ่านด้วย `GET\nevil`) — เหมือน ModSecurity ทุกประการ
  จึงไม่ใช่บั๊ก แต่ variable extraction ต้องไม่ปล่อย `\n` เข้าไปในค่าที่ไม่ควรมี (method, protocol)
- **`RuleSet::needs` คือชุดที่ถูก "อ่าน"** — การเขียน (`setvar`, `initcol`) ลง collection ที่ไม่มีใครอ่านทิ้งได้ แต่ห้ามสมมติว่า collection นั้นมีอยู่
- `{,n}` ใน regex ถูกปฏิเสธพร้อมเหตุผลตอนโหลด (PCRE2 ≥ 10.43 อ่านเป็น `{0,n}`, PCRE เก่าและ Go อ่านเป็น literal) — CRS ไม่มีใช้

