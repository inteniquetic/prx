# T002 — Baseline report + docs/BENCHMARKS.md

**Phase:** 0 · Measurement
**Status:** partial
**Size:** S (~0.5d)
**Depends on:** T001
**Files:** `docs/BENCHMARKS.md`, `bench/results/baseline/`

## เป้าหมาย

รู้ว่า "วันนี้ prx อยู่ตรงไหน" เทียบ nginx/haproxy เพื่อใช้เป็นเส้นฐานของทุก optimization

## ขอบเขตงาน

1. รัน T001 harness ครบทุก scenario บนเครื่อง reference (ระบุ spec ชัดเจน: CPU model, cores, kernel, NIC)
2. บันทึกผลดิบลง `bench/results/baseline/` และ commit
3. เขียน `docs/BENCHMARKS.md`:
   - ตารางเทียบ RPS / p99 / p999 / CPU-per-request / RSS
   - กราฟหรือตาราง RSS vs จำนวน connection (1k / 10k / 50k)
   - หัวข้อ "ตอนนี้เราแพ้ตรงไหน" พร้อม hypothesis ว่าทำไม (link ไป task ที่จะแก้)
   - หัวข้อ "ข้อจำกัดของตัวเลขนี้" (single box, loopback, synthetic upstream)
4. เปิด issue/ปรับ README ของ tasks ให้ priority ตรงกับจุดที่แพ้จริง

## Acceptance criteria

- [ ] `docs/BENCHMARKS.md` มีตัวเลขจริงพร้อม git sha และ spec เครื่อง
- [ ] ทุกช่องที่ prx แพ้ มี link ไป task ที่รับผิดชอบ
- [ ] README หลักลิงก์ไป `docs/BENCHMARKS.md` และไม่มีคำเคลมเรื่องความเร็วที่ไม่มีตัวเลขรองรับ

## Out of scope

- แก้โค้ดให้เร็วขึ้น

## สถานะ: ทำได้บางส่วน

ทำแล้ว:
- `docs/BENCHMARKS.md` พร้อมตัวเลข micro-benchmark จริง + การวิเคราะห์ว่าแพ้ตรงไหนและ link ไป task ที่รับผิดชอบ
- `bench/results/baseline/micro-bench.json` — baseline ของ criterion (25 benchmarks)
- README หลัก link ไป `docs/BENCHMARKS.md`

ยังทำไม่ได้: ตัวเลข prx vs nginx vs haproxy — สภาพแวดล้อมที่พัฒนาไม่มี Docker
ต้องรัน `scripts/bench.sh --all <scenario>` บนเครื่อง reference แล้ว copy ผลเข้า `bench/results/baseline/`
