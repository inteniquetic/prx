# T004 — Perf regression gate ใน CI

**Phase:** 0 · Measurement
**Status:** done
**Size:** M (~1d)
**Depends on:** T002, T003
**Files:** `.github/workflows/perf.yml`, `scripts/perf-gate.sh`

## เป้าหมาย

กัน PR ที่ทำให้ช้าลงเงียบๆ โดยไม่ทำให้ CI ปกติช้า

## ขอบเขตงาน

1. `.github/workflows/perf.yml`:
   - trigger: `schedule` (nightly) + `workflow_dispatch` + label `perf` บน PR
   - job A (เร็ว, รันทุก PR ที่มี label): `cargo bench -- --save-baseline pr` เทียบ baseline ของ main
     ผ่าน `critcmp` fail ถ้าแย่ลง > 5%
   - job B (nightly): T001 harness แบบย่อ (scenario `h1-keepalive` + `many-conns`)
     เทียบกับ `bench/results/baseline/` fail ถ้า RPS ตก > 7% หรือ RSS เพิ่ม > 10%
2. `scripts/perf-gate.sh`: อ่าน JSON ผลลัพธ์ เทียบ threshold ออก exit code + comment สรุป
3. เก็บผล nightly เป็น artifact และเขียนกลับ `bench/results/history.jsonl`
4. เอกสารสั้นๆ ใน `docs/BENCHMARKS.md`: runner ของ GitHub มี noise สูง ใช้ตัดสินแค่ regression ใหญ่ๆ

## Acceptance criteria

- [ ] PR ที่ใส่ regression จงใจ (เช่น `thread::sleep` ใน matcher) ทำให้ job A แดง
- [ ] Nightly job รันจบใน < 20 นาที
- [ ] CI ปกติ (`ci.yml`) ไม่ช้าลงเลย

## Out of scope

- Self-hosted bare-metal runner (บันทึกเป็น follow-up ถ้า noise สูงเกินใช้งาน)

## ผลลัพธ์ที่ส่งมอบ

- `.github/workflows/perf.yml` — job `micro` (nightly / manual / PR ที่ติด label `perf`) และ job `macro` (nightly, informational)
- `scripts/perf-gate.sh micro` — เทียบ criterion median กับ baseline ที่ commit ไว้ fail ถ้าช้าลงเกิน 5%
- `scripts/perf-gate.sh macro <scenario>` — เทียบ RPS/RSS กับ baseline fail ถ้า RPS ตกเกิน 7% หรือ RSS โตเกิน 10%
- ทดสอบแล้ว: ใส่ regression 30% เข้าไป gate คืน exit 1 และชี้ชื่อ benchmark ที่ช้าลง

CI ปกติ (`ci.yml`) ไม่ช้าลง — เพิ่มแค่ `cargo build --benches` เพื่อกัน bench พังเงียบ

**ยังเหลือ:** job `macro` ตั้งเป็น `continue-on-error` จนกว่าจะมี runner ที่นิ่งพอ (shared runner วัด proxy-vs-proxy ไม่ได้)
