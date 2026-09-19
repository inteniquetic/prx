# T305 — หน้า Services & Upstreams

**Phase:** 3 · Web UI
**Status:** done
**Size:** L (~2d)
**Depends on:** T304
**Files:** `webui/src/lib/components/pages/ServicesPage.svelte`, `webui/src/lib/components/services/*`, `webui/src/lib/serviceValidation.ts`, `src/admin.rs`, `src/config.rs`, `src/runtime.rs`

## เป้าหมาย

จัดการ backend pool ได้ครบ: เพิ่ม/ถอด upstream, ปรับ weight, ดูสุขภาพ, ดู circuit breaker แบบเห็นภาพ

## ขอบเขตงาน

1. Service card/table: ชื่อ, LB strategy, จำนวน upstream (healthy/total), retry setting, CB state, route ที่ใช้ service นี้
2. Upstream editor ในแต่ละ service:
   - ตาราง addr, tls, sni, weight, timeouts, สถานะสุขภาพสด, latency ล่าสุด (จาก T113/T114)
   - ปรับ weight ด้วย slider พร้อมแสดงสัดส่วนทราฟฟิกที่คาดว่าจะได้เป็น %
   - ปุ่ม "ทดสอบการเชื่อมต่อ" ต่อ upstream (ใช้ `POST /web/health/routes` ที่มีอยู่ หรือ endpoint เฉพาะ)
   - ปุ่ม drain (ตั้ง weight 0 / ถอดชั่วคราว) โดยไม่ลบ config
3. แสดง circuit breaker แบบเห็นภาพ: closed / open (นับถอยหลังเวลาที่เหลือ) / half-open พร้อมเหตุผลล่าสุด
4. Form ของ service: LB strategy (อธิบายแต่ละแบบสั้นๆ ให้คนเลือกถูก), max_retries, retry budget (T107),
   health check (T113), pool settings (T105)
5. เตือนเมื่อจะลบ service ที่ยังมี route อ้างอยู่ (บอกว่า route ไหนบ้าง และห้ามลบจนกว่าจะย้าย)

## ผลลัพธ์ที่ส่งมอบ

แยกเป็น 2 commit: ฝั่ง server แล้วค่อยฝั่ง UI

### Commit 1 — admin API

**`update_service` เคยเติม field ที่ไม่รู้จักด้วย `..Default::default()`**
แปลว่าการแก้ชื่อ service ใน UI **รีเซ็ต health check, session affinity, retry budget
และ request timeout ทิ้ง** ตอนนี้ payload ขนครบทั้งขาไปขากลับ และ block ที่ request ไม่พูดถึง
= เก็บของเดิมไว้

**upstream ได้ `enabled`** — ต้องเป็น flag ของตัวเองเพราะ balancer clamp weight เป็นอย่างน้อย 1
ดังนั้น `weight = 0` ไม่ได้ drain อะไรเลย มันยังรับทราฟฟิกอยู่เงียบๆ (มี test คุมข้อเท็จจริงนี้)
upstream ที่ drain แล้วยังอยู่ใน config และยังถูก probe จึงเอากลับมาได้จากหลักฐาน
ส่วน readiness ไม่นับมันแล้วเพราะ balancer เลือกมันไม่ได้

**`GET /web/services/status`** บอกสิ่งที่กำลังเกิดขึ้น ไม่ใช่สิ่งที่ตั้งไว้:
สถานะ circuit + เวลาที่เหลือ + จำนวน failure ที่ทำให้เปิด, ผล probe + อายุ, in-flight,
EWMA latency และ share ของแต่ละ upstream ในวงเลือก
**share อ่านจากวงจริง** ไม่ได้คำนวณจาก weight ซ้ำ — ตัวเลขที่ UI แปะข้าง slider
จึงมาจากโครงสร้างที่ balancer index เข้าไปจริงๆ และมี test เทียบกับการกระจายของ round-robin 4,000 ครั้ง

