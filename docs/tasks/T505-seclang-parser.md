# T505 — SecLang parser → rule model ที่คอมไพล์แล้ว

**Phase:** 5 · Plugin & WAF
**Status:** ✅ done — parser + rule model ที่คอมไพล์แล้ว · สองเกณฑ์ที่ต้องมี evaluator ยกไป T506 (ดู Acceptance criteria)
**Size:** L (~2d)
**Depends on:** T504
**Files:** `src/waf/mod.rs`, `src/waf/parse.rs`, `src/waf/rule.rs`, `src/waf/pcre.rs`, `tests/waf_crs.rs`

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

## ผลลัพธ์

API ทั้งหมดคือ `prx::waf::load(&[PathBuf]) -> Loaded { ruleset, issues, summary }`
(path เป็น glob ได้ เช่น `rules/*.conf` · `load_text` สำหรับข้อความที่อยู่ในหน่วยความจำแล้ว)
แบ่งเป็นสองชั้นโดยตั้งใจ:

- `parse.rs` — **syntax ล้วน ๆ** ไม่คอมไพล์อะไรเลย ผลลัพธ์ (`Directive`) จึงเทียบด้วย `==` ได้
  และ render กลับเป็นข้อความได้ · chain ถูกจัดกลุ่มตั้งแต่ชั้นนี้
- `mod.rs` — **ความหมาย**: สืบทอด `SecDefaultAction` ต่อ phase, `t:none`, ลบกฎ / เพิ่ม target,
  resolve `skipAfter` เป็น index ใน phase ของตัวเอง, คอมไพล์ regex, intern ชุด transformation,
  และรวมรายการ collection ที่ ruleset อ่าน (`RuleSet::needs` รวม macro `%{...}` ด้วย)

เหตุผลที่แยก: regex ที่พังกลาง chain ต้องทิ้ง**ทั้ง chain** ไม่ใช่แค่ link เดียว
(ทิ้งแค่ link จะทำให้กฎข้อถัดไปถูกดูดเข้า chain เงียบ ๆ) และ fuzz ต้องไม่จ่ายค่าคอมไพล์ regex

ตัวเลข (CRS v4.21.0 = `crs-setup.conf` + `rules/*.conf` · `tests/waf_crs.rs` บังคับทุกตัว):

| | T504 | T505 |
|---|---|---|
| `SecRule` / `SecMarker` / `SecAction` / `SecDefaultAction` / `SecComponentSignature` | 678 / 30 / 8 / 2 / 1 | **ตรงกันทุกตัว** (719 directive ไม่มีอย่างอื่น) |
| กฎที่โหลดได้ (chain นับเป็นหนึ่ง) | — | **615** (54 ข้อเป็น chain · 686 link = 678 + 8) |
| กฎที่ข้าม / issue | — | **0 / 0** |
| `@rx` คอมไพล์ผ่าน | 312/312 | **312/312** |
| ในนั้นต้องผ่านชั้นซ่อม | 7 | **13** ← ไม่ตรง ดูหัวข้อถัดไป |
| ชุด transformation ที่ไม่ซ้ำกัน | — | 40 |
| เวลาโหลด release / debug | — | **86–108 ms** (3 รอบ, รอบแรก cold) / 660 ms (เป้า < 1 s) |

## สิ่งที่วัดแล้วไม่ตรงกับ T504

T504 คอมไพล์ด้วย `regex::Regex::new` ค่าเริ่มต้น คือ **Unicode mode** ซึ่ง "คอมไพล์ผ่าน"
แต่ความหมายไม่ตรงกับ PCRE ที่ ModSecurity ใช้ (ทำงานบน byte ไม่มี UTF mode):

1. `.` และ `[^x]` ใน Unicode mode เดินทีละ code point และ**หยุดที่ byte ที่ไม่ใช่ UTF-8**
   — `union.*select` ไม่ match `union\xffselect` · byte เดียวคือ bypass ของครึ่ง CRS
2. `\w` `\d` `\s` `\b` กลายเป็น Unicode ซึ่งไม่ใช่ทั้งใน PCRE และ RE2
3. กฎ **934120** เขียน UTF-8 ของ ①②③ เป็น byte ตรง ๆ (`\x{e2}\x91[\xa0-\x{bf}]`)
   ใน Unicode mode มัน**คอมไพล์ผ่านแต่หมายถึง U+00E2 (`C3 A2`)** — กฎที่จับอะไรไม่ได้อีกเลย
   โดยไม่มี error ใด ๆ

T505 จึงคอมไพล์แบบ byte (`unicode(false)`) + DOTALL + MULTILINE ตาม ModSecurity v3 / Coraza
ผลคือมีอีก **6 pattern** ที่ต้องซ่อม และชั้นซ่อมมีเพิ่มสองกฎ (ยังเป็นการแปลงเฉพาะที่และตรงกับ PCRE):
`\x{HH}` → `\xHH` (934120) และตัวอักษร non-ASCII ใน class → byte ของมัน (`[´’‘]` ใน 942420/421/430/431/432)

