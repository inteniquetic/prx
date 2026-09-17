# T108 — Rate limit + connection limit

**Phase:** 1 · Data plane
**Status:** todo
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
