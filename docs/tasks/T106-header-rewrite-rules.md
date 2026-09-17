# T106 — Header add/remove/set ต่อ route

**Phase:** 1 · Data plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** T102
**Files:** `src/config.rs`, `src/runtime.rs`, `src/proxy.rs`, `docs/CONFIG-WIKI.md`

## เป้าหมาย

งานพื้นฐานที่ทุกคนย้ายจาก nginx ต้องใช้: `X-Forwarded-For`, `X-Real-IP`, ตั้ง Host, ลบ header ภายใน,
เติม security header — และต้องทำแบบ precompiled ไม่ parse string ต่อ request

## สถานะปัจจุบัน

`upstream_request_filter()` (`src/proxy.rs:312`) มีอยู่แล้วแต่ยังไม่มี config ให้ผู้ใช้กำหนดกฎเอง
ไม่มี X-Forwarded-* เลย → ถ้า upstream ต้องรู้ client IP จริงจะใช้ไม่ได้

## ขอบเขตงาน

1. Config ต่อ route (และระดับ global default):

```toml
[route.request_headers]
set = { "X-Real-IP" = "$client_ip", "Host" = "$upstream_host" }
add = { "X-Forwarded-For" = "$client_ip" }   # append ต่อท้ายของเดิม
remove = ["X-Internal-Token"]

[route.response_headers]
set = { "X-Frame-Options" = "DENY" }
remove = ["Server"]
```

2. ตัวแปรที่รองรับ (จำกัดไว้ให้แคบ ไม่ทำ template engine): `$client_ip`, `$client_port`, `$scheme`,
   `$host`, `$route_name`, `$upstream_addr`, `$request_id`
3. Precompile เป็น `Vec<HeaderOp>` ที่มี `HeaderName` parse ไว้แล้วตอน build runtime
   (ห้าม `HeaderName::from_str` ต่อ request)
4. `X-Forwarded-For`: มีสวิตช์ `trust_proxy_hops` ว่าจะ append หรือ replace (ไม่งั้นโดนปลอม IP ได้)
5. `$request_id`: generate ถ้าไม่มี `X-Request-Id` เข้ามา และส่งต่อไป log (ต่อยอด T403)

## Acceptance criteria

- [ ] e2e test: upstream เห็น `X-Forwarded-For` ถูกต้องทั้งกรณีมี/ไม่มีของเดิม
- [ ] header ที่ `remove` ไม่หลุดไป upstream และ response header ถูกแก้จริง
- [ ] bench: overhead ของกฎ 5 ข้อ < 2% RPS
- [ ] validate ปฏิเสธชื่อ header ที่ไม่ถูกต้องตั้งแต่ตอน load config (ไม่ใช่ตอน runtime)

## Out of scope

- rewrite path / redirect (บันทึกเป็น follow-up)
