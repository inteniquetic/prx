# T402 — OpenTelemetry tracing (opt-in)

**Phase:** 4 · Observability & Ops
**Status:** todo
**Size:** M (~1d)
**Depends on:** T401
**Files:** `src/telemetry.rs` (ใหม่), `src/config.rs`, `Cargo.toml`

## เป้าหมาย

ตาม request ข้ามระบบได้ตั้งแต่ client → prx → upstream โดยที่ค่าใช้จ่ายเป็นศูนย์เมื่อปิด

## ขอบเขตงาน

1. Config:

```toml
[observability.tracing]
enabled = false
exporter = "otlp"
endpoint = "http://127.0.0.1:4317"
sample_ratio = 0.01
service_name = "prx"
```

2. Span ต่อ request: attribute = route, service, upstream addr, status, retry count, cache status
   และ span ย่อยของ upstream request
3. รองรับ W3C `traceparent`: รับต่อจาก client และส่งต่อไป upstream (ทำงานร่วมกับ header rules ของ T106)
4. Sampling: head-based ratio + บังคับ sample เมื่อ error หรือ latency เกิน threshold
5. ต้องเป็น feature flag ของ Cargo ด้วย (`--features tracing-otlp`) เพื่อไม่ให้ binary default ใหญ่ขึ้น
   และเมื่อ `enabled = false` ต้องไม่มี overhead ที่วัดได้

## Acceptance criteria

- [ ] bench: ปิด tracing แล้ว RPS เท่ากับก่อนเพิ่มฟีเจอร์ (ต่างไม่เกิน noise)
- [ ] เปิดแล้วเห็น trace ครบใน Jaeger/Tempo ทดสอบด้วย docker compose ใน `bench/` หรือ `ops/`
- [ ] `traceparent` ถูกส่งต่อไป upstream ถูกต้อง
- [ ] exporter ล่มไม่ทำให้ proxy ช้าหรือค้าง (drop span แล้วนับเป็น metric)

## Out of scope

- Tail-based sampling
