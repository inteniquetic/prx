# T110 — Response compression (gzip / brotli / zstd)

**Phase:** 1 · Data plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T106
**Files:** `src/config.rs`, `src/proxy.rs`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

ย้ายจาก nginx ได้โดยไม่เสียฟีเจอร์ที่ใช้อยู่ และให้ compression ไม่กลายเป็นตัวกิน CPU อันดับหนึ่ง

## ขอบเขตงาน

1. Config (global + override ต่อ route):

```toml
[compression]
enabled = true
algorithms = ["zstd", "br", "gzip"]   # เรียงตามความชอบ, เจรจากับ Accept-Encoding
level = 4
min_body_bytes = 1024
content_types = ["text/*", "application/json", "application/javascript", "image/svg+xml"]
```

2. ใช้ compression ของ Pingora ถ้ามี (ตรวจ `vendor/pingora-core` ก่อนเขียนเอง)
3. ห้าม re-compress ถ้า upstream ส่งมา compressed แล้ว; ห้าม compress ถ้า `Content-Type` ไม่อยู่ใน allowlist
4. ต้องทำงานร่วมกับ T109 ให้ถูก: cache key ต้อง vary ตาม encoding (เก็บ variant แยก หรือ cache ต้นฉบับแล้วบีบตอนตอบ — เลือกอย่างใดอย่างหนึ่งแล้วเขียนเหตุผลไว้)
5. Metrics: `prx_compression_total{algo}`, `prx_compression_bytes_saved_total`, และ CPU time ที่ใช้

## Acceptance criteria

- [ ] e2e: client ส่ง `Accept-Encoding: br, gzip` → ได้ br; ส่ง gzip อย่างเดียว → ได้ gzip; ไม่ส่ง → ไม่บีบ
- [ ] body < `min_body_bytes` ไม่ถูกบีบ
- [ ] bench: วัด RPS/CPU ทั้งเปิดและปิด และบันทึกไว้ใน `docs/BENCHMARKS.md` (ผู้ใช้ต้องรู้ว่าแลกอะไร)
- [ ] ไม่มี double-encoding ในทุกเคสที่เทสต์

## Out of scope

- Request body decompression
