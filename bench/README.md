# prx benchmark harness

เครื่องมือวัดผลของ [T001](../docs/tasks/T001-bench-harness.md) สำหรับเทียบ **prx vs nginx vs haproxy**
บนงานเดียวกัน เครื่องเดียวกัน backend เดียวกัน

> ตัวเลขทุกตัวที่อ้างในโปรเจกต์นี้ต้องมาจาก harness นี้ ถ้าไม่มีไฟล์ผลลัพธ์ใน `bench/results/` แนบ = ยังไม่นับ

## สิ่งที่ต้องมี

- Docker + Docker Compose v2
- [`oha`](https://github.com/hatoo/oha) (`cargo install oha`) สำหรับ HTTP/1.1
- `h2load` (แพ็กเกจ `nghttp2-client`) สำหรับ scenario `h2`
- Python 3 (สำหรับ normalize ผลลัพธ์)

## รัน

```bash
# ทีละ target
scripts/bench.sh prx h1-keepalive
scripts/bench.sh nginx h1-keepalive
scripts/bench.sh haproxy h1-keepalive

# ครบทุก target แล้วพิมพ์ตารางเทียบ
scripts/bench.sh --all h1-keepalive

# ผ่าน make
make bench TARGET=prx SCENARIO=h1-keepalive
make bench-all SCENARIO=h2
make bench-compare SCENARIO=h1-keepalive
```

ผลลัพธ์ JSON ออกที่ `bench/results/<scenario>-<target>-<git sha>.json`

## Scenario

| Scenario | ยิงอะไร | ใช้ตอบคำถามอะไร |
|---|---|---|
| `h1-keepalive` | HTTP/1.1 keep-alive, body 1KB | throughput/latency พื้นฐาน — ตัวหลักที่ใช้เทียบ |
| `h1-close` | HTTP/1.1 ปิด connection ทุกครั้ง | ต้นทุนการ accept + setup connection ใหม่ |
| `h2` | HTTP/2 multiplex 32 stream/conn | ประสิทธิภาพเส้นทาง h2 |
| `large-body` | body 64KB | ต้นทุนการ copy/buffer body |
| `many-conns` | 10,000 connection พร้อมกัน | **memory ต่อ connection** — จุดที่ prx ควรชนะชัดที่สุด |
| `slow-upstream` | upstream หน่วง 50ms | พฤติกรรม tail latency ตอน backend ช้า |

## Env knobs

| ตัวแปร | default | ความหมาย |
|---|---|---|
| `DURATION` | 60 | วินาทีที่วัดจริง |
| `WARMUP` | 10 | วินาที warmup (ทิ้งผล) |
| `CONNECTIONS` | 64 | จำนวน connection |
| `CONNECTIONS_MANY` | 10000 | จำนวน connection ของ scenario `many-conns` |
| `BENCH_PROXY_CPUS` | `0,1` | core ที่ให้ proxy ใช้ |
| `BENCH_BACKEND_CPUS` | `2,3` | core ที่ให้ backend ใช้ |
| `KEEP_UP` | 0 | 1 = ไม่ปิด container หลังวัดเสร็จ (ไว้ profile ต่อ) |

## ความยุติธรรมของการเทียบ

harness นี้ตั้งใจให้ทั้ง 3 ตัวได้เงื่อนไขเดียวกัน:

- backend ชุดเดียวกัน (`bench/backend/`) 2 ตัว round-robin
- 4 worker threads เท่ากัน, pin CPU ชุดเดียวกัน (`cpuset`), `nofile` 65535 เท่ากัน
- **ปิด access log ทั้งหมด** (เปิดแล้วเป็นการวัด logging ไม่ใช่วัด proxy — ดู T403 ถ้าอยากวัดกรณีเปิด)
- ไม่มี retry ทั้ง 3 ตัว (`max_retries = 0` / `proxy_next_upstream off` / `retries 0`)
- keepalive ไป upstream เปิดทั้งหมด
- รันทีละ target เท่านั้น (compose profile) ไม่ให้แย่ง CPU กัน

ถ้าแก้ config ของตัวใดตัวหนึ่ง ต้องแก้อีกสองตัวให้เทียบเท่าด้วย ไม่งั้นผลที่ได้ใช้ไม่ได้

## ข้อควรระวัง (อ่านก่อนเชื่อตัวเลข)

1. **อย่าวัดบนแล็ปท็อป** — thermal throttling กับ power governor ทำให้ผลแกว่งเกิน 20%
   ใช้เครื่องที่ตั้ง governor เป็น `performance` และปิด turbo ถ้าทำได้
2. **โหลดเจนเนอเรเตอร์ต้องไม่เป็นคอขวด** — ถ้า CPU ของ `oha`/`h2load` เต็ม 100% แสดงว่ากำลังวัดตัวเอง
   ให้ยิงจากอีกเครื่องผ่าน LAN หรือลด connection ลง
3. **loopback ไม่เหมือน NIC จริง** — ผลที่ได้คือขอบบนของ software overhead ไม่ใช่ตัวเลข production
4. **รันอย่างน้อย 3 รอบ** ถ้า RPS ต่างกันเกิน 3% แปลว่า harness ยังไม่นิ่ง ให้แก้ harness ก่อน อย่าเพิ่งสรุปว่า proxy ตัวไหนเร็วกว่า
5. **Docker บน macOS/Windows รันใน VM** — ตัวเลขไม่มีความหมายสำหรับการเทียบ ใช้ Linux เท่านั้น

## โครงสร้าง

```
bench/
├── backend/           backend จำลอง (Rust, tokio) — ตอบ 1KB/64KB, มี /slow/<ms>
├── configs/           config ของ prx / nginx / haproxy ที่ตั้งให้เทียบเท่ากัน
├── docker-compose.yml compose profile ของแต่ละ target
└── results/           ผลลัพธ์ JSON (baseline/ = ผลอ้างอิงที่ commit ไว้)
```
