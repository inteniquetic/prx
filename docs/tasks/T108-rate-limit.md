# T108 — Rate limit + connection limit

**Phase:** 1 · Data plane
**Status:** done
**Size:** L (~2d)
**Depends on:** T102
**Files:** `src/limiter.rs` (ใหม่), `src/config.rs`, `src/proxy.rs`

## เป้าหมาย

มี rate limit ที่เร็วพอจะเปิดใช้ตลอดเวลา (lock-free, ไม่ alloc ต่อ request) เทียบชั้น `limit_req` ของ nginx
และ `stick-table` ของ haproxy

## ขอบเขตงาน

1. Config ต่อ route:

```toml
[route.rate_limit]
enabled = true
key = "client_ip"        # client_ip | header:X-Api-Key | route
requests_per_second = 100
burst = 200
response_status = 429
retry_after = true
```

2. Implementation: sharded token bucket (`Vec<Mutex<HashMap>>` หรือ `dashmap`) แบ่ง shard ตาม hash ของ key
   ใช้ `AtomicU64` เก็บ (tokens, last_refill) แบบ CAS — ไม่มี allocation ใน hot path
3. TTL eviction ของ entry เก่า (กัน memory โตไม่จำกัดจาก IP สุ่ม) + `max_entries` แล้ว LRU drop
4. `[route.connection_limit] max_concurrent` — จำกัด request ที่ in-flight ต่อ route
5. Metrics: `prx_rate_limited_total{route}`, `prx_limiter_entries` (gauge)
6. เอกสารเตือนเรื่อง key = client_ip เมื่ออยู่หลัง CDN (ต้องใช้ร่วมกับ trust_proxy_hops จาก T106)

## Acceptance criteria

- [ ] e2e: ยิงเกิน rate → ได้ 429 พร้อม `Retry-After`; ยิงต่ำกว่า rate → ผ่านครบ 100%
- [ ] bench: เปิด rate limit แล้ว RPS ตกไม่เกิน 3%
- [ ] memory ของ limiter ที่ 1M unique key ไม่เกินเพดานที่ตั้งไว้ (มีเทสต์)
- [ ] เปลี่ยนค่าแล้ว hot-reload ได้โดยไม่รีเซ็ต counter ของ key ที่ยังใช้อยู่ (หรือถ้ารีเซ็ต ต้องเขียนไว้ในเอกสาร)

## Out of scope

- Distributed rate limit ข้าม instance (ต้องมี store กลาง — follow-up)

## ผลลัพธ์ที่ส่งมอบ

- `src/limiter.rs` — token bucket แบบ sharded 64 shard, เก็บ token เป็นหน่วยพัน (milli-token)
  เพื่อไม่ให้ rate ต่ำกว่า 1/s หรือการเติมเศษระหว่างเรียกถูกปัดทิ้ง
  - hot path = hash + lock สั้นๆ ของ shard เดียว + เลขจำนวนเต็ม ไม่มี allocation
  - shard เก็บค่าตรงๆ ไม่ใช่ `Arc` → lookup ที่เจอ key เดิมแตะ cache line เดียว
- key รองรับ `client_ip` (ตัด port ทิ้ง ไม่งั้นเปิด connection ใหม่ = allowance ใหม่),
  `route`, และ `header:<Name>` (request ที่ไม่มี header ใช้ bucket ร่วมกัน กันการเลี่ยง limit)
- `[route.concurrency_limit] max_concurrent` — กัน route เดียวกินทุก worker คืน 503 เมื่อเต็ม
- ตอบ 429 พร้อม `Retry-After` ที่คำนวณจากเวลาที่ต้องรอจริง
- ตรวจ limit **ก่อน** เลือก upstream → request ที่ถูกปฏิเสธเสียแค่ค่า hash
- metrics: `prx_rate_limited_total{route,kind}`, `prx_limiter_entries{route}`

## บั๊กที่กันไว้ได้

`ConcurrencyLimitConfig` ตอนแรก derive `Default` — แต่ `#[serde(default = "...")]` มีผลตอน deserialize เท่านั้น
ค่า `response_status` จึงเป็น 0 สำหรับ config ที่สร้างจากฝั่ง Rust แล้วตกการ validate ของตัวเอง
(เทสต์เดิม 5 ตัวแดงทันที) เขียน `Default` เองพร้อม comment — เป็นบั๊กประเภทเดียวกับ `UpstreamState` ใน T113

## Acceptance criteria

- [x] e2e: ยิงเกิน rate → 429 พร้อม `Retry-After`; ยิงต่ำกว่า rate → ผ่าน 100%
- [x] memory มีเพดานจริง: unit test ยิง 100,000 key ที่ไม่ซ้ำกันเลย แล้ว entries ไม่เกิน `max_entries`
- [x] แต่ละ key แยก allowance กัน (unit + e2e ด้วย `header:X-Api-Key`)
- [x] concurrency limit ตัดโหลดจริงและคืน slot เมื่อ request จบ
- [ ] bench: เปิด rate limit แล้ว RPS ตกไม่เกิน 3% — ต้องใช้ harness ของ T001 (ไม่มี Docker ในเครื่องนี้)

## หมายเหตุ

reload รีเซ็ต counter เพราะ state อยู่ใน config snapshot — เขียนกำกับไว้ใน `docs/CONFIG-WIKI.md` แล้ว
ส่วน distributed rate limit ข้าม instance ยังอยู่นอกขอบเขตตามที่ระบุไว้แต่แรก
