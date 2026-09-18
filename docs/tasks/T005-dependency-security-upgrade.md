# T005 — แก้ช่องโหว่ของ dependency ให้ `cargo audit` กลับมาเขียว

**Phase:** 0 · Measurement (ขวาง release gate จึงต้องทำก่อน)
**Status:** todo
**Size:** L (~2d)
**Depends on:** —
**Files:** `Cargo.toml`, `Cargo.lock`, `vendor/`, `ops/ZERO-EXCEPTION-POLICY.md`

## เป้าหมาย

`make gate` ผ่านครบทุกขั้นรวมถึง `cargo audit` — ตอนนี้ขั้นสุดท้ายแดง ทำให้ policy ที่เขียนไว้
ใน `ops/ZERO-EXCEPTION-POLICY.md` ไม่เป็นจริง

## สถานะปัจจุบัน (วัดแล้ว 2026-09-17)

`cargo audit` คืน `error: 1 vulnerability found!` และ `5 allowed warnings`
(เดิม 2 รายการ — `h2` ถูกแก้ไปแล้วระหว่างทำ T115)
ทั้งหมดมาจาก dependency ของ pingora 0.7 ไม่ได้มาจากโค้ดของ prx เอง
(ยืนยันแล้วว่าเกิดกับ `Cargo.lock` ก่อนเพิ่ม criterion เช่นกัน)

### Vulnerabilities

| Crate | Version | Advisory | ความรุนแรง | ทางแก้ที่ advisory ระบุ |
|---|---|---|---|---|
| `pingora-cache` | 0.7.0 | [RUSTSEC-2026-0035](https://rustsec.org/advisories/RUSTSEC-2026-0035) — cache poisoning จาก cache key ที่ไม่ปลอดภัยโดย default | **8.4 (high)** | อัปเป็น >= 0.8.0 |
| ~~`h2`~~ | ~~0.4.13~~ | ~~[RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258)~~ | — | **แก้แล้ว** — lockfile ขยับเป็น 0.4.19 ตอนเพิ่ม dev-dependency ของ T115 |

### Warnings (unmaintained / unsound)

`proc-macro-error2` (unmaintained), `anyhow` 1.0.101 (unsound `Error::downcast_mut()`),
`lru` 0.16.3 (unsound), `rand` 0.8.5 และ 0.9.2 (unsound เมื่อใช้ custom logger)

## ประเมินความเสี่ยงจริง

- **`pingora-cache`**: prx ไม่ได้เรียกใช้ cache module เลย (`grep pingora_cache src/` ว่าง)
  มันถูกดึงเข้ามาเพราะ `pingora-proxy` depend ไว้ → ความเสี่ยงจริงตอนนี้ใกล้ศูนย์
  **แต่** [T109](T109-micro-cache.md) จะทำ cache — ห้ามสร้างบนเวอร์ชันที่มีช่องโหว่นี้
- **`h2`**: แก้แล้ว (0.4.19) — และตอนนี้ HTTP/2 ถูกใช้งานจริงมากขึ้นหลัง T115 เปิด h2c และ upstream h2
- warnings ที่เหลือเป็น unsound/unmaintained ยังไม่ใช่ช่องโหว่ที่ถูก exploit ได้โดยตรง

## ขอบเขตงาน

1. ลองอัป `pingora` 0.7 → 0.8 (หรือใหม่กว่า) ดูว่า API ที่ prx ใช้เปลี่ยนตรงไหนบ้าง
   (`HttpPeer`, `ProxyHttp` trait, `Server`, `TlsSettings`, `http_proxy_service`)
2. **สำคัญ:** `Cargo.toml` patch `pingora-core` และ `pingora-load-balancing` ไปยัง `vendor/`
   ต้อง re-vendor ใหม่จากเวอร์ชันใหม่ พร้อมย้าย patch ที่ทำไว้ (รวมถึง fix `initgroups`
   ที่ใส่ไว้ตอน T001 ถ้า upstream ยังไม่แก้) และบันทึกว่าแต่ละ patch มีไว้ทำไม
3. ~~บังคับเวอร์ชัน `h2`~~ — ทำไปแล้ว เหลือแค่ `pingora-cache`
4. รัน `make gate` ให้ผ่านครบ และรัน harness ของ [T001](T001-bench-harness.md) เทียบ
   before/after เพราะการอัป pingora อาจเปลี่ยน performance อย่างมีนัย
5. ปรับ `ops/ZERO-EXCEPTION-POLICY.md` ให้ตรงความจริง: ถ้ายังมีข้อยกเว้นเหลือ ต้องระบุเป็นรายการ
   พร้อมเหตุผลและวันที่ทบทวน ไม่ใช่ประกาศว่า zero แล้วจริงๆ ไม่ zero

## Acceptance criteria

- [ ] `cargo audit` คืน exit 0 หรือมีรายการข้อยกเว้นที่ documented พร้อมวันหมดอายุ
- [ ] `make gate` ผ่านครบ 4 ขั้น
- [ ] เทสต์ทั้งหมดผ่านหลังอัป (รวม e2e)
- [ ] มีตัวเลข performance before/after ของการอัป pingora
- [ ] เหตุผลของทุก patch ใน `vendor/` ถูกบันทึกไว้

## Out of scope

- เขียน cache เอง (นั่นคือ [T109](T109-micro-cache.md))
