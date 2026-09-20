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

### สิ่งที่คู่แข่งทำได้จริง (วัดใน Phase A ของ [`WAF-BENCH-PLAN.md`](../WAF-BENCH-PLAN.md))

ตัวเลขจากแล็ปท็อป — ใช้ดูลำดับขนาดและตั้งเป้า **ห้ามเคลม** ต้องวัดซ้ำบนเครื่องอ้างอิงก่อนแก้ budget จริง

| แถว budget | nginx + ModSecurity 3.0.16 | Caddy + Coraza 2.6.1 | ผลต่อ budget |
|---|---|---|---|
| GET ปกติไม่มี body | CPU +1.35 ms/req · 68k → 1.4k rps | CPU +0.85 ms/req · 23k → 2.1k rps | budget "+1 ms p99" ยังใช้ได้ แต่เป้าที่ต้องชนะจริงคือ **WAF tax < 0.85 ms CPU/req** |
| ตรวจ body 128 KB (JSON 7,815 leaf) | 2.6 วินาที/request | 1.4 วินาที/request | budget +5 ms เข้มกว่าคู่แข่ง ~300–500 เท่า **ต้นทุนของทั้งคู่โตตามจำนวนตัวแปร ไม่ใช่จำนวน byte** — ต้องระบุ leaf count คู่กับขนาด body เสมอ |
| RSS ที่ CRS โหลดครบ | +42 MB idle | +42 MB idle · **7.8 GB** ตอนมี JSON 16 KB เข้าพร้อมกัน 64 connection | budget +50 MB ไม่ใช่เงื่อนไขชนะ → ควรเป็น **< +42 MB** และต้องวัด peak ตอนตรวจ body พร้อมกันด้วย |

ข้อที่ต้องเพิ่มใน scope เพราะสิ่งที่เห็น: WAF ที่ใช้เวลาเป็นวินาทีต่อ request คือช่องทาง DoS —
prx ต้องมีเพดานงานต่อ request (จำนวนตัวแปร และ/หรือ time budget ที่เลือก fail-open/fail-closed ได้จาก config)

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
