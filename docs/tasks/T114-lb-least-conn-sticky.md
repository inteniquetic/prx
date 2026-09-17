# T114 — LB: least_conn, P2C-EWMA, sticky session

**Phase:** 1 · Data plane
**Status:** todo
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
