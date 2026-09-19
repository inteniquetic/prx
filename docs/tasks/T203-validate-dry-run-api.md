# T203 — Validate / dry-run API ที่คืน error แบบมีพิกัด

**Phase:** 2 · Control plane
**Status:** 🟡 partly done (ทำไปกับ [T307](T307-toml-editor-diff-apply.md))
**Size:** M (~1d)
**Depends on:** —
**Files:** `src/config.rs`, `src/admin.rs`

## เป้าหมาย

ให้ UI ชี้ได้ว่าผิดบรรทัดไหน ฟิลด์ไหน และแก้ยังไง แทนที่จะโยนข้อความ error ก้อนเดียว

## สถานะปัจจุบัน

`validate()` (`src/config.rs:36`) ใช้ `bail!` หยุดที่ error แรก และคืนแค่ string
UI จึงบอกได้แค่ "ผิด" ไม่รู้ว่าตรงไหน และผู้ใช้ต้องไล่เดาเอง

## ขอบเขตงาน

1. เปลี่ยน validate ให้ **เก็บ error ทั้งหมด** ไม่หยุดที่อันแรก คืนเป็น struct:

```json
{
  "valid": false,
  "errors": [
    { "severity": "error", "path": "route[2].service", "line": 74, "column": 11,
      "code": "unknown_service", "message": "route 'api-v3' อ้าง service 'api-v9' ที่ไม่มีอยู่",
      "hint": "services ที่มี: api-v1, api-v2" }
  ],
  "warnings": [
    { "severity": "warning", "path": "service[0].circuit_breaker.open_ms", "code": "aggressive_value",
      "message": "open_ms = 1000 ต่ำมาก อาจทำให้ flap" }
  ]
}
```

2. หา line/column จาก `toml::de::Error::span()` และ map path ของ field ให้ครบ
3. Endpoint `POST /web/config/validate` (ไม่เขียนไฟล์) + `?dry_run=true` บน `PUT` ที่คืนผลเดียวกัน
4. เพิ่ม warning ที่มีประโยชน์: ไม่มี default route, service ไม่ถูกใช้โดย route ไหนเลย, upstream ซ้ำ,
   route ทับกันจนตัวหลังไม่มีวันถูก match (ตรวจจาก index ของ T102), timeout ตั้งไว้สูงผิดปกติ
5. เทสต์ครอบทุก error code

## ทำไปแล้วใน T307

Editor ของ T307 ใช้ endpoint นี้ทุก keystroke จึงต้องทำส่วนใหญ่ของ task นี้ไปก่อน:

- `PrxConfig::check()` เก็บทุกปัญหาในรอบเดียว (`ConfigIssue`: severity, code, path, message, hint)
  `validate()` กลายเป็น wrapper ที่คืน error แรก ทางเรียกเดิมจึงไม่เปลี่ยน
- `src/validate.rs` แปลง path → บรรทัด/คอลัมน์ด้วย span ของ `toml_edit`
  (field ที่ไม่ได้เขียนในไฟล์ถอยไปใช้ table แม่; คอลัมน์นับตัวอักษรไม่ใช่ byte)
- `POST /web/config/validate` และ `PUT /web/config?dry_run=true` คืนรายงานเดียวกัน
- warning: ไม่มี default route, service ที่ไม่ถูกใช้, upstream ซ้ำ, route ที่ทับกัน,
  route ที่ปิดอยู่, service ที่ upstream ถูก drain หมด, health check timeout ≥ interval,
  upstream TLS ที่ปิด verify

เหลือของ task นี้: แยกชนิดของ TOML syntax error ให้มี `code` ละเอียดกว่า `invalid_toml`
(ตอนนี้ syntax error รายงานตัวเดียวเพราะทุกอย่างหลังจุดที่พังคือการเดา)
และ warning เรื่องค่า timeout ที่สูงผิดปกติ

## Acceptance criteria

- [x] config ที่มี 3 ข้อผิดพลาด → คืนครบ 3 รายการในครั้งเดียว
- [x] line/column ชี้ถูกต้อง (เทสต์กับไฟล์ตัวอย่าง)
- [x] ทุก error มี `code` ที่เสถียร (UI ใช้ map เป็นข้อความไทย/อังกฤษได้)
- [x] warning ไม่บล็อกการ apply แต่แสดงใน UI ได้

## Out of scope

- Auto-fix
