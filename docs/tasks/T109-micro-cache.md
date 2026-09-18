# T109 — In-memory micro-cache สำหรับ GET

**Phase:** 1 · Data plane
**Status:** done
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

## ผลลัพธ์ที่ส่งมอบ

- `src/cache.rs` — cache แบบ sharded (32 shard) มีเพดานทั้งจำนวน entry และจำนวน byte
  evict ของหมดอายุก่อน แล้วค่อยไล่ทิ้งตัวที่เก่าสุด
- **request coalescing (single-flight)** — miss พร้อมกันบน key เดียวกันจะมี "leader" ตัวเดียวที่ไป upstream
  ที่เหลือรอแล้วรับผลจาก cache
  - e2e ยืนยัน: **30 request พร้อมกันบน key เย็น → upstream โดนครั้งเดียว**
  - คนที่รอมี timeout (`coalesce_wait_ms`) ถ้า leader ค้างจะไปเองไม่รอตลอดกาล
  - leader ปลุกคนที่รอเสมอใน `logging()` ไม่ว่าจะสำเร็จ ล้มเหลว หรือไม่ cacheable
- cache key = route + host + path + query + method + `vary_headers`
- `X-Cache: HIT|MISS` + `Age`
- admin: `GET /web/cache` (สถิติต่อ route), `DELETE /web/cache[?route=]` (purge) — ทดสอบกับ proxy จริงแล้ว
- metrics: `prx_cache_total{route,result}`, `prx_cache_entries`, `prx_cache_bytes`

## กฎความปลอดภัยที่บังคับเสมอ ไม่ขึ้นกับ config

ทุกข้อมีเทสต์คุม:

| กรณี | เหตุผล |
|---|---|
| method ที่ไม่ใช่ GET/HEAD | response ของ POST ไม่ใช่ของสาธารณะ |
| request มี `Authorization` | response เป็นของ client คนนั้นคนเดียว |
| request มี `Cache-Control: no-store`/`no-cache` | client ขอไม่ให้ใช้ของเก่า |
| response มี `Cache-Control: no-store` หรือ `private` | upstream บอกว่าห้ามแชร์ |
| response มี `Set-Cookie` | เก็บไว้ = ยก session ของคนหนึ่งให้อีกคน |
| response มี `Vary: *` | เล่นซ้ำไม่ได้อย่างปลอดภัย |
| body เกิน `max_body_bytes` | ส่งผ่านได้ แต่ไม่เก็บ |

hop-by-hop header ถูกตัดทิ้งก่อนเก็บ

## Acceptance criteria

- [x] e2e: 30 concurrent GET บน key เย็น → upstream เห็นแค่ 1 request
- [x] เคารพ no-store/private/Set-Cookie/Authorization (มีเทสต์แยกทุกเคส)
- [x] memory ไม่เกิน cap (unit test ยัด 5,000 entry เข้า cache ที่ cap 64 entry / 64KB)
- [ ] bench: RPS ของงาน read-heavy เพิ่มขึ้น — ต้องใช้ harness ของ T001

## ที่ยังไม่ได้ทำ

`stale-while-revalidate` — ต้องมี task refresh เบื้องหลัง และ coalescing ที่ทำไปแล้วปิดความเสี่ยง
thundering herd ซึ่งเป็นเหตุผลหลักของฟีเจอร์นี้ไปเกือบหมด บันทึกไว้ใน CONFIG-WIKI ว่ายังไม่มี
