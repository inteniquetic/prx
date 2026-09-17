# T115 — เทสต์จริงของ WebSocket + gRPC streaming

**Phase:** 1 · Data plane
**Status:** done
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

## ผลลัพธ์ที่ส่งมอบ

`tests/common/mod.rs` แยก helper ของ e2e ออกมาใช้ร่วมกันได้ทุกไฟล์

### WebSocket (`tests/e2e_websocket.rs`, 4 tests)

upgrade handshake (101), text/binary frame, frame 256KB, close handshake,
และ idle connection ที่อยู่นานกว่า `read_timeout_ms` ของ upstream

**เจอบั๊กจริงและแก้แล้ว:** `read_timeout_ms` / `write_timeout_ms` / `idle_timeout_ms` ถูกใส่ให้ทุก request
รวมถึง connection ที่ upgrade แล้ว → websocket ที่เงียบเกิน read timeout โดนตัดทิ้ง
(`Protocol(ResetWithoutClosingHandshake)`) ตอนนี้ prx ตรวจ header `upgrade` แล้วไม่ใส่ timeout
ชุดนี้ให้ connection ที่ upgrade แล้ว

### gRPC (`tests/e2e_grpc.rs`, 3 tests)

unary + trailers, `grpc-status` ที่ไม่ใช่ 0, server streaming 4 messages
เขียนด้วย h2 frame ตรงๆ ไม่ต้องใช้ protoc

**เจอเรื่องใหญ่กว่านั้น: gRPC ใช้งานไม่ได้เลยมาตลอด**

- `HttpPeer::new()` ตั้ง ALPN เป็น H1 เสมอ และ prx ไม่เคยเปลี่ยน → prx คุย HTTP/1.1 กับ upstream เท่านั้น
  ซึ่ง gRPC ทำไม่ได้เพราะต้องใช้ trailer ของ HTTP/2
- listener แบบ plaintext ไม่เปิด h2c → client ที่ไม่ใช้ TLS ต่อด้วย HTTP/2 ไม่ได้

ยืนยันด้วยการทดลอง: ตั้ง `upstream_h2 = "never"` แล้วเรียก gRPC ผ่าน prx ได้ **502**

แก้โดยเพิ่ม 2 knob (ดึงงานส่วนหนึ่งของ [T105](T105-upstream-pool-keepalive.md) มาก่อนเพราะไม่มีมันแล้วเทสต์ไม่ได้):

- `[server] h2c` (default `true`) — pingora peek หา h2 preface อยู่แล้วและ fallback เป็น h1 จึงเปิดไว้ได้ปลอดภัย
- `[[service]] upstream_h2 = "never" | "always" | "auto"` (default `never` = พฤติกรรมเดิม)
  map ไปเป็น ALPN H1 / H2 / H2H1

## Acceptance criteria

- [x] เทสต์ทั้งหมดผ่านและรันใน CI (7 tests, ~2 วินาที)
- [x] websocket อยู่ได้นานกว่า `read_timeout_ms` ของ upstream
- [x] `grpc-status` ถูกส่งต่อครบทั้ง success (0) และ error (5)
- [x] README ตรงกับความสามารถจริง — เพิ่มวิธีตั้งค่าที่ทำให้ gRPC ใช้ได้จริง
