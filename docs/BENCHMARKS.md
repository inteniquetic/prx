# prx benchmarks

เอกสารนี้คือแหล่งอ้างอิงเดียวของคำเคลมเรื่องประสิทธิภาพทั้งหมดของ prx
ถ้าตัวเลขไหนไม่มีในนี้ = ยังไม่ได้วัด ห้ามเอาไปเคลม

สถานะ: [T002](tasks/T002-baseline-report.md) — **micro-benchmark วัดแล้ว / proxy-vs-proxy ยังไม่ได้วัด**

## สรุปสั้น

| สิ่งที่อยากรู้ | สถานะ |
|---|---|
| prx เร็วกว่า nginx/haproxy ไหม | **ยังไม่รู้** — ต้องรัน `scripts/bench.sh --all` บนเครื่องที่มี Docker |
| route matching แพงแค่ไหน | วัดแล้ว ดูตารางด้านล่าง — และเจอปัญหาใหญ่ 1 จุด |
| reload config แพงแค่ไหน | วัดแล้ว: ~10.6ms ที่ 1000 routes |

## 1. Micro-benchmarks (วัดแล้ว)

**สภาพแวดล้อมที่วัด:** container 4 vCPU, Linux 6.18, rustc 1.94.1, release build,
criterion `--warm-up-time 1 --measurement-time 3 --sample-size 30`

> ⚠️ เครื่องที่วัดเป็น container ที่แชร์ทรัพยากร ตัวเลขสัมบูรณ์จึงใช้เทียบข้ามเครื่องไม่ได้
> แต่ **สัดส่วนระหว่างเคส** (ซึ่งเป็นประเด็นของหน้านี้) เชื่อถือได้
> ค่า baseline ดิบอยู่ที่ `bench/results/baseline/micro-bench.json`

### 1.1 `select_route` — ต้นทุนการหา route ต่อ 1 request

| เคส | 10 routes | 100 routes | 1000 routes | โตแบบไหน |
|---|---|---|---|---|
| route แรกในลิสต์ | 52 ns | 336 ns | 3.11 µs | เชิงเส้น |
| route สุดท้ายในลิสต์ | 95 ns | 480 ns | 4.27 µs | เชิงเส้น |
| ไม่มี host ตรง → default | 82 ns | 364 ns | 3.14 µs | เชิงเส้น |
| host มี port + ตัวใหญ่ | 80 ns | 366 ns | 3.15 µs | เชิงเส้น |
| **wildcard host (`*.x`)** | **667 ns** | **5.99 µs** | **59.34 µs** | **เชิงเส้น × 14 เท่าต่อ route** |

### 1.2 ฟังก์ชันย่อยใน request path

| ฟังก์ชัน | เวลา | หมายเหตุ |
|---|---|---|
| `normalize_host` (host ปกติ) | 43.0 ns | จัดสรร `String` ใหม่ทุก request |
| `normalize_host` (มี port) | 68.1 ns | จัดสรรสองครั้ง (lowercase + split) |
| `normalize_host` (IPv6) | 35.8 ns | |
| `hash_key(host, path)` | 32.6 ns | คำนวณทุก request แม้ LB ไม่ใช่ `hash` |

### 1.3 Config reload

| งาน | 10 routes | 100 routes | 1000 routes |
|---|---|---|---|
| parse + validate TOML | 93.8 µs | 889 µs | 9.29 ms |
| `RuntimeConfig::from_config` | 8.8 µs | 108 µs | 1.32 ms |
| **รวมต่อ 1 reload** | ~0.1 ms | ~1.0 ms | **~10.6 ms** |

## 2. อ่านตัวเลขข้างบนยังไง

### ปัญหาที่ 1 (ใหญ่ที่สุด): wildcard host แพงมหาศาล

`matches_host()` (`src/runtime.rs:143`) เรียก `format!(".{suffix}")` **ทุกครั้งที่เทียบกับ wildcard route หนึ่งตัว**
= จัดสรรหน่วยความจำ N ครั้งต่อ 1 request เมื่อมี N wildcard routes

ที่ 1000 wildcard routes: **59.34 µs ต่อ request เฉพาะขั้นตอนหา route**
แปลว่าเพดาน throughput ของ 1 core อยู่ที่ราว **16,800 req/s** ก่อนจะทำงานอย่างอื่นเลยด้วยซ้ำ
nginx ที่ทำ server_name hash จะไม่เจอปัญหานี้

