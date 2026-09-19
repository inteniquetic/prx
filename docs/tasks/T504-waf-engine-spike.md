# T504 — Spike: วัด CRS compat แล้วเลือก engine

**Phase:** 5 · Plugin & WAF
**Status:** todo
**Size:** M (~1d)
**Depends on:** T501
**Files:** `docs/decisions/0001-waf-engine.md`, `spike/waf/` (ทิ้งได้หลังจบ)

## เป้าหมาย

ได้ตัวเลขมาตัดสินว่าจะเขียน SecLang engine เอง ใช้ crate ที่มีอยู่ หรือพลิกไป proxy-wasm
**ก่อน**ที่จะมีใครเขียน parser สักบรรทัด

## สถานะปัจจุบัน

เลือกไว้แล้วว่า rule format คือ SecLang/SecRules เพื่อให้ OWASP CRS ใช้ได้
แต่ "compatible" เป็นสเปกตรัม และยังไม่มีใครรู้ว่า prx จะไปถึงจุดไหนของสเปกตรัมนั้นได้

ข้อจำกัดที่ทำให้ต้องวัด: `regex` ของ Rust ไม่รองรับ backreference และ lookaround
เพราะนั่นคือราคาของการรับประกัน linear time (ซึ่งกับ WAF คือฟีเจอร์ ไม่ใช่ข้อจำกัด — WAF
ที่ถูก DoS ด้วย ReDoS คือช่องโหว่)

## ขอบเขตงาน

1. **ตัวเลขหลัก** — ดาวน์โหลด OWASP CRS ฉบับปัจจุบัน ดึง regex จากทุก `SecRule` แล้วนับว่า
   คอมไพล์ผ่าน `regex` ได้กี่ % · กฎที่ไม่ผ่านต้องจำแนกด้วยว่าเพราะอะไร (backreference,
   lookahead, possessive, syntax อื่น) และกฎพวกนั้นอยู่ใน paranoia level ไหน
   — กฎที่พังทั้งหมดอยู่ที่ PL4 มีความหมายต่างจากพังที่ PL1 มาก
2. **เทียบสี่ทางด้วยของจริง ไม่ใช่ด้วย README:**
   - เขียนเอง (prototype เล็ก ๆ ที่รันได้แค่ `@rx` + `@pm` พอให้วัด)
   - [`zentinel-modsec`](https://crates.io/crates/zentinel-modsec) 0.4.0
   - [`barbacane-waf`](https://crates.io/crates/barbacane-waf) 0.1.1
   - [`modsecurity`](https://crates.io/crates/modsecurity) (FFI → libmodsecurity3)
   วัด: กฎที่โหลดได้, เวลา/ความจำตอนโหลด CRS, latency ต่อ request, RSS, และ
   **ผลตรวจจับบน corpus เดียวกัน** (ตัวไหนบล็อกอะไรได้/ไม่ได้)
3. **ตรวจสถานะจริงของ upstream** — libmodsecurity3 ยังถูกดูแลอยู่แค่ไหน, CRS เวอร์ชันล่าสุดคืออะไร,
   สองcrate ฝั่ง pure-Rust มี commit ล่าสุดเมื่อไหร่และ test ครอบแค่ไหน
   **ห้ามเชื่อบรรทัดใน `PLUGIN-WAF-PLAN.md` ข้อนี้** — มันเขียนจากข้อมูลที่อาจเก่าแล้ว
4. **License และการพึ่งพา** — `zentinel-modsec` และ `barbacane-waf` เป็นส่วนหนึ่งของ proxy
   คู่แข่ง ถ้าจะใช้ต้องประเมินเรื่อง vendoring, license, และความเสี่ยงที่ upstream จะเปลี่ยนทิศ
5. **เขียน decision record** ที่ `docs/decisions/0001-waf-engine.md` — ตัวเลข, ทางที่เลือก,
   ทางที่ไม่เลือกพร้อมเหตุผล, และเงื่อนไขที่จะทำให้กลับมาทบทวนใหม่

## เกณฑ์ตัดสิน (ตั้งไว้ก่อนวัด เพื่อไม่ให้ตัวเลขถูกตีความเข้าข้างคำตอบที่อยากได้)

| CRS ที่คอมไพล์ผ่าน | ทางที่เลือก |
|---|---|
| ≥ 95% | เขียนเอง (T505–T508) · กฎที่เหลือ skip แบบดังตามกติกาข้อ 5 ของแผน |
| 80–95% | เขียนเอง + fallback engine สำหรับกฎที่เหลือ (ขอบเขตเพิ่ม ~1 task) |
| < 80% | พลิกไป T510 (proxy-wasm) แล้วรัน `coraza-proxy-wasm` |

ถ้า crate ฝั่ง pure-Rust ตัวใดตัวหนึ่งชนะทั้ง compat และ perf อย่างชัดเจน **ให้ใช้มัน** —
การเขียนเองไม่ใช่เป้าหมาย การมี WAF ที่เร็วและถูกต้องต่างหากที่เป็น

## Acceptance criteria

- [ ] มีตัวเลข % CRS compat พร้อมการจำแนกสาเหตุของกฎที่ไม่ผ่าน และแยกตาม paranoia level
- [ ] มีตาราง benchmark สี่ทางบน corpus และ hardware เดียวกัน reproduce ได้
- [ ] มี `docs/decisions/0001-waf-engine.md` ที่ระบุทางเลือกและเงื่อนไขทบทวน
- [ ] T505–T508 หรือ T510 ถูกอัปเดตให้ตรงกับผล (task ที่ไม่ได้เลือกถูกทำเครื่องหมายว่าพับ)

## วิธีทดสอบ

spike นี้ไม่ส่งโค้ดขึ้น main — ผลลัพธ์คือตัวเลขกับเอกสาร โค้ดใน `spike/` ทิ้งได้

## Out of scope

- เขียน engine จริง — งานนี้แค่ prototype พอให้วัด
