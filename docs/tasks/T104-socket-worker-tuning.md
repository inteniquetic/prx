# T104 — Socket & worker tuning (SO_REUSEPORT, tcp_nodelay, backlog, threads)

**Phase:** 1 · Data plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T002
**Files:** `src/main.rs`, `src/config.rs`, `Prx.toml`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

ดึงประสิทธิภาพระดับ OS ที่ nginx/haproxy ปรับกันเป็นมาตรฐาน ให้ prx ตั้งได้ผ่าน config

## สถานะปัจจุบัน

`[server]` มีแค่ `listen`, `threads`, `grace_period_seconds`, `graceful_shutdown_timeout_seconds`
(`src/config.rs:120-150`) ยังไม่มี knob ระดับ socket เลย และไม่มีเอกสารว่า `threads` ควรตั้งเท่าไหร่

## ขอบเขตงาน

1. เพิ่ม `[server.tuning]`:
   - `reuse_port` (bool, default true บน Linux) — ให้หลาย worker accept ขนานกัน ลด thundering herd
   - `tcp_nodelay` (default true), `tcp_fastopen` (backlog size, opt-in)
   - `listen_backlog` (default 4096)
   - `so_rcvbuf` / `so_sndbuf` (optional)
   - `max_connections` (soft limit → 503 เมื่อเกิน)
   - `accept_threads` / `threads` = `auto` (= จำนวน core) เป็นค่า default แทนที่จะ hardcode 4 ใน `Prx.toml`
2. ผูกค่าเหล่านี้เข้ากับ Pingora `ServerConf` / `TcpSocketOptions` ให้ครบ และ log ค่าที่ effective ตอน start
3. ตรวจ `ulimit -n` ตอน start ถ้าต่ำกว่า `max_connections` ให้ warn พร้อมวิธีแก้
4. อัปเดต `docs/CONFIG-WIKI.md` + ใส่ตัวอย่าง production ใน `docs/CONFIG-PLAYBOOK.md`

## Acceptance criteria

- [ ] scenario `many-conns` (10k idle conns): RSS และ p99 ดีขึ้นเทียบ baseline
- [ ] เปิด/ปิด `reuse_port` แล้วเห็นผลต่างใน bench และไม่พังบน macOS (ต้อง no-op อย่างสง่างาม)
- [ ] config ผิด (เช่น `listen_backlog = 0`) โดน validate ปฏิเสธพร้อมข้อความชัดเจน
- [ ] เอกสารบอกค่าที่แนะนำต่อขนาดเครื่อง

## Out of scope

- CPU pinning / thread-per-core (บันทึกเป็น follow-up ถ้า bench ชี้ว่าคุ้ม)