หลัก "ซ่อมเฉพาะ pattern ที่ใช้ตามที่เขียนไม่ได้" ยังอยู่ครบ · รายการกฎที่ถูกซ่อมถูก **pin เป็น id**
ใน `tests/waf_crs.rs` ไม่ใช่แค่นับ — CRS รุ่นใหม่ที่เพิ่มมาอีกข้อจะ fail จนกว่าจะมีคนดู

## Acceptance criteria

- [x] โหลด CRS ครบชุดได้ และจำนวนกฎที่รันได้ตรงกับตัวเลขที่ T504 วัดไว้ (±1%) — ตรงกันพอดี ไม่ใช่ ±1%
- [x] เวลาโหลด CRS < 1 วินาที — 86–108 ms (release)
- [ ] reload ไม่บล็อก request ที่วิ่งอยู่ — **ยกไป T506**: `RuleSet` immutable และ `Send + Sync`
      (มีเทสต์) พร้อมให้ `Arc` swap แต่ยังไม่ได้ต่อเข้า proxy จึงยังพิสูจน์ด้วยของจริงไม่ได้
- [x] กฎที่พังรายงานพร้อมไฟล์/บรรทัด/id — `waf::Issue` (severity, code, file, line, rule_id, message)
      serialize ได้ รูปแบบเดียวกับ `validate::Diagnostic`
- [x] `chain` — โครงสร้างถูกต้อง: กฎที่มี `chain` เป็นเจ้าของ `SecRule` ถัดไป, link ไม่มี id,
      `chain` ค้างเป็น error พร้อมพิกัด, link ที่พังทิ้งทั้ง chain
- [ ] `chain` — "ต้องโดนครบทุกข้อถึงจะนับ" เป็นเรื่องของ evaluator — **ยกไป T506**
- [x] round-trip test: parse แล้ว render กลับได้ความหมายเดิม — unit, ทุกไฟล์ของ CRS, และใน fuzz

## วิธีทดสอบ

- `cargo test` — unit ต่อ directive + fixture จาก CRS จริง
- fuzz เบา ๆ บน parser: input ขยะต้องไม่ panic — 2,000 input ต่อรอบ (< 1 s ใน debug),
  รันครั้งเดียว 100,000 input ก่อน commit: ไม่ panic และ round-trip ได้ทุกตัว
- `bash bench/waf/fetch-crs.sh` แล้ว `cargo test --test waf_crs -- --test-threads=1 --nocapture`
  · ไม่มี checkout = ข้ามพร้อมบอก · `PRX_REQUIRE_CRS=1` (CI ตั้งไว้) = fail
- เวลาโหลด: `cargo test --release --test waf_crs -- --test-threads=1 crs_loads --nocapture`

## ส่งต่อให้ T506 / T507

- **ลำดับของ removal**: `SecRuleRemoveById/ByTag` และ `SecRuleUpdateTargetById` มีผลกับกฎที่โหลด**ก่อน**มัน
  (แบบ v2 / Coraza) · วางผิดที่ = ไม่มีผล + warning `no_match` ไม่ใช่ปิดกฎเงียบ ๆ
- **`SecRuleRemoveByTag` เทียบ tag ตรงตัว** (แบบ v3 / Coraza) ไม่ใช่ regex แบบ v2
- **`SecRuleUpdateTargetById` แก้เฉพาะ link แรกของ chain**
- **`ctl:` เก็บเป็น `(option, value)` ดิบ** — `ruleRemoveTargetById=942100;ARGS:foo` ควรถูก parse
  เป็น target ตอนโหลด ไม่ใช่ตอน request · ทำพร้อมกับที่ T506 ใส่ความหมายให้มัน
- `@pmFromFile` / `@ipMatchFromFile`: ยังไม่ได้อ่านไฟล์ข้อมูล · `Rule::file` มีไว้ resolve path
- `@rx` ที่มี macro: `rx = None` + warning `rx_macro` (CRS ไม่มี)
- ยังไม่รองรับ (รายงานเป็น `unsupported_directive`): `Include`, `SecRuleUpdateActionById`,
  `SecRuleUpdateTargetByTag`, `SecRuleRemoveByMsg`, `SecRuleScript` (ถาวร)
- prefilter: ไม่ได้ทำ (T508) · จุดต่ออยู่ที่ `pcre::compile` มี `ponytail:` กำกับ

## Out of scope

- รัน operator จริง (T506)
- Lua ของ ModSecurity (`SecRuleScript`) — นอกขอบเขตถาวร
