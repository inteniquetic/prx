# T110 — Response compression (gzip / brotli / zstd)

**Phase:** 1 · Data plane
**Status:** done
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

## ผลลัพธ์ที่ส่งมอบ

**ใช้ของ pingora ไม่เขียนเอง** — ตรวจตามที่ task กำหนดแล้วพบว่า `vendor/pingora-core` มี
`ResponseCompressionBuilder` ที่บีบแบบ streaming พร้อม gzip/brotli/zstd (เป็น dependency ตรง ไม่ต้องเปิด feature)
การเขียน compressor เองจะได้ของที่แย่กว่า: ถ้า buffer ทั้ง body ก่อนบีบ response ใหญ่ๆ จะเสีย streaming ไป

- `[compression] enabled / level / decompress_upstream`
- ลงทะเบียนผ่าน `init_downstream_modules()` — level 0 = มี module อยู่แต่ปิดอยู่ (ค่า default ของ pingora)
- เจรจา `Accept-Encoding` ให้เอง รองรับ gzip, br, zstd
- validate `level` ต้องอยู่ใน 1–11

## ลำดับการทำงานร่วมกับ cache (T109)

compression ทำงานที่ชั้น downstream module ซึ่งอยู่ **หลัง** `response_body_filter` ของ prx
แปลว่า cache เก็บ body ดิบจาก upstream แล้วบีบตอนส่งออกทุกครั้ง (ทั้ง hit และ miss)
ผลคือ entry เดียวเสิร์ฟได้ทุก encoding และไม่ต้องเก็บซ้ำต่อ variant

## Acceptance criteria

- [x] e2e: `Accept-Encoding: gzip` → ได้ gzip, ไม่ส่ง header → ไม่บีบ
- [x] `br` และ `zstd` เจรจาได้ถูกต้อง (body 64KB → เหลือไม่ถึง 1/4)
- [x] ไม่มี double-encoding — upstream ที่ส่ง `content-encoding: gzip` มาแล้วจะถูกส่งผ่าน
- [x] compression เป็น opt-in (ปิดอยู่จนกว่าจะตั้งค่า)
- [ ] bench: วัด RPS/CPU ทั้งเปิดและปิด แล้วบันทึกใน `docs/BENCHMARKS.md` — ต้องใช้ harness ของ T001

## หมายเหตุขอบเขต

`min_body_bytes` และ `content_types` allowlist ที่ระบุไว้ตอนแรกยังไม่ได้ทำ — pingora ตัดสินใจเรื่องนี้เอง
ภายใน module ถ้าจะ override ต้องแก้ที่ `ResponseCompressionCtx` ต่อ request ซึ่งควรทำตอนมีตัวเลข
จาก bench ว่ามันคุ้มจริง ไม่ใช่เดาเอา
