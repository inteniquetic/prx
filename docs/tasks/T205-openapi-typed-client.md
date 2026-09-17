# T205 — OpenAPI + JSON Schema + typed client ให้ WebUI

**Phase:** 2 · Control plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T203
**Files:** `src/admin.rs`, `docs/openapi.json`, `webui/src/lib/api/`, `webui/src/lib/types/config.ts`

## เป้าหมาย

ตัดปัญหา type ฝั่ง Rust กับ TypeScript ไม่ตรงกัน (ตอนนี้ `webui/src/lib/types/config.ts` เขียนมือ
ซึ่งจะ drift ทุกครั้งที่เพิ่ม field ใน `src/config.rs` — และเราจะเพิ่มอีกเยอะมากใน Phase 1)

## ขอบเขตงาน

1. ใช้ `utoipa` (หรือ `schemars`) annotate struct ใน `src/config.rs` + handler ใน `src/admin.rs`
2. Export `docs/openapi.json` และ JSON Schema ของ `PrxConfig` ผ่าน:
   - build step / `cargo test` ที่ fail ถ้าไฟล์ไม่ตรงกับโค้ด (กัน drift)
   - endpoint `GET /web/schema` ให้ UI ดึงไปสร้างฟอร์ม/validate ฝั่ง client ได้
3. Generate TS types อัตโนมัติ (`openapi-typescript`) ลง `webui/src/lib/types/api.d.ts` และเลิกเขียนมือ
4. เขียน API client ที่ typed ครบ + จัดการ error shape เดียวกันทั้งระบบ (ใช้ error code จาก T203)
5. เพิ่ม script `make schema` และ CI check

## Acceptance criteria

- [ ] เพิ่ม field ใหม่ใน `src/config.rs` แล้วไม่ regenerate → CI แดง
- [ ] `webui` build ผ่านด้วย types ที่ generate มา ไม่มี `any` ใน API layer
- [ ] `GET /web/schema` คืน JSON Schema ที่ validate `Prx.toml` ปัจจุบันผ่าน
- [ ] `docs/openapi.json` เปิดใน Swagger UI แล้วยิง endpoint ได้จริง

## Out of scope

- Public API versioning policy (บันทึกไว้ทำตอนจะ 1.0)