**`POST /web/upstreams/test`** probe upstream ตัวเดียว (เดิมต้อง probe ทั้ง config ผ่าน
`/web/health/routes`) และการลบ service ที่ยังมี route ชี้อยู่ตอนนี้บอกชื่อ route ที่ขวางด้วย

### Commit 2 — หน้า Services

การ์ดต่อ service: สถานะรวม, LB, ป้าย probed/sticky/breaker, upstream พร้อมกี่ตัวจากกี่ตัว,
route ที่ใช้ และตาราง upstream (share, สถานะ, in-flight, latency)

**สถานะสดโดยไม่ต้อง reload** — poll ทุก 3 วินาที หยุดเมื่อแท็บไม่ได้อยู่หน้าจอ
(ยังไม่มี event stream จนกว่า T207) `services:check` ยืนยันว่า circuit ที่เปิดขึ้น
ปรากฏบนหน้าจอเอง แล้วหายไปเองเมื่อมันกลับมาปกติ

ฟอร์ม 4 แท็บ: Upstreams (ตาราง + slider weight พร้อม % + สวิตช์ drain + ปุ่มทดสอบต่อ upstream),
Balancing (LB พร้อมคำอธิบายแต่ละแบบ + session affinity + HTTP ที่คุยกับ upstream),
Retries (max_retries, backoff, retry budget, idempotent-only, request timeout),
Health (health check + circuit breaker)

ลบ service ที่มี route ชี้อยู่ถูกบล็อกพร้อมรายชื่อ route ทั้งในฟอร์มและใน toast

### บั๊กข้อมูลหายที่ปิดจบในงานนี้

T304 แก้ฝั่ง route ไปแล้ว งานนี้ปิดฝั่ง service: `ServiceConfig`/`UpstreamConfig` ฝั่ง UI,
normalizer และ TOML codec ขนครบทุก field และเขียนเฉพาะค่าที่ต่างจาก default ของ prx

เจอเพิ่มระหว่างทาง: `normalizeLb` ฝั่ง UI รู้จักแค่ `round_robin` / `random` / `hash`
ดังนั้น config ที่ใช้ `least_conn` หรือ `p2c_ewma` จะถูกเปลี่ยนเป็น round_robin เงียบๆ ตอน Save

`npm run smoke` ตรวจ end-to-end กับ binary จริง: render TOML จาก UI → `PUT /web/config`
→ อ่านกลับมาเทียบว่า health check, sticky, circuit breaker, retry budget, upstream_h2, lb
และสถานะ drain ยังอยู่ครบ

### ที่ไม่ได้ทำตามขอบเขต

- **pool settings (T105)** — ยังไม่มีใน config เลย (T105 ยัง todo) จึงไม่มีอะไรให้ฟอร์มแก้
- **half-open** — prx ไม่มี state นี้ วงจรเปิดจนหมดเวลาแล้วกลับมาใช้เลย
  ฟอร์มเขียนความจริงข้อนี้ไว้แทนที่จะวาด state ที่ไม่มีอยู่
- **latency ย้อนหลังต่อ upstream** — out of scope ตามที่ task ระบุ (T306)

## Acceptance criteria

- [x] เพิ่ม/ลบ upstream แล้ว traffic เปลี่ยนตามจริง — CRUD ผ่าน admin API (apply ทันที),
      `/web/services/status` จาก binary จริงแสดง share 100% ให้ upstream ที่เหลือหลัง drain
- [x] ลบ service ที่มี route อ้างอยู่ → ถูกบล็อกพร้อมรายการ route ที่กระทบ (ทั้ง UI และ server)
- [x] สถานะสุขภาพอัปเดตสดโดยไม่ต้องรีเฟรชหน้า — poll 3 วินาที, `services:check` ยืนยัน
- [x] slider weight แสดงสัดส่วนตรงกับที่ LB ทำจริง — share อ่านจากวงเลือกของ balancer เอง
      + test เทียบกับการกระจายจริงของ round-robin

## Out of scope

- กราฟ latency ย้อนหลังต่อ upstream (T306)
