# T106 — Header add/remove/set ต่อ route

**Phase:** 1 · Data plane
**Status:** done
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

## ผลลัพธ์ที่ส่งมอบ

- `src/headers.rs` — กฎถูก precompile ตอนโหลด config: ชื่อ header parse ครั้งเดียว,
  ค่า template ถูกแยกเป็น literal/variable ไว้ก่อน, ค่าที่ไม่มีตัวแปรถูก render เป็น `HeaderValue` ตั้งแต่ตอน build
- รองรับ `set` / `add` / `remove` ทั้งระดับ route (`[route.request_headers]`, `[route.response_headers]`)
  และระดับ global (`[headers.request]`, `[headers.response]`) โดย global ทำงานก่อน
- ตัวแปร: `$client_ip`, `$client_port`, `$scheme`, `$host`, `$route_name`, `$upstream_addr`, `$request_id`
- `$request_id` ใช้ `X-Request-Id` ที่เข้ามาถ้ามี ไม่งั้นสร้างใหม่ — และสร้างเฉพาะเมื่อมีกฎที่ใช้จริง
- ค่า client address ถูก resolve เฉพาะเมื่อมีกฎที่ใช้ `$client_ip`/`$client_port`
- validate ตอนโหลด: ชื่อ header ที่ไม่ถูกต้อง และตัวแปรที่ไม่รู้จัก ถูกปฏิเสธทันที
- เทสต์: 7 unit ใน `src/headers.rs` + 7 e2e ใน `tests/e2e_headers.rs` (upstream สะท้อน request head กลับมาให้ตรวจจริง)

## สิ่งที่เจอระหว่างทำ

**pingora ไม่สนการเขียน `upstream_request.headers` ตรงๆ** — มันเก็บ case map แยกไว้และ serialize HTTP/1.1
จากตรงนั้น การ insert ลง `HeaderMap` ตรงๆ จึงหายไปเงียบๆ บนสาย (พิสูจน์ด้วย marker สองตัว: ตัวที่เขียนผ่าน
`insert_header()` ไปถึง upstream ส่วนตัวที่เขียนลง `.headers` ไม่ไป)
แก้ด้วย trait `HeaderSink` ที่บังคับให้ทุกการเขียนผ่าน API ของ pingora และมี implementation สำหรับ
`HeaderMap` ไว้ใช้ในเทสต์

**แถม: Web UI และ admin API ตายสนิทมาตลอด** — `admin_router()` ใช้ path syntax ของ axum 0.7 (`/admin/services/:name`)
แต่ repo ใช้ axum 0.8 ซึ่งต้องเป็น `{name}` → router panic ตั้งแต่ตอน start ทำให้ thread ของ admin ตาย
(proxy ยังวิ่งอยู่ จึงไม่มีใครสังเกต) แก้แล้ว ยืนยันว่า `/web/config`, `/admin/services`,
`/admin/services/{name}` และหน้า WebUI ตอบ 200 ครบ

## Acceptance criteria

- [x] e2e: upstream เห็น `X-Forwarded-For` ถูกต้องทั้งกรณีมี/ไม่มีของเดิม
- [x] header ที่ `remove` ไม่หลุดไป upstream และ response header ถูกแก้จริง
- [x] validate ปฏิเสธชื่อ header และตัวแปรที่ไม่ถูกต้องตั้งแต่โหลด config
- [ ] bench: overhead ของกฎ 5 ข้อ < 2% RPS — ยังวัดไม่ได้ในคอนเทนเนอร์นี้ (ต้องใช้ harness ของ T001)

## Out of scope ที่ยังค้าง

rewrite path / redirect (ตามที่ระบุไว้แต่แรก)
