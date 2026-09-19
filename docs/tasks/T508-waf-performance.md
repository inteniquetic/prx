# T508 — WAF performance: prefilter, RegexSet, perf gate

**Phase:** 5 · Plugin & WAF
**Status:** todo (รอผล T504)
**Size:** M (~1d)
**Depends on:** T507
**Files:** `src/waf/prefilter.rs`, `src/waf/engine.rs`, `benches/waf.rs`, `scripts/perf-gate.sh`

## เป้าหมาย

WAF ที่เปิดแล้วยังเป็น prx ไม่ใช่กลายเป็น nginx+modsec ที่เราพยายามจะเร็วกว่า

## สถานะปัจจุบัน

หลัง T507 WAF ถูกต้องแล้วแต่รันกฎเยอะเกินจำเป็น CRS มีกฎระดับ 900+ ข้อ
ถ้ารัน regex ทุกข้อกับทุกตัวแปรของทุก request คือจุดที่ nginx+modsec เสียเวลาไปทั้งหมด

## ขอบเขตงาน

1. **Literal prefilter ตัวเดียวทั้ง ruleset** — ดึง literal substring จาก regex ของทุกกฎ
   สร้าง `aho-corasick` automaton หนึ่งตัว สแกน input รอบเดียวได้ชุดกฎที่มีโอกาสโดน
   กฎที่ literal ไม่โดน ไม่ต้องรัน regex เลย นี่คือตัวที่เปลี่ยน O(rules) เป็น ~O(1)
2. **`RegexSet` ต่อ target** — กฎที่ยิงตัวแปรเดียวกันรวมเป็น set เดียว สแกนรอบเดียว
3. **Short-circuit** — พอ anomaly score ทะลุ threshold แล้วและโหมดเป็น blocking
   ไม่ต้องรันกฎที่เหลือใน phase นั้น (แต่โหมด detection ต้องรันครบ เพราะจุดประสงค์คือเห็นทั้งหมด)
4. **`benches/waf.rs`** — criterion bench: request ปกติ, request ที่มี body, request โจมตี
   วัดแยกระหว่าง prefilter hit และ miss
5. **ต่อเข้า `scripts/perf-gate.sh`** ให้ CI แดงได้เมื่อ WAF ทำให้ช้าลงเกิน budget
6. **วัด memory** — CRS ที่โหลดครบกินเท่าไร, ข้อมูลต่อ request เท่าไร, และมันคงที่หรือโตตาม body

## Budget ที่ต้องผ่าน

| กรณี | เพดาน |
|---|---|
| route ที่ไม่เปิด WAF | ไม่ถอยเกิน noise ของ harness |
| WAF เปิด, CRS PL1, GET ปกติไม่มี body | p99 เพิ่ม < 1 ms |
| WAF เปิด + ตรวจ body 128 KB | p99 เพิ่ม < 5 ms |
| RSS ที่ CRS โหลดครบ | < 50 MB เหนือ baseline |

ตัวเลขเหล่านี้เป็นสมมติฐานตั้งต้น **task นี้มีสิทธิ์แก้พร้อมเหตุผลและตัวเลขประกอบ** —
สิ่งที่ห้ามคือปล่อยให้ไม่มี budget เลย

## Acceptance criteria

- [ ] ผ่าน budget ทั้งสี่แถว หรือมี budget ใหม่ที่มีตัวเลขและเหตุผลกำกับ
- [ ] ก่อน/หลัง prefilter: แนบตัวเลขว่าลดการรัน regex ไปกี่ % บน corpus จริง
- [ ] ผลการตรวจจับ**ไม่เปลี่ยน**หลัง optimize — เทสต์ของ T507 ต้องผ่านเหมือนเดิมทุกข้อ
      (optimization ที่ทำให้ WAF พลาดของคือ regression ไม่ใช่ trade-off)
- [ ] perf gate แดงได้จริง (ทดสอบโดยทำให้ช้าลงจงใจ)
- [ ] RSS คงที่ภายใต้ load ต่อเนื่อง ไม่โตเรื่อย ๆ

## วิธีทดสอบ

- `cargo bench --bench waf` before/after
- `scripts/bench.sh` เทียบกับ nginx+modsecurity บน CRS ชุดเดียวกัน ถ้าเครื่องมี Docker
- soak test วัด RSS

## Out of scope

- SIMD / Hyperscan — ถ้า budget ผ่านแล้วไม่ต้องทำ ถ้าไม่ผ่านค่อยเปิดเป็น task ใหม่พร้อมตัวเลข
