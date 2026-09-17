# T115 — เทสต์จริงของ WebSocket + gRPC streaming

**Phase:** 1 · Data plane
**Status:** todo
**Size:** M (~1d)
**Depends on:** —
**Files:** `tests/e2e_websocket.rs`, `tests/e2e_grpc.rs` (ใหม่), `Cargo.toml`

## เป้าหมาย

README เคลมว่า prx proxy gRPC และ websocket ได้ แต่ `tests/e2e_proxy.rs` ยังไม่มีเทสต์ทั้งสองอย่าง
— ต้องมีหลักฐาน ไม่งั้นคือความเสี่ยงที่จะพังเงียบตอน refactor hot path (T101–T103)

## ขอบเขตงาน

1. `tests/e2e_websocket.rs`:
   - upstream echo server (tokio-tungstenite) → ผ่าน prx → client
   - ตรวจ upgrade handshake, ส่ง/รับ text + binary frame, ping/pong, close frame, frame ใหญ่ (> 64KB)
   - ตรวจว่า idle websocket ไม่โดน read timeout ตัด (เคสพังคลาสสิก — ต้องมี config แยกถ้าจำเป็น)
2. `tests/e2e_grpc.rs`:
   - upstream tonic echo service → unary, server-streaming, client-streaming, bidi
   - ตรวจว่า trailer (`grpc-status`) ส่งผ่านถูกต้อง (ถ้า trailer หาย gRPC พังทั้งระบบ)
   - ตรวจ h2 prior-knowledge (gRPC ไม่ทำ upgrade)
3. ถ้าเจอว่าอันไหนยังไม่รองรับจริง → แก้ให้รองรับ หรือแก้ README ให้ตรงความจริงในงานนี้
4. เพิ่มทั้งสองไฟล์เข้า `make test` / CI

## Acceptance criteria

- [ ] เทสต์ทั้งหมดผ่านและรันใน CI ภายในเวลาที่ยอมรับได้
- [ ] มีเทสต์ยืนยันว่า websocket connection อยู่ได้นานกว่า `read_timeout_ms` ของ upstream config
- [ ] `grpc-status` trailer ถูกส่งต่อครบทั้ง success และ error case
- [ ] README ตรงกับความสามารถจริงที่เทสต์พิสูจน์แล้ว

## Out of scope

- gRPC-Web translation
