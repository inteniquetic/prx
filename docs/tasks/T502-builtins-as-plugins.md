# T502 — ย้ายของที่ฝังตายอยู่มาเป็น plugin

**Phase:** 5 · Plugin & WAF
**Status:** todo
**Size:** M (~1d)
**Depends on:** T501
**Files:** `src/plugin/builtin/*`, `src/proxy.rs`, `src/runtime.rs`

## เป้าหมาย

พิสูจน์ว่า plugin API รองรับของจริงได้ทุกรูปแบบ **ก่อน** ที่จะมีคนนอกมาเขียนพึ่งมัน

## สถานะปัจจุบัน

header rules, rate limit, concurrency limit, micro-cache, compression ทั้งห้าตัวคือ plugin
ที่เขียนด้วยมือไว้ใน `request_filter`/`response_filter` แต่ละตัวใช้ความสามารถคนละแบบ:

| ของเดิม | ใช้ความสามารถอะไรของ plugin API |
|---|---|
| header rules | แก้ header ทั้งขาไปและขากลับ |
| rate limit | ตอบกลับเองพร้อม `Retry-After` |
| concurrency limit | **ถือ resource ที่ต้องคืนตอนจบ** ไม่ว่า request จะจบยังไง |
| micro-cache | ตอบกลับเองจาก cache + สะสม response body ขากลับ + เขียนลง store ตอนจบ |
| compression | ลงทะเบียน pingora module ตอน init ไม่ใช่ต่อ request |

ถ้า API ไม่รองรับครบห้าแบบนี้ แปลว่า API ผิด และดีกว่ารู้ตอนนี้

## ขอบเขตงาน

1. ย้ายทีละตัว ตัวละ commit โดยแต่ละ commit ต้อง `make gate` ผ่านและ bench ไม่ถอย
2. ลำดับเริ่มต้น (เมื่อไม่ได้ระบุ `plugins` ใน route) ต้องให้ผลเหมือนเดิมเป๊ะ:
   `headers → rate_limit → concurrency → cache` — **พฤติกรรมของ config ที่มีอยู่ห้ามเปลี่ยน**
3. config เดิม (`[route.rate_limit]` ฯลฯ) ต้องใช้ได้ต่อโดยไม่ต้องแก้ไฟล์ — แปลงเป็น plugin
   ภายในตอน `from_config` การบังคับให้ทุกคนเขียน config ใหม่เพื่อ refactor ภายในคือการผลักต้นทุนให้ผู้ใช้
4. compression ที่ทำงานคนละจังหวะ (pingora module) — ถ้า API รองรับไม่ได้ ให้บันทึกไว้ว่า
   ทำไมมันไม่ควรเป็น plugin แทนที่จะดัด API ให้รองรับ

## Acceptance criteria

- [ ] ห้าตัวเดิมวิ่งผ่าน plugin API (หรือมีบันทึกเหตุผลว่าตัวไหนไม่ควรย้ายและเพราะอะไร)
- [ ] config ที่มีอยู่เดิมทุกไฟล์ทำงานเหมือนเดิมโดยไม่ต้องแก้ (เทสต์ด้วย `Prx.toml` และ `scripts/configs/`)
- [ ] `scripts/bench.sh` before/after: RPS และ p99 ไม่ถอย
- [ ] `request_filter` สั้นลงอย่างมีนัยสำคัญ และลำดับการทำงานอ่านออกจาก config ไม่ใช่จากลำดับ `if`
- [ ] test เดิมของ rate limit / cache / header ทั้งหมดยังผ่านโดยไม่ต้องแก้ assertion

## วิธีทดสอบ

- `cargo test` ชุดเดิมทั้งหมด (ถ้าต้องแก้ assertion แปลว่าพฤติกรรมเปลี่ยน = ผิด)
- `scripts/bench.sh` + `scripts/load-smoke.sh`

## Out of scope

- เพิ่มความสามารถใหม่ให้ของเดิม — งานนี้คือย้าย ไม่ใช่ปรับปรุง
