# Profiling prx

คู่มือหา "CPU หายไปไหน" ของ [T003](tasks/T003-profiling-and-microbench.md)
ใช้คู่กับ harness ใน [`bench/README.md`](../bench/README.md)

## 1. Micro-benchmark (ไม่ต้องใช้ docker)

```bash
make bench-micro                 # criterion: routing + config
cargo bench --bench routing -- select_route     # เฉพาะบางกลุ่ม
```

เก็บผลเป็น baseline สำหรับเทียบภายหลัง:

```bash
python3 scripts/bench-micro-export.py --out bench/results/baseline/micro-bench.json
```

เทียบของใหม่กับ baseline:

```bash
make bench-micro
python3 scripts/bench-micro-export.py --out bench/results/micro-bench-current.json
scripts/perf-gate.sh micro                      # exit 1 ถ้าช้าลงเกิน 5%
```

criterion เก็บ HTML report ไว้ที่ `target/criterion/report/index.html` ดูกราฟการกระจายตัวได้

## 2. Flamegraph ของ proxy จริงใต้ load

```bash
cargo install flamegraph
sudo sysctl -w kernel.perf_event_paranoid=1
scripts/profile.sh h1-keepalive
# -> bench/results/flamegraph-h1-keepalive-<sha>.svg
```

สคริปต์นี้จะ:

1. build prx ด้วย profile `profiling` (= release + debug symbols ไม่งั้น frame จะเป็นเลขที่อยู่ล้วน)
2. สตาร์ต backend จำลอง 2 ตัวจาก `bench/backend/`
3. ยิงโหลดด้วย `oha` ระหว่างที่ `perf` เก็บ sample
4. เขียน SVG ออกมา

### วิธีอ่าน

- **แกนนอน = สัดส่วนเวลา ไม่ใช่ลำดับเวลา** แท่งกว้าง = กิน CPU มาก
- ไล่จากบนลงล่างเพื่อหาว่า frame กว้างๆ ถูกเรียกจากไหน
- สิ่งที่ควรมองหาในเส้นทาง request ของ prx:
  - `__rust_alloc` / `malloc` / `free` กว้างผิดปกติ → allocation ต่อ request (ดู [T101](tasks/T101-zero-alloc-request-path.md))
  - `select_route` / `matches_host` / `format` กว้าง → route matching เป็นเชิงเส้น (ดู [T102](tasks/T102-route-matcher-index.md))
  - `arc_swap` / atomic กว้างตอน core เยอะ → contention ของ snapshot (ดู [T103](tasks/T103-config-snapshot-access.md))
  - `write`/`writev` syscall ถี่ → buffering ของ response ไม่ดี
  - `memcpy` กว้างตอน `large-body` → เส้นทาง body ยัง copy อยู่

## 3. เจาะ async task ด้วย tokio-console (เมื่อจำเป็น)

Pingora มี runtime ของตัวเอง การเปิด `tokio-console` ต้อง build ด้วย `--cfg tokio_unstable`
ซึ่งเปลี่ยนพฤติกรรม scheduler จึง **ห้ามใช้ตัวเลขจาก build นี้ในการเทียบ performance**
ใช้เพื่อดูว่า task ค้าง/ไม่ถูก wake เท่านั้น

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo build --profile profiling
```

## 4. ระเบียบการวัดที่ต้องทำตาม

1. วัด **ก่อน** แก้เสมอ และเก็บไฟล์ผลไว้
2. แก้ทีละอย่าง — ถ้าแก้ 3 อย่างพร้อมกันแล้วเร็วขึ้น 10% จะไม่มีทางรู้ว่าอันไหนได้ผล
3. รันซ้ำ 3 รอบ ถ้าผลแกว่งเกิน noise ของเครื่อง ให้แก้สภาพแวดล้อมก่อนสรุป
4. แนบตัวเลข before/after ใน PR description ทุกครั้งที่แตะ hot path

## 5. ค่าอ้างอิงปัจจุบัน

ดู [`docs/BENCHMARKS.md`](BENCHMARKS.md)
