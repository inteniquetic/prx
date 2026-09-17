# T201 — Admin API: auth + bind policy + CORS

**Phase:** 2 · Control plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** —
**Files:** `src/admin.rs`, `src/config.rs`, `webui/src/lib/api/admin.ts`

## เป้าหมาย

ตอนนี้ใครก็ตามที่ต่อถึงพอร์ต admin ได้ สามารถ `PUT /web/config` เขียน config ใหม่และ redirect ทราฟฟิกทั้งระบบได้ทันที
นี่คือช่องโหว่ระดับ critical ที่ต้องปิดก่อนจะโปรโมต Web UI ให้ใครใช้

## สถานะปัจจุบัน

`admin_router()` (`src/admin.rs:1492`) ไม่มี auth layer ใดๆ; endpoint เขียนได้แก่
`PUT /web/config`, `POST/PUT/DELETE /web/services/*`, `POST/PUT/DELETE /web/routes/*`
default listen มาจาก `PRX_ADMIN_LISTEN` ซึ่งควรเป็น loopback เสมอเว้นแต่สั่งเอง

## ขอบเขตงาน

1. `[admin]` block ใน config:

```toml
[admin]
listen = "127.0.0.1:9091"
auth = "token"                    # none | token | basic
token_file = "./admin.token"      # อ่านจากไฟล์/ENV เท่านั้น ห้ามใส่ token ใน config
allow_origins = []                # CORS allowlist (ว่าง = same-origin เท่านั้น)
read_only = false
```

2. Middleware ตรวจ auth (constant-time compare) ครอบทุก route ยกเว้น static asset ของ UI
   และคืน 401 พร้อม `WWW-Authenticate` ที่ถูกต้อง
3. ถ้า `auth = "none"` และ listen ไม่ใช่ loopback → **refuse to start** พร้อมข้อความอธิบาย
4. CORS แบบ allowlist ชัดเจน (ไม่ใช่ `*`) และปฏิเสธ cross-origin write โดย default
5. ฝั่ง UI: หน้า login/token prompt เก็บใน memory + `sessionStorage`, แนบ header ทุก request, จัดการ 401 → กลับหน้า login
6. `read_only = true` ปิด endpoint ที่เขียนทั้งหมด (โหมดไว้โชว์/monitor)

## Acceptance criteria

- [ ] ทุก write endpoint คืน 401 เมื่อไม่มี token (มีเทสต์ครบทุก endpoint)
- [ ] `auth = "none"` + listen 0.0.0.0 → process ไม่ยอม start
- [ ] token ไม่เคยถูก log และไม่โผล่ใน error message
- [ ] UI ใช้งานได้ครบหลังใส่ token และจัดการ token หมดอายุได้

## Out of scope

- OIDC / SSO / multi-user (ดู T206 สำหรับ audit log ก่อน)
