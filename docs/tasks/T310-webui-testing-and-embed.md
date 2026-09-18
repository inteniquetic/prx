# T310 — Test ของ WebUI + embed pipeline ใน CI

**Phase:** 3 · Web UI
**Status:** todo
**Size:** M (~1d)
**Depends on:** T309
**Files:** `webui/`, `.github/workflows/ci.yml`, `Makefile`, `build.rs`

## เป้าหมาย

กัน UI พังเงียบ และกัน `webui/dist` ที่ commit ไว้ไม่ตรงกับ source

## สถานะปัจจุบัน

`webui/dist` ถูก commit และ embed ตอน compile (README บอกให้ rebuild เอง) — ถ้าลืม build
binary จะ serve UI เวอร์ชันเก่าโดยไม่มีใครรู้ และ `ci.yml` ยังไม่แตะ `webui/` เลย

## ขอบเขตงาน

1. Unit test (vitest): `configCodec.ts`, `configNormalize.ts`, i18n key, logic ของ route matching preview
2. E2E (Playwright) ยิงกับ binary จริง:
   - บูต prx ด้วย config ชั่วคราว → เปิด UI → สร้าง service + route → apply → ยิง proxy แล้วต้องได้ผลถูกต้อง
   - เคส error: apply config ผิด → เห็น error ที่ฟิลด์, rollback ได้
   - เคส auth: ไม่มี token → เข้าไม่ได้
3. CI job `webui`: install (bun/npm ให้เลือกอันเดียวแล้ว commit lockfile เดียว — ตอนนี้มีทั้ง `bun.lockb` และ `package-lock.json`),
   `check`, `test`, `build`
4. CI check ว่า `webui/dist` ที่ commit ตรงกับผล build (`git diff --exit-code webui/dist`)
   หรือเลิก commit dist แล้วให้ build ใน CI/Dockerfile แทน — **ตัดสินใจแล้วบันทึกเหตุผลไว้ใน `webui/README.md`**
5. `make webui` / `make webui-dev` และอัปเดต `Dockerfile` ให้ build UI ในขั้น builder

## Acceptance criteria

- [ ] E2E ผ่านใน CI และจับ regression ได้จริง (ทดสอบด้วยการทำให้พังจงใจ)
- [ ] `dist` ไม่มีทางไม่ตรงกับ source อีก (CI แดงถ้าไม่ตรง)
- [ ] เหลือ lockfile เดียว
- [ ] เวลา CI รวมเพิ่มไม่เกิน ~5 นาที

## Out of scope

- Visual regression testing (follow-up ถ้าจำเป็น)