→ แก้ที่ [T101](tasks/T101-zero-alloc-request-path.md) (ตัด `format!`) และ [T102](tasks/T102-route-matcher-index.md) (index)

### ปัญหาที่ 2: matching เป็นเชิงเส้นตามจำนวน route

ทุกเคสโตเป็นเส้นตรงตามจำนวน route ตามที่คาด เพราะ `select_route` วน `for` ทั้งตาราง (`src/runtime.rs:56-75`)

ที่ 1000 exact-host routes: 3.1–4.3 µs/request → เพดานราว 230–320k req/s ต่อ core
ยังพอไหว แต่จุดขายของ prx คือ "เพิ่ม route ผ่าน Web UI ได้เยอะๆ" ดังนั้นต้องเป็น O(1)/O(log n)

→ แก้ที่ [T102](tasks/T102-route-matcher-index.md) เป้าหมาย: ที่ 1000 routes ต้องเร็วขึ้น ≥ 10 เท่า

### ปัญหาที่ 3: allocation ต่อ request

`normalize_host` 43–68 ns คือค่าของการ allocate + lowercase ทุก request
รวมกับ `path.to_string()`, `route.name.clone()`, `snapshot.clone()` ใน `src/proxy.rs:169-220`

→ แก้ที่ [T101](tasks/T101-zero-alloc-request-path.md) และ [T103](tasks/T103-config-snapshot-access.md)

### ข่าวดี: reload ไม่ใช่ปัญหา

`RuntimeConfig::from_config` ที่ 1000 routes = 1.32 ms ยังต่ำกว่างบ 5 ms ที่ T102 ตั้งไว้มาก
ต้นทุนส่วนใหญ่ของ reload อยู่ที่ TOML parse (9.29 ms) ซึ่งเกิดนอก request path จึงไม่กระทบ traffic
แต่มีผลกับความรู้สึกตอนกด Save ใน Web UI (T204/T307 ควรเผื่อไว้ที่ ~10ms ต่อ 1000 routes)

## 3. Proxy vs nginx vs haproxy (ยังไม่ได้วัด)

harness พร้อมแล้ว (`bench/`) แต่ยังไม่มีตัวเลข เพราะสภาพแวดล้อมที่พัฒนาอยู่ไม่มี Docker

วิธีเติมตารางนี้:

```bash
scripts/bench.sh --all h1-keepalive
scripts/bench.sh --all many-conns
scripts/bench.sh --all h2
cp bench/results/*-<sha>.json bench/results/baseline/
```

| Scenario | Metric | prx | nginx | haproxy |
|---|---|---|---|---|
| h1-keepalive | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h1-keepalive | p99 | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h1-keepalive | CPU µs/req | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| many-conns (10k) | peak RSS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h2 | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| large-body (64KB) | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |

**เป้าหมายที่ตั้งไว้** (จาก `docs/tasks/README.md`): RPS ต่อ core เท่าหรือดีกว่า,
p99 ดีกว่า ≥ 20% ตอน connection reuse สูง, peak RSS ที่ 10k conns ต่ำกว่า ≥ 30%

## 4. ข้อจำกัดของวิธีวัดนี้

- วัดบนเครื่องเดียว ผ่าน loopback/bridge network — ไม่มี latency ของเครือข่ายจริง
  ผลจึงเป็น "ขอบบนของ software overhead" ไม่ใช่ตัวเลข production
- backend จำลองตอบจาก memory ทันที — งานจริงที่ backend ช้ากว่านี้มาก ส่วนแบ่งของ proxy จะเล็กลงตามไปด้วย
- ไม่ได้วัด TLS handshake (ต้องรอ [T111](tasks/T111-tls-perf-multicert.md))
- ปิด access log ทั้งหมดเพื่อความเป็นธรรม — เปิดแล้วทุกตัวจะช้าลงคนละแบบ (ดู [T403](tasks/T403-access-log-async-json.md))
- ยังไม่มีตัวเลขจากเครื่อง bare-metal

## 5. วิธี reproduce

อ่าน [`bench/README.md`](../bench/README.md) — มีข้อกำหนดเครื่อง, วิธีรัน, และเหตุผลว่าทำไมผลบนแล็ปท็อปเชื่อไม่ได้
