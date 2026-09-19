# T306 — Dashboard สถิติสด

**Phase:** 3 · Web UI
**Status:** done
**Size:** L (~2d)
**Depends on:** T303, T207
**Files:** `webui/src/lib/components/pages/DashboardPage.svelte`, `webui/src/lib/components/dashboard/*`, `webui/src/lib/api/stats.ts`, `webui/src/lib/stores/stats.ts`, `src/stats.rs`, `src/metrics.rs`, `src/admin.rs`

## เป้าหมาย

เปิดหน้าแรกแล้วรู้ทันทีว่าระบบปกติหรือไม่ และถ้าไม่ปกติ ปัญหาอยู่ที่ route/upstream ไหน

## สถานะปัจจุบัน

`DashboardPage` แสดงสรุป config (`ServerCard`, `StatsCard`, `RouteTableCard`, `ObservabilityCard`)
ซึ่งเป็นข้อมูล "ตั้งค่าอะไรไว้" ไม่ใช่ "ตอนนี้เกิดอะไรขึ้น" — ต้องเพิ่มข้อมูล runtime จาก T207

## ขอบเขตงาน

1. แถวบน `MetricTile`: RPS, p99 latency, error rate (4xx/5xx แยกกัน), active connections,
   upstream healthy / total, cache hit ratio (ถ้าเปิด T109) — แต่ละอันมี sparkline 5 นาทีและ delta เทียบ 1 นาทีก่อน
2. กราฟหลัก: RPS + error rate ตามเวลา และ latency percentile (p50/p95/p99) — ใช้ไลบรารีขนาดเล็ก
   (เช่น `layerchart`/`uPlot`) ที่ self-host ได้, ต้องไม่กระตุกตอนอัปเดตทุกวินาที
3. ตาราง "Top routes" เรียงตาม traffic และ "Worst routes" เรียงตาม error rate / p99
4. แผงสุขภาพ upstream: grid ของจุดสถานะ คลิกแล้วไปหน้า service นั้น
5. แถบเหตุการณ์ล่าสุด: config apply, reload สำเร็จ/ล้มเหลว, circuit breaker trip, cert ใกล้หมดอายุ
   (ดึงจาก audit ของ T206 + event ของ runtime)
6. สถานะ degraded ต้องเด่นชัด: แถบสีด้านบนพร้อมข้อความว่าเกิดอะไรและลิงก์ไปจุดที่ต้องแก้

## ผลลัพธ์ที่ส่งมอบ

แยกเป็น 2 commit: ฝั่ง server (T207 ซึ่งเป็น dependency ที่ยังไม่มีคนทำ) แล้วค่อยฝั่ง UI

### Commit 1 — live stats (T207)

`src/stats.rs` อ่าน metric registry วินาทีละครั้งจาก runtime ของ admin
แปลง counter สะสมเป็น rate/percentile เก็บย้อนหลัง 5 นาที แล้ว push ให้คนที่เปิดดูอยู่

- **ไม่มีอะไรเพิ่มใน request path นอกจาก atomic เดียว** — `prx_inflight_requests`
  นับขึ้น/ลงอย่างละครั้งต่อ request ที่เหลือ sampler อ่านจาก registry เดียวกับที่
  `/metrics` เสิร์ฟ ตัวเลขสองที่จึงเพี้ยนกันไม่ได้ (มี e2e เทียบตรงๆ)
- **`GET /web/stats`** = snapshot + ประวัติ (อ่านครั้งแรก),
  **`GET /web/stats/stream`** = SSE วินาทีละครั้ง มี keep-alive และ retry hint
  จำกัด 16 stream พร้อมกัน และ slot คืนตอนปิดแท็บจริง (guard อยู่ใน response body
  ไม่มี task ต่อ client ให้รั่ว) — e2e เปิด 16 + โดนปฏิเสธตัวที่ 17 + ปิดแล้วนับกลับเป็น 0
- **แก้ bucket ของ histogram** — default ของ Prometheus จบที่ 10 แต่ metric นับเป็น
  มิลลิวินาที ทุก request ที่ช้ากว่า 10 ms จึงตกไปอยู่ `+Inf` ทั้งหมด p95/p99 ที่ผ่านมา
  จึงเป็นการเดา ตอนนี้ bucket ไปถึง 10 วินาที และ observe ด้วยความละเอียดต่ำกว่า
  มิลลิวินาที (เดิมปัดทิ้ง request ที่เร็วกว่า 1 ms เป็น 0)
- **event strip** — config apply/reload (สำเร็จและล้มเหลว), circuit เปิด, upstream ล้ม/กลับมา,
  cert ใกล้หมดอายุ โดย event ของ upstream/cert sampler อนุมานจาก metric ที่อ่านอยู่แล้ว
  ไม่ต้องไป instrument ซ้ำใน hot path

### Commit 2 — หน้า Dashboard

`stores/stats.ts` ถือ lifecycle ทั้งหมดไว้ที่เดียว: stream, watchdog, polling fallback
และ abort controller ทุกอย่างถูกปิดโดยฟังก์ชันที่ `startLiveStats()` คืนมา
ออกจากหน้า = ไม่เหลืออะไรทำงานต่อ (และ slot ฝั่ง server ก็คืนทันที)

