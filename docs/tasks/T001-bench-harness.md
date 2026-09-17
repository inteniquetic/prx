# T001 — Benchmark harness: prx vs nginx vs haproxy

**Phase:** 0 · Measurement
**Status:** todo
**Size:** M (~1d)
**Depends on:** —
**Files:** `bench/` (ใหม่), `scripts/bench.sh`, `Makefile`

## เป้าหมาย

มีชุดวัดผลที่ reproduce ได้ ใช้คำสั่งเดียวแล้วได้ตัวเลขของ prx / nginx / haproxy
บนเครื่องเดียวกัน upstream เดียวกัน เพื่อให้ทุก PR ที่อ้างว่า "เร็วขึ้น" มีหลักฐาน

## สถานะปัจจุบัน

มีแค่ `scripts/load-smoke.sh` (oha/hey ยิง `/healthz`) ซึ่งวัดได้แค่ smoke ไม่มีตัวเปรียบเทียบ
ไม่มี backend จำลอง ไม่มีการ pin CPU ไม่มี warmup ไม่เก็บ RSS

## ขอบเขตงาน

1. `bench/docker-compose.yml`: backend จำลอง 2 ตัว (nginx static หรือ `hyper` echo server ที่ตอบ 1KB/64KB),
   และ container ของ prx / nginx / haproxy ที่ config ให้ route เหมือนกันเป๊ะ
2. `bench/configs/`: `nginx.conf`, `haproxy.cfg`, `prx.toml` ที่ตั้งค่า equivalent
   (worker = N, keepalive pool เท่ากัน, access log ปิดทั้งหมดในรอบวัด)
3. `scripts/bench.sh`:
   - รับ scenario: `h1-keepalive`, `h1-close`, `h2`, `tls-h2`, `large-body`, `many-conns`
   - warmup 10s แล้วค่อยวัด 60s
   - ใช้ `oha` (HTTP/1.1) และ `h2load` (HTTP/2); ยิงจาก container แยก pin คนละ core
   - เก็บ RPS, p50/p90/p99/p999, error rate, CPU time (`/proc/<pid>/stat`), peak RSS
   - output เป็น JSON ลง `bench/results/<scenario>-<target>-<git sha>.json`
4. `make bench` และ `make bench-compare` (แสดงตารางเทียบ 3 ตัว)
5. เอกสาร `bench/README.md`: ต้องรันบนเครื่องอะไร, ปิด turbo/​powersave ยังไง, ทำไมผลบน laptop เชื่อไม่ได้

## Acceptance criteria

- [ ] `make bench` รันจบได้บนเครื่องสะอาด และได้ JSON ครบ 3 target
- [ ] รันซ้ำ 3 รอบ ผล RPS ต่างกัน < 3% (ถ้าไม่ผ่าน ต้องแก้ harness ก่อน ไม่ใช่แก้ proxy)
- [ ] มี scenario อย่างน้อย: `h1-keepalive`, `h2`, `many-conns` (10k idle conns)
- [ ] วัด RSS ของ process proxy ได้จริง ไม่ใช่ RSS ของทั้ง container

## วิธีทดสอบ

```bash
make bench SCENARIO=h1-keepalive
make bench-compare
```

## Out of scope

- CI gate (อยู่ T004)
- การ optimize ใดๆ (harness ต้องเป็นกลาง ห้ามแก้ prx ใน PR นี้)
