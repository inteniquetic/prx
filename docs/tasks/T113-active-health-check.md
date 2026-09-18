# T113 — Active health check prober

**Phase:** 1 · Data plane
**Status:** done
**Size:** M (~1d)
**Depends on:** —
**Files:** `src/health.rs` (ใหม่), `src/runtime.rs`, `src/config.rs`

## เป้าหมาย

ตอนนี้ upstream ที่ล่มจะถูกตัดออกก็ต่อเมื่อมี request จริงไปเจ๊งก่อน (passive) — ผู้ใช้จริงต้องรับ error
ต้องมี prober เชิงรุกที่ตัด upstream ออกก่อน และดึงกลับมาเมื่อหายดี

## สถานะปัจจุบัน

มี circuit breaker แบบ passive (`src/runtime.rs:100,310`) ตัดหลังพลาดติดกัน N ครั้ง แล้วเปิดกลับตามเวลา (`open_ms`)
ไม่มีการพิสูจน์ว่า upstream หายดีจริงก่อนส่ง traffic กลับ
`admin.rs` มี `check_upstream_health` (TCP connect) แต่เป็นการเช็คแบบ on-demand จาก UI เท่านั้น

## ขอบเขตงาน

1. Config ต่อ service:

```toml
[service.health_check]
enabled = true
type = "http"              # tcp | http
path = "/healthz"
interval_ms = 2000
timeout_ms = 1000
healthy_threshold = 2
unhealthy_threshold = 3
expected_status = [200]
```

2. Prober background task ต่อ service (task เดียวต่อ upstream, ไม่ใช่ต่อ request) อัปเดต state ผ่าน atomic
3. รวมกับ circuit breaker: half-open ต้องผ่าน prober ก่อนถึงจะรับ traffic เต็ม (ไม่ใช่ปล่อยตามเวลาอย่างเดียว)
4. ถ้า upstream ล่มหมดทุกตัว → ส่ง 503 แต่ยังคง probe ต่อ และ `/readyz` ต้องสะท้อนสถานะ (มี `is_ready()` อยู่แล้วที่ `src/runtime.rs:85`)
5. Metrics: `prx_upstream_healthy{service,addr}` (gauge 0/1), `prx_health_check_total{result}`
6. Admin API คืนสถานะสดจาก prober แทนการ TCP connect สดทุกครั้งที่ UI เปิด (เร็วกว่ามากสำหรับ UI)

## Acceptance criteria

- [ ] e2e: kill upstream → ถูกตัดออกภายใน `interval_ms × unhealthy_threshold` โดยไม่มี client เห็น error หลังจากนั้น
- [ ] upstream กลับมา → รับ traffic อีกครั้งหลังผ่าน `healthy_threshold`
- [ ] prober ไม่ทำให้ CPU idle สูงขึ้นอย่างมีนัย (วัดตอน 100 upstreams)
- [ ] reload config ระหว่าง probe ทำงานอยู่ ไม่ leak task (เทสต์นับ task)

## Out of scope

- Outlier detection แบบสถิติ (P2C/EWMA อยู่ใน T114)

## ผลลัพธ์ที่ส่งมอบ

- `src/health.rs` — prober แบบ task เดียวต่อทั้ง process อ่าน snapshot ปัจจุบันทุก tick (200ms)
  แล้วยิง probe ของ upstream ที่ถึงกำหนดแบบขนาน
  - รองรับ `kind = "tcp"` และ `kind = "http"` (เขียน HTTP/1.1 GET เองเพื่อไม่ต้องลาก client stack เข้ามา)
  - รันบน thread + runtime ของตัวเอง เหมือน config watcher จึงไม่แย่ CPU กับการ handle request
  - **reload ไม่ leak task** เพราะไม่มี task ต่อ upstream ให้ต้องเก็บกวาด
- สถานะ probe อยู่ใน `UpstreamState` (atomic ล้วน) และถูกรวมเข้ากับ `is_available_at()`
  → ทั้ง LB และ `/readyz` เห็นผลทันที
- ผ่าน probe ครบ `healthy_threshold` แล้วจะ **ล้าง circuit breaker ให้ด้วย** (half-open ที่พิสูจน์แล้วจริง
  ไม่ใช่ปล่อยกลับมาเพราะหมดเวลา)
- admin API อ่านจาก prober แทนการเปิด TCP สดทุกครั้ง — payload มี `source: "active_probe"`
  และ `last_probe_ms_ago` (ยืนยันด้วยการรันจริง)
- metrics: `prx_health_check_total{service,upstream,result}`, `prx_upstream_healthy{service,upstream}`

## บั๊กที่กันไว้ได้ตอนเขียน

`UpstreamState` เดิม `#[derive(Default)]` ถ้าปล่อยไว้ `probe_healthy` จะ default เป็น `false`
= upstream ทุกตัวถือว่าตายตั้งแต่ start และหลัง reload ทุกครั้ง traffic จะไม่วิ่งเลย
เขียน `Default` เองพร้อม comment กำกับว่าห้าม derive

## Acceptance criteria

- [x] e2e: upstream เสีย → ถูกตัดออกก่อนที่ user request จะเจอ และหลังจากนั้นไม่มี request ไปโดนอีกเลย
- [x] upstream กลับมา → รับ traffic อีกครั้งหลังผ่าน `healthy_threshold`
- [x] `/readyz` สะท้อนสถานะ (200 → 503 → 200 ตาม upstream)
- [x] reload ไม่ leak task (สถาปัตยกรรมไม่มี task ต่อ upstream)
- [ ] วัดว่า prober ไม่ทำให้ CPU idle สูงขึ้นตอน 100 upstreams — ต้องใช้ harness ของ T001

## หมายเหตุ

สถานะ probe ไม่ถูกส่งต่อข้าม reload (config ใหม่ = state ใหม่ ซึ่งเริ่มที่ healthy)
ตั้งใจให้เป็นแบบนี้เพื่อไม่ให้ reload ทำให้ traffic หยุดวิ่ง — prober จะตรวจซ้ำภายใน `interval_ms` แรก
