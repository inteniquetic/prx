# T204 — Atomic apply + auto-rollback

**Phase:** 2 · Control plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T202, T203
**Files:** `src/admin.rs`, `src/reload.rs`, `src/runtime.rs`

## เป้าหมาย

การกด Save ใน Web UI ต้องเป็น all-or-nothing: ถ้า config ใหม่ใช้ไม่ได้ ระบบต้องกลับไปสถานะเดิมเองโดยไม่มี downtime

## สถานะปัจจุบัน

`apply_config_text()` (`src/admin.rs:69`) เขียนไฟล์แล้วปล่อยให้ file watcher (`src/reload.rs`) โหลดเอง
→ มีช่วงเวลาที่ไฟล์ถูกเขียนแล้วแต่ runtime ยังไม่อัปเดต, ถ้าเขียนไม่ครบ (เครื่องดับ/disk เต็ม) ไฟล์อาจเสีย,
และถ้า build runtime ไม่สำเร็จ ผู้ใช้จะไม่รู้จาก response ของ API

## ขอบเขตงาน

1. เขียนไฟล์แบบ atomic: เขียน `Prx.toml.tmp` → `fsync` → `rename()` (atomic บน POSIX) → `fsync` ของ directory
2. ลำดับการ apply ที่ถูกต้อง:
   parse → validate (T203) → build `RuntimeConfig` (รวม index ของ T102) → เก็บ snapshot เก่าไว้ →
   `ArcSwap::store` ของใหม่ → เขียนไฟล์ → บันทึก history (T202)
   ถ้าขั้นไหนพัง: คืน snapshot เก่า + ไม่แตะไฟล์ + คืน error รายละเอียดให้ UI
3. กัน race กับ file watcher: ใส่ระบบ "ignore next N events ที่เกิดจากตัวเอง" หรือเทียบ content hash
   (ตอนนี้ apply จาก API จะไป trigger watcher อีกรอบ = reload ซ้ำซ้อน)
4. Serialize การ apply ด้วย mutex — กันสอง tab กด Save พร้อมกันแล้วเขียนทับกัน
   พร้อม optimistic concurrency: UI ส่ง `If-Match: <config-hash>` มา ถ้าไม่ตรงคืน 409 + diff
5. Response ของ apply บอก: สำเร็จไหม, reload ใช้เวลาเท่าไหร่, route/service ที่เปลี่ยน, warning ที่เจอ

## Acceptance criteria

- [ ] จำลอง apply ล้มเหลวกลางทาง → runtime ยังใช้ config เดิม ไฟล์เดิมไม่ถูกแตะ traffic ไม่ขาด
- [ ] apply สองครั้งพร้อมกัน → ครั้งหลังได้ 409 ไม่ใช่เขียนทับเงียบๆ
- [ ] apply จาก API ไม่ทำให้เกิด reload ซ้ำจาก watcher (เทสต์นับจำนวน reload)
- [ ] ระหว่าง apply มี load ยิงอยู่ → error rate = 0

## Out of scope

- Multi-node config distribution
