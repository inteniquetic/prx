# T507 — CRS: anomaly scoring, paranoia level, การปรับจูน

**Phase:** 5 · Plugin & WAF
**Status:** todo (รอผล T504)
**Size:** L (~2d)
**Depends on:** T506
**Files:** `src/waf/engine.rs`, `src/waf/score.rs`, `src/plugin/builtin/waf.rs`, `src/config.rs`

## เป้าหมาย

WAF ที่เปิดแล้วใช้งานได้จริงกับ traffic จริง ไม่ใช่ที่บล็อกทุกอย่างแล้วอ้างว่าปลอดภัย

## สถานะปัจจุบัน

หลัง T506 มีกฎที่รันได้ทีละข้อ แต่ CRS ไม่ได้ทำงานแบบ "กฎข้อไหนโดน = บล็อก" —
มันสะสมคะแนนผ่าน `TX` ข้าม phase แล้วค่อยตัดสินใจตอนท้าย ซึ่งเป็นเหตุผลที่ CRS ใช้ได้จริง
โดยไม่พังทุกเว็บ

## ขอบเขตงาน

1. **Anomaly scoring ครบวง** — inbound/outbound score, severity → คะแนน, threshold,
   และกฎ blocking ของ CRS เอง (949110 / 959100) ที่เป็นตัวตัดสินจริง
2. **Phase ครบห้า** ตาม ModSecurity: request headers (1), request body (2), response headers (3),
   response body (4), logging (5) แมปกับ hook ของ prx ตาม §4.1 ของแผน
3. **Paranoia level** 1–4 เลือกได้จาก config พร้อมคำอธิบายว่าแต่ละระดับแลกอะไรกับอะไร
   ใน `CONFIG-WIKI.md` — ไม่ใช่แค่ตัวเลขเปล่า ๆ
4. **`mode = "detection"` เป็นค่าเริ่มต้น** ตามกติกาข้อ 4 ของแผน · `"blocking"` ต้องตั้งใจเปลี่ยนเอง
   และ UI ต้องบอกชัดว่าตอนนี้อยู่โหมดไหน
5. **Exclusion และการปรับจูน** — `SecRuleRemoveById`, `SecRuleUpdateTargetById`,
   per-route exclusion และ CRS exclusion package สำหรับ app ยอดนิยม
   นี่คือสิ่งที่ทำให้ CRS ใช้ได้จริง ถ้าปรับจูนไม่ได้ ทุกคนจะปิดมันทิ้ง
6. **Persistent collection** (`IP`, `SESSION`) — ประเมินว่าจำเป็นกับ CRS core แค่ไหน
   ถ้าจำเป็น ทำแบบ in-memory ที่มีเพดานแบบเดียวกับ `limiter.rs` ถ้าไม่จำเป็นให้บันทึกว่าไม่รองรับ
7. **Audit log** — เมื่อบล็อกหรือตรวจพบ ต้องบอกได้ว่ากฎ id ไหน, ตัวแปรไหน, ค่าอะไรที่โดน
   (พร้อม **ปิดบังข้อมูลอ่อนไหว** — audit log ที่เก็บ password ที่ผู้ใช้พิมพ์ผิดฟอร์มคือช่องโหว่ใหม่)

## Acceptance criteria

- [ ] CRS PL1 รันครบวงบน traffic จริงโดยไม่บล็อก request ที่ถูกต้อง (corpus ปกติ ต้อง 0 false positive)
- [ ] payload ชุดโจมตี (SQLi, XSS, path traversal, RCE, scanner) ถูกตรวจจับครบตามที่ CRS อ้าง
- [ ] anomaly score สะสมข้าม phase ถูกต้อง เทียบกับผลของ engine อ้างอิงใน T504
- [ ] `mode = "detection"` ไม่บล็อกอะไรเลย แต่ log ครบ
- [ ] exclusion ต่อ route ทำงาน และแก้ผ่าน config ได้
- [ ] audit log ไม่มีข้อมูลอ่อนไหวรั่ว (เทสต์ด้วย request ที่มี password/token)

## วิธีทดสอบ

- corpus สองฝั่งตามกติกาข้อ 3 ของแผน: traffic ปกติที่ต้องไม่โดน + payload โจมตีที่ต้องโดน
- เทียบผลกับ engine อ้างอิงจาก T504 รายกฎ

## Out of scope

- Perf tuning (T508) — งานนี้เอาถูกก่อน
- UI (T509)