- **แถวบน 6 ตัว**: RPS, p99, 5xx, 4xx (แยกกันตามที่ task ระบุ), in-flight,
  และ cache hit ratio เมื่อมี route เปิด cache ไว้ (ไม่งั้นแสดง upstream ready)
  ทุกตัวมี sparkline 5 นาทีและ delta เทียบ 1 นาทีก่อน
- **กราฟเขียนเอง** ไม่ดึงไลบรารีเข้ามา (registry ถูก network บล็อกตั้งแต่ T302 และ UI
  ต้องฝังในไบนารี): SVG ล้วน + label เป็น HTML ข้างนอก viewBox จึงไม่ถูกยืดจนอ่านไม่ออก
  กราฟ traffic ซ้อนตาม status class (แกนเดียว ไม่มี dual axis) กราฟ latency ใช้สีเดียว
  ไล่เข้ม + เส้นประต่างกัน p50/p95/p99 จึงแยกออกโดยไม่พึ่งสี
  มี crosshair + tooltip และอ่านด้วยลูกศรซ้าย/ขวาได้
- **Top routes / Worst routes** จาก window 60 วินาทีที่ server คิดให้ คลิกชื่อไปหน้า route นั้น
- **Upstream grid** สี่เหลี่ยมละ upstream จัดกลุ่มตาม service คลิกไปหน้า service
- **แถบสถานะด้านบน** บอกว่าปกติ/degraded/down พร้อมประโยคว่าอะไรพัง และปุ่มไปยังที่ที่ต้องแก้
  ถ้าติดต่อ admin API ไม่ได้ อันนั้นขึ้นก่อนเพราะตัวเลขที่ค้างอยู่ไม่ได้แปลว่าระบบยังดี

### ที่ตัดสินใจต่างจากที่ task เขียนไว้

- **"active connections" → "in flight"** — pingora ไม่ได้ให้ hook ระดับ connection
  ที่นับได้ถูกโดยไม่แตะ hot path การนับ request ที่กำลังเสิร์ฟอยู่เป็นของจริงที่วัดได้
  (connection-level เป็นงานของ T401 ที่จะทำ `prx_connections_total{state}`)
- **ไม่ได้ทำ T401 ทั้งก้อน** — งานนี้แก้เฉพาะสิ่งที่ขวาง dashboard อยู่ (bucket, ความละเอียด
  ของ latency, gauge in-flight) ส่วนการเปลี่ยนชื่อ metric เป็น `prx_request_duration_seconds`,
  `status_class` label, `max_label_values` และ `docs/METRICS.md` ยังเป็นของ T401
- **หน้า Services ยังใช้ poll 3 วินาทีเหมือนเดิม** — ย้ายมาใช้ stream ได้แล้ว แต่เป็นงานต่อ
  ไม่ใช่ของ T306

### ทดสอบ

- `cargo test --test e2e_stats` — ตัวเลขตรงกับ `/metrics`, stream push จริง,
  เพดาน 16 client, slot คืนหลังปิด, config apply ขึ้น event strip
- `npm run dashboard:check` — tile/chart/leaderboard/แถบสถานะ, ตัวเลขขยับเองจาก SSE,
  degraded แล้วลิงก์ไปถูกที่, stream โดนปฏิเสธ → fallback เป็น polling และบอกว่ากำลัง poll,
  admin API ตาย → บอกว่า reconnecting ไม่ใช่กราฟค้างเงียบๆ, และ DOM/กราฟไม่โตตามเวลา
- `npm run smoke` กับไบนารีจริง + ยิงทราฟฟิกจริงเข้า proxy: dashboard แสดง RPS,
  p99 ระดับไมโครวินาที, 4xx, cache hit ratio และ event "Config applied from the admin API"
  ที่เกิดจากการ apply ของ smoke เอง

## Acceptance criteria

- [x] กราฟอัปเดตสดจาก SSE โดยไม่ memory leak เมื่อเปิดทิ้งไว้ 1 ชั่วโมง — history ถูก cap
      ที่ 300 sample (เท่า ring ฝั่ง server) และ event ที่ 50 รายการ `dashboard:check`
      ยิง tick หลายร้อยครั้งแล้ววัดว่า path ของกราฟและจำนวน DOM node ไม่โตขึ้น
      (แทนการนั่งดู devtools หนึ่งชั่วโมง)
- [x] เมื่อ admin API หลุด UI แสดงสถานะ reconnecting ไม่ใช่กราฟค้างเงียบๆ — watchdog
      ตัดเป็น reconnecting เมื่อเงียบเกิน 4 วินาที, stream ที่ถูกปฏิเสธตกไปเป็น polling
      พร้อมบอกว่ากำลัง poll, และแถบด้านบนบอกว่าตัวเลขที่เห็นคือค่าล่าสุดที่มาถึง
- [x] หน้า render ครั้งแรกใน < 1s บนเครื่องธรรมดา — bundle 184 KB gzip (ใต้เพดาน 250 KB),
      snapshot เดียวมาพร้อมประวัติ 5 นาที กราฟจึงมีของตั้งแต่เฟรมแรกโดยไม่ต้องรอ stream
- [x] ทุกตัวเลขมี tooltip อธิบายว่ามาจาก metric ไหน — `MetricTile` มี `hint`
      ที่บอกชื่อ metric ต้นทาง และ `dashboard:check` ตรวจว่ามีครบทุก tile

## Out of scope

- แทนที่ Grafana สำหรับข้อมูลย้อนหลังยาว
