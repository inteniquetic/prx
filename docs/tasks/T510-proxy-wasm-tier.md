# T510 — proxy-wasm tier

**Phase:** 5 · Plugin & WAF
**Status:** todo (ทางเลือก · หรือ**ทางหลัก**ถ้า T504 บอกว่า CRS compat < 80%)
**Size:** L (~2d)
**Depends on:** T501
**Files:** `src/plugin/wasm/*`, `Cargo.toml`

## เป้าหมาย

ให้คนนอกเขียน plugin ได้โดยไม่ต้อง rebuild prx และเปิดทางให้รัน `coraza-proxy-wasm`
ซึ่งเป็น WAF ที่ compat กับ CRS อยู่แล้ว

## ทำไมต้องเป็น proxy-wasm ไม่ใช่ ABI ของเราเอง

[`proxy-wasm`](https://crates.io/crates/proxy-wasm) เป็น ABI ที่ Envoy/Istio ใช้ และมี plugin
ที่เขียนไว้แล้วจำนวนมาก — รวมถึง `coraza-proxy-wasm` การเล็ง ABI นี้แปลว่า prx
ได้ระบบนิเวศทั้งก้อนมาฟรี และได้**แผนสำรองของ WAF** มาในตัว

ถ้าเราออกแบบ ABI เอง จะได้ plugin ที่มีแค่ที่เราเขียนเอง ซึ่งไม่ต่างจากการไม่มี WASM เลย

## ขอบเขตงาน

1. wasmtime + proxy-wasm host implementation: `on_http_request_headers`, `on_http_request_body`,
   `on_http_response_headers`, `on_http_response_body`, shared data, metrics
2. เสียบเข้า `Plugin` trait ของ T501 — สำหรับ prx มันคือ plugin ตัวหนึ่ง ไม่ใช่ระบบคู่ขนาน
3. **Sandbox ที่มีขอบเขตจริง** — เพดาน memory ต่อ instance, เพดานเวลาต่อ call,
   ไม่มี filesystem/network เว้นแต่ประกาศไว้ · plugin ที่วนลูปไม่รู้จบต้องฆ่าได้ ไม่ใช่แขวนเวิร์กเกอร์
4. Instance pooling — สร้าง instance ต่อ request ไม่ไหว ต้อง pool ต่อเธรดและวัดว่าคุ้ม
5. วัดราคาที่แท้จริง: ns ต่อ call, RSS ต่อ instance, ขนาด binary ที่เพิ่ม
   ถ้าราคาสูงเกินจนขัดกับวิทยานิพนธ์ของโปรเจกต์ ให้บันทึกตัวเลขแล้วพับ — นั่นคือคำตอบที่ถูกต้อง
6. ถ้า T504 บอกให้พลิกมาทางนี้: รัน `coraza-proxy-wasm` กับ CRS ให้ผ่าน corpus เดียวกับ T507

## Acceptance criteria

- [ ] plugin proxy-wasm ที่มีอยู่แล้วจากภายนอกรันได้โดยไม่ต้องแก้
- [ ] route ที่ไม่ได้เปิด WASM ไม่จ่ายอะไรเลย (bench ยืนยัน)
- [ ] plugin ที่วนลูปไม่รู้จบถูกฆ่าโดยไม่กระทบ request อื่น
- [ ] มีตัวเลข: ns ต่อ call, RSS ต่อ instance, ขนาด binary ที่เพิ่มขึ้น
- [ ] ถ้าเป็นทางหลัก: `coraza-proxy-wasm` + CRS ผ่าน corpus ของ T507 พร้อมตัวเลข perf

## Out of scope

- WASI preview 2 / component model — ถ้า proxy-wasm พอใช้แล้วยังไม่ต้อง
- ให้ plugin WASM เข้าถึง config ของ prx ได้ — plugin เห็นแค่ request/response ของตัวเอง
