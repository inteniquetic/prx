# T505 — SecLang parser → rule model ที่คอมไพล์แล้ว

**Phase:** 5 · Plugin & WAF
**Status:** todo (รอผล T504)
**Size:** L (~2d)
**Depends on:** T504
**Files:** `src/waf/parse.rs`, `src/waf/rule.rs`, `src/waf/mod.rs`

## เป้าหมาย

อ่านไฟล์ `.conf` ของ CRS แล้วได้โครงสร้างที่รันได้ โดยงานหนักทั้งหมดเกิดตอนโหลด ไม่ใช่ตอน request

## ขอบเขตงาน

1. Parser ของ directive ที่ CRS ใช้จริง: `SecRule`, `SecAction`, `SecMarker`, `SecRuleRemoveById`,
   `SecRuleUpdateTargetById`, `SecDefaultAction`, `SecCollectionTimeout`, `SecComponentSignature`
   — **ขอบเขตคือ "พอรัน CRS ได้" ไม่ใช่ "รองรับ SecLang ทั้งภาษา"**
2. Rule model: targets (variable + การยกเว้นด้วย `!`), operator, transformations, actions,
   phase, id, severity, tags, chain (`chain` action ที่ผูกกฎหลายข้อเข้าด้วยกัน)
3. **คอมไพล์ตอนโหลด** — regex, literal ที่จะเข้า prefilter, ชุด transformation ที่ normalize แล้ว,
   และรายการตัวแปรที่ ruleset ทั้งชุดต้องการ (ให้ T506 ใช้ทำ lazy extraction)
4. Error ต้องมีพิกัด — ไฟล์, บรรทัด, rule id เพื่อให้ Web UI (T509) ชี้ได้ว่ากฎไหนพัง
   รูปแบบเดียวกับ `ConfigIssue` ที่ `validate.rs` ใช้อยู่
5. กฎที่คอมไพล์ไม่ได้: นับ, เก็บเหตุผล, ไม่ทำให้ทั้งไฟล์ล้ม — แต่ต้องรายงานดัง ๆ ตามกติกาข้อ 5
6. `SecRuleRemoveById` และ exclusion ต้องทำงานได้ เพราะนั่นคือวิธีเดียวที่คนปรับ CRS ให้เข้ากับ app ของตัวเอง

## Acceptance criteria

- [ ] โหลด CRS ครบชุดได้ และจำนวนกฎที่รันได้ตรงกับตัวเลขที่ T504 วัดไว้ (±1%)
- [ ] เวลาโหลด CRS < 1 วินาที และ reload ไม่บล็อก request ที่วิ่งอยู่
- [ ] กฎที่พังรายงานพร้อมไฟล์/บรรทัด/id
- [ ] `chain` ทำงานถูกต้อง (กฎลูกโซ่ต้องโดนครบทุกข้อถึงจะนับ)
- [ ] round-trip test: parse แล้ว render กลับได้ความหมายเดิม สำหรับ directive ที่รองรับ

## วิธีทดสอบ

- `cargo test` — unit ต่อ directive + fixture จาก CRS จริง
- fuzz เบา ๆ บน parser: input ขยะต้องไม่ panic

## Out of scope

- รัน operator จริง (T506)
- Lua ของ ModSecurity (`SecRuleScript`) — นอกขอบเขตถาวร
