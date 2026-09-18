# T405 — เอกสารสถาปัตยกรรม + เผยแพร่ผล benchmark

**Phase:** 4 · Observability & Ops
**Status:** todo
**Size:** M (~1d)
**Depends on:** T004
**Files:** `docs/ARCHITECTURE.md`, `docs/BENCHMARKS.md`, `docs/MIGRATION-FROM-NGINX.md`, `README.md`

## เป้าหมาย

ให้คนอื่น (และตัวเราเองในอีก 6 เดือน) เข้าใจว่าทำไม prx เร็ว และตรวจสอบคำเคลมได้เอง

## ขอบเขตงาน

1. `docs/ARCHITECTURE.md`:
   - แผนภาพเส้นทางของ 1 request: listener → request_filter → router index → LB → upstream pool → response
   - โมเดล concurrency ของ Pingora (work-stealing runtime, shared connection pool) และทำไมมันต่างจาก worker-per-process ของ nginx
   - กลไก config reload แบบ ArcSwap (ทำไม reload ไม่ drop connection)
   - จุดที่จงใจแลก: อะไรที่เราเลือกไม่ทำ
2. `docs/BENCHMARKS.md` เวอร์ชันเผยแพร่:
   - วิธี reproduce ทีละขั้น (ใครก็รันซ้ำได้)
   - สเปกเครื่อง, เวอร์ชันของ nginx/haproxy ที่เทียบ, config ที่ใช้ทั้งหมด (ลิงก์ไฟล์จริง)
   - ตารางผลพร้อมค่า variance ไม่ใช่ตัวเลขเดียว
   - **หัวข้อ "เมื่อไหร่ prx ไม่ใช่ตัวเลือกที่ดีกว่า"** — ความน่าเชื่อถือทั้งหมดอยู่ตรงนี้
3. `docs/MIGRATION-FROM-NGINX.md`: ตารางเทียบ directive ที่ใช้บ่อย (`proxy_pass`, `upstream`, `limit_req`,
   `proxy_cache`, `add_header`, `keepalive`) → key ของ `Prx.toml`
4. README: เขียนคำเคลมใหม่ให้อิงตัวเลขจริงพร้อมลิงก์, เพิ่มสกรีนช็อต Web UI, quickstart 5 บรรทัด

## Acceptance criteria

- [ ] คนนอกทีมทำตาม `docs/BENCHMARKS.md` แล้วได้ตัวเลขใกล้เคียง (±10%)
- [ ] ทุกคำเคลมเรื่องประสิทธิภาพใน README มีลิงก์ไปตัวเลขที่ reproduce ได้
- [ ] `docs/MIGRATION-FROM-NGINX.md` ครอบคลุม directive ที่ใช้บ่อย 20 อันดับแรก
- [ ] แผนภาพใน ARCHITECTURE ตรงกับโค้ดจริง ณ commit นั้น

## Out of scope

- บล็อกโพสต์/การตลาด
