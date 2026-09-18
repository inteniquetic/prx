# T203 — Validate / dry-run API ที่คืน error แบบมีพิกัด

**Phase:** 2 · Control plane
**Status:** todo
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

## Acceptance criteria

- [ ] config ที่มี 3 ข้อผิดพลาด → คืนครบ 3 รายการในครั้งเดียว
- [ ] line/column ชี้ถูกต้อง (เทสต์กับไฟล์ตัวอย่าง)
- [ ] ทุก error มี `code` ที่เสถียร (UI ใช้ map เป็นข้อความไทย/อังกฤษได้)
- [ ] warning ไม่บล็อกการ apply แต่แสดงใน UI ได้

## Out of scope

- Auto-fix
