# T503 — Request body inspection pipeline

**Phase:** 5 · Plugin & WAF
**Status:** todo
**Size:** L (~2d)
**Depends on:** T501 *เฉพาะการต่อเข้า plugin phase* — ตัว `request_body_filter` เองทำก่อนได้
**Files:** `src/plugin/body.rs`, `src/proxy.rs`, `src/config.rs`

## เป้าหมาย

ให้ plugin เห็น request body ได้ โดยไม่ทำให้ proxy กลายเป็นตัวกินแรมและไม่ฆ่า streaming

## สถานะปัจจุบัน

`PrxProxy` มี `response_body_filter` แต่**ไม่มี `request_body_filter`** — body ขาไปไหลผ่านโดยไม่มีใครเห็น
WAF ตรวจ `ARGS_POST` / `REQUEST_BODY` ไม่ได้เลยถ้าไม่มีอันนี้

micro-cache สะสม response body อยู่แล้วใน `ctx.cache_pending` พร้อมธง `too_large` —
รูปแบบเดียวกันใช้ซ้ำได้ แต่ขาไปมีข้อจำกัดต่างออกไป

## ขอบเขตงาน

1. เพิ่ม `request_body_filter` เข้า `impl ProxyHttp` — **ส่วนนี้ไม่พึ่ง T501** ทำก่อนได้
   แล้วต่อเข้า phase `on_request_body` เมื่อ plugin core พร้อม (หรือเรียก WAF ตรง ๆ ไปก่อน
   ถ้าเลือกทำ WAF ก่อน plugin)
2. **Buffering ที่มีเพดานสองชั้น** — ต่อ request (`request_body_limit`) และรวมทั้ง instance
   (`body_buffer_total_limit`) ชั้นที่สองคือตัวที่กัน OOM ตอนโดนยิงพร้อมกันหลายพัน connection
   ซึ่งเพดานต่อ request อย่างเดียวกันไม่ได้
3. พฤติกรรมเมื่อเกินเพดาน เป็น config ไม่ใช่การตัดสินใจของเรา:
   `on_limit = "pass"` (ปล่อยผ่านพร้อม log — ค่าเริ่มต้น) หรือ `"block"` (413)
   ค่าเริ่มต้นเป็น `pass` เพราะ WAF ที่ทำ upload พังคือ WAF ที่ถูกปิด
4. ไม่ buffer เมื่อไม่มีใครขอ — ถ้าไม่มี plugin ที่ประกาศ `on_request_body` ในเชนของ route นั้น
   body ต้องไหลผ่านแบบ zero-copy เหมือนเดิม
5. ยกเว้น request ที่ buffer ไม่ได้อยู่แล้ว: websocket upgrade, และ body ที่ไม่มี `content-length`
   แบบ streaming ยาว — ต้องระบุพฤติกรรมให้ชัดและ log
6. `Content-Type` ที่รู้จัก (`urlencoded`, `multipart`, `json`) เตรียมไว้ให้ T506 แกะเป็นตัวแปร
   แต่ **งานนี้ยังไม่แกะ** — แค่ส่งมอบ bytes กับ content-type

## Acceptance criteria

- [ ] route ที่ไม่มี plugin สนใจ body: ไม่มี buffer, ไม่มี allocation เพิ่ม (ยืนยันด้วย bench + test)
- [ ] body ใหญ่กว่าเพดาน → พฤติกรรมตรงกับ `on_limit` และมี metric นับ
- [ ] เพดานรวมทั้ง instance ทำงานจริง: ยิง N request ใหญ่พร้อมกันแล้ว RSS ไม่ทะลุ (เทสต์ด้วยของจริง)
- [ ] websocket และ gRPC streaming ยังทำงาน (ชุดเทสต์ของ T115 ต้องผ่าน)
- [ ] chunked body ที่มาหลาย chunk ถูกประกอบครบก่อนส่งให้ plugin
- [ ] upload 100 MB ผ่าน route ที่เปิด WAF ไม่ทำให้ RSS โต 100 MB

## วิธีทดสอบ

- `cargo test` — เพดานทั้งสองชั้น, chunked, ไม่มี content-length, upgrade
- e2e: upload ใหญ่พร้อมกันหลาย connection แล้ววัด RSS
- ชุดเทสต์ WebSocket/gRPC เดิม

## Out of scope

- แกะ body เป็นตัวแปรของ WAF (`ARGS_POST`, `FILES`) — T506
- Response body inspection สำหรับ data leak — ใช้ hook ที่มีอยู่แล้ว ทำใน T507
