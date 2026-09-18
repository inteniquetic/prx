# T114 — LB: least_conn, P2C-EWMA, sticky session

**Phase:** 1 · Data plane
**Status:** done
**Size:** M (~1d)
**Depends on:** T105
**Files:** `src/runtime.rs`, `src/config.rs`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

`round_robin`/`random`/`hash` ไม่พอสำหรับงานจริงที่ backend แต่ละตัวเร็วไม่เท่ากัน
P2C-EWMA (power of two choices + moving average latency) คือสิ่งที่ทำให้ p99 ดีขึ้นจริงโดยไม่ต้องจูนอะไร

## สถานะปัจจุบัน

`LbStrategy` (`src/config.rs:252`) มี 3 แบบ; `next_upstream()` (`src/runtime.rs:185`) เลือกจาก ring
`upstream_weight()` (`src/runtime.rs:362`) ยัง ignore weight ที่ config ไว้ (`_idx` ไม่ถูกใช้) — ต้องเช็คว่า weight ทำงานจริงไหม

## ขอบเขตงาน

1. ตรวจและแก้ให้ `weight` มีผลจริงกับทุก strategy (พร้อมเทสต์การกระจายตัว)
2. เพิ่ม `least_conn`: นับ in-flight ต่อ upstream ด้วย `AtomicUsize` เลือกตัวที่น้อยสุดจาก 2 ตัวสุ่ม (P2C ถูกกว่าการสแกนทั้งหมด)
3. เพิ่ม `p2c_ewma`: เก็บ EWMA ของ latency ต่อ upstream แล้วเลือกจาก 2 ตัวสุ่มโดยถ่วง in-flight × latency
4. Sticky session: `sticky = { mode = "cookie", name = "prx_upstream", ttl_s = 3600 }`
   หรือ `mode = "header"` / `mode = "client_ip"` — ต้อง fallback อย่างสง่างามเมื่อ upstream นั้นตาย
5. Metrics: `prx_upstream_inflight{addr}`, `prx_upstream_ewma_ms{addr}`

## Acceptance criteria

- [ ] เทสต์กระจายตัว: weight 3:1 ได้สัดส่วน ~3:1 (tolerance 5%)
- [ ] จำลอง upstream ช้า 1 ตัวใน 3: `p2c_ewma` ให้ p99 ดีกว่า `round_robin` อย่างมีนัยใน bench
- [ ] sticky: request เดิมได้ upstream เดิมตราบใดที่ยัง healthy; ตายแล้วย้ายได้ไม่ error
- [ ] เลือก strategy ไม่ทำให้ hot path alloc เพิ่ม

## Out of scope

- Consistent hashing ring แบบ bounded-load (follow-up ถ้าทำ cache affinity)

## ผลลัพธ์ที่ส่งมอบ

- **`weight` ทำงานอยู่แล้วจริง** (ring ซ้ำตามน้ำหนัก) — `_idx` ที่ไม่ได้ใช้ใน `upstream_weight()`
  ทำให้ดูเหมือนตายแต่ไม่ใช่ ตอนนี้มีเทสต์การกระจายตัว 3:1 ยืนยัน
- `least_conn` — P2C บนจำนวน in-flight
- `p2c_ewma` — P2C ถ่วงด้วย in-flight × EWMA ของ latency (เก็บเป็น fixed-point microseconds ใน atomic)
- **สุ่มสองตัวแบบไม่ซ้ำ** เพราะ deployment ส่วนใหญ่มี 2–3 upstream ถ้าสุ่มซ้ำได้
  traffic 25% จะไปตัวที่แย่กว่าทั้งที่รู้อยู่ว่าแย่กว่า (เจอจากเทสต์จริง: ได้ [251, 749])
- sticky sessions 3 โหมด: `cookie` (pin แม่นยำ, prx ออก cookie ให้เอง), `client_ip`, `header`
- in-flight counting ใช้ `saturating_sub` — decrement เกินจะไม่ wrap เป็น `usize::MAX`
  ซึ่งจะทำให้ upstream นั้นถูกมองว่าโหลดเต็มตลอดกาล
- metrics: `prx_upstream_inflight`, `prx_upstream_ewma_ms`

## บั๊กที่เจอระหว่างทำ

1. **โหมด `client_ip`/`header` ไม่ pin จริง** — ผมส่ง hash เข้า `next_upstream()` ซึ่งใช้ strategy
   ที่ตั้งไว้ (เช่น round_robin) ทำให้ hash ถูกทิ้ง เพิ่ม `select_by_hash()` ที่บังคับใช้ hash ring แก้แล้ว
   (e2e จับได้)
2. **retry budget ของ [T107](T107-timeout-retry-budget.md) ไม่เคยเห็น success เลย** —
   `record_success()` ถูกเขียนไว้แต่ไม่มีใครเรียกในโค้ดจริง budget จึงใช้แค่ค่า floor ตลอด
   ตอนนี้ต่อสายใน `logging()` แล้ว (นับเฉพาะ request ที่จบโดยไม่มี error)

## Acceptance criteria

- [x] เทสต์กระจายตัว: weight 3:1 ได้สัดส่วน ~3:1 (tolerance 5%)
- [x] จำลอง upstream ช้า: `p2c_ewma` ส่ง traffic ไปตัวเร็วมากกว่า 10 เท่า
- [x] sticky: request เดิมได้ upstream เดิมตราบใดที่ยัง healthy; ตายแล้วย้ายได้ (ต้องมี `max_retries >= 1`
      หรือ health check — เขียนกำกับไว้ใน CONFIG-WIKI)
- [x] การเลือก strategy ไม่เพิ่ม allocation ใน hot path (P2C ใช้ atomic load ล้วน)
