# T109 — In-memory micro-cache สำหรับ GET

**Phase:** 1 · Data plane
**Status:** todo
**Size:** L (~2d)
**Depends on:** T102
**Files:** `src/cache.rs` (ใหม่), `src/config.rs`, `src/proxy.rs`

## เป้าหมาย

micro-cache แบบ nginx `proxy_cache` ระยะสั้น (1–10 วินาที) ที่ช่วยกัน thundering herd ได้มหาศาล
ทำให้ตัวเลข RPS ในงานอ่านหนักๆ ชนะขาด และลดโหลด upstream จริง

## ขอบเขตงาน

1. Config ต่อ route:

```toml
[route.cache]
enabled = true
ttl_ms = 2000
max_body_bytes = 262144
max_entries = 10000
key = ["host", "path", "query"]     # เลือก component ได้
vary_headers = ["Accept-Encoding"]
stale_while_revalidate_ms = 5000
cache_status_codes = [200, 203, 300, 301, 404]
```

2. Storage: sharded LRU + `Bytes` (zero-copy ตอนตอบ), มี memory cap รวมเป็น bytes ไม่ใช่แค่จำนวน entry
3. Request coalescing (single-flight): request ที่ miss พร้อมกันบน key เดียว ยิง upstream แค่ 1 ครั้ง
   ตัวอื่นรอผล — นี่คือส่วนที่ได้ผลจริงที่สุด
4. เคารพ `Cache-Control: no-store/private` ของ upstream และ `Authorization` ใน request (default = ไม่ cache)
5. `stale-while-revalidate`: ตอบของเก่าไปก่อนแล้ว refresh เบื้องหลัง
6. Response header `X-Cache: HIT|MISS|STALE` (เปิด/ปิดได้)
7. Metrics: `prx_cache_hit_total`, `prx_cache_miss_total`, `prx_cache_bytes`, `prx_cache_evictions_total`
8. Admin: `DELETE /web/cache?route=...` purge (ผูกกับ auth ของ T201)

## Acceptance criteria

- [ ] e2e: 100 concurrent GET บน key เดียวตอน cold → upstream เห็นแค่ 1 request
- [ ] เคารพ no-store/private/Authorization (มีเทสต์แต่ละเคส)
- [ ] memory ไม่เกิน cap ที่ตั้งไว้ภายใต้ load ผสม (เทสต์)
- [ ] bench: scenario read-heavy RPS เพิ่มขึ้นชัดเจน และ p99 ลดลง

## Out of scope

- Disk cache / shared cache ข้าม instance
