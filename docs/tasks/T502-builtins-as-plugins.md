# T502 — ย้ายของที่ฝังตายอยู่มาเป็น plugin

**Phase:** 5 · Plugin & WAF
**Status:** ✅ done
**Size:** M (~1d)
**Depends on:** T501
**Files:** `src/plugin/builtin/{headers,rate_limit,concurrency,cache}.rs`, `src/plugin/mod.rs`,
`src/proxy.rs`, `src/runtime.rs`

## เป้าหมาย

พิสูจน์ว่า plugin API รองรับของจริงได้ทุกรูปแบบ **ก่อน** ที่จะมีคนนอกมาเขียนพึ่งมัน

## สถานะปัจจุบัน

header rules, rate limit, concurrency limit, micro-cache, compression ทั้งห้าตัวคือ plugin
ที่เขียนด้วยมือไว้ใน `request_filter`/`response_filter` แต่ละตัวใช้ความสามารถคนละแบบ:

| ของเดิม | ใช้ความสามารถอะไรของ plugin API |
|---|---|
| header rules | แก้ header ทั้งขาไปและขากลับ |
| rate limit | ตอบกลับเองพร้อม `Retry-After` |
| concurrency limit | **ถือ resource ที่ต้องคืนตอนจบ** ไม่ว่า request จะจบยังไง |
| micro-cache | ตอบกลับเองจาก cache + สะสม response body ขากลับ + เขียนลง store ตอนจบ |
| compression | ลงทะเบียน pingora module ตอน init ไม่ใช่ต่อ request |

ถ้า API ไม่รองรับครบห้าแบบนี้ แปลว่า API ผิด และดีกว่ารู้ตอนนี้

## ขอบเขตงาน

1. ย้ายทีละตัว ตัวละ commit โดยแต่ละ commit ต้อง `make gate` ผ่านและ bench ไม่ถอย
2. ลำดับเริ่มต้น (เมื่อไม่ได้ระบุ `plugins` ใน route) ต้องให้ผลเหมือนเดิมเป๊ะ:
   `headers → rate_limit → concurrency → cache` — **พฤติกรรมของ config ที่มีอยู่ห้ามเปลี่ยน**
3. config เดิม (`[route.rate_limit]` ฯลฯ) ต้องใช้ได้ต่อโดยไม่ต้องแก้ไฟล์ — แปลงเป็น plugin
   ภายในตอน `from_config` การบังคับให้ทุกคนเขียน config ใหม่เพื่อ refactor ภายในคือการผลักต้นทุนให้ผู้ใช้
4. compression ที่ทำงานคนละจังหวะ (pingora module) — ถ้า API รองรับไม่ได้ ให้บันทึกไว้ว่า
   ทำไมมันไม่ควรเป็น plugin แทนที่จะดัด API ให้รองรับ

## สิ่งที่ทำ

ย้ายสี่ตัว: header rules (แยกเป็นสองตัว), rate limit, concurrency, micro-cache
ตัวที่ห้า — compression — **ไม่ย้าย** โดยตั้งใจ ดูเหตุผลด้านล่าง

### การย้ายเปิดช่องโหว่ของ API สองจุดทันที

นี่คือเหตุผลที่ task นี้มีอยู่ และมันทำงานตามที่ตั้งใจ:

1. **`PluginDecision::Respond` ไม่มีที่ให้ใส่ header** — rate limit ต้องบอก `Retry-After`
   และ cache hit ต้องคืน header ชุดที่เก็บไว้ ทั้งคู่ต้องการสิ่งเดียวกันโดยไม่ได้นัดกัน
   จึงเปลี่ยนเป็น `PluginResponse { status, headers, body }`
2. **plugin มองไม่เห็นอะไรนอกจาก header** — rate limit คีย์จาก client address,
   header rules เรนเดอร์มันเป็น `$client_ip` การส่ง `&mut Session` เข้าไปจะทำให้สัญญาของ
   `Respond` ไร้ความหมาย จึงใช้วิธี**ประกาศ**แทน: `needs()` คืน mask ที่ chain รวมกัน
   แล้ว proxy resolve เฉพาะที่ขอ — ข้อตกลงเดียวกับที่ header rules ทำไว้ตั้งแต่ T101
   และแยกเป็นสองบิต เพราะ rate limit ต้องการ**ที่อยู่** (ไม่ต้อง allocate)
   ส่วน header rules ต้องการ**สตริง** (ต้อง allocate)

### header rules ต้องแยกเป็นสองตัว

chain เป็น list เรียงลำดับอันเดียว แต่ "ทำงานก่อนสุดขาไป และหลังสุดขากลับ" คือสองตำแหน่ง
จึงแยกเป็น `request-headers` กับ `response-headers` — ซึ่งเป็นข้อค้นพบ ไม่ใช่การตกแต่ง:
API ที่มี plugin = หนึ่งตำแหน่ง รองรับของที่ต้องการสองตำแหน่งไม่ได้

### ลำดับใน chain

```
request-headers · rate-limit · concurrency · <plugin ของ route> · cache · response-headers
```

ทุกตำแหน่งมีเหตุผลและถูกเลือกให้ผลเหมือนโค้ดเดิมเป๊ะ:

- **limit มาก่อน** เพื่อให้ flood ถูกปัดด้วยเช็คที่ถูกที่สุด
- **plugin ของ route ก่อน cache** เพื่อให้ request ที่ plugin จะปฏิเสธไม่ได้รับ 200 ที่เก็บไว้
- **response-headers หลัง cache** เพราะ cache เก็บสิ่งที่ upstream ส่งมา
  ถ้าเอา header ของ route ใส่ก่อน มันจะถูกอบติดไปกับทุก cache hit หลังจากนั้น
  (โค้ดเดิมทำแบบนี้ และการ "แก้" มันคือการเปลี่ยนพฤติกรรม ซึ่ง task นี้ห้าม)

### built-in ไม่ได้มาจาก `[[plugin]]`

สร้างจาก config ของ route เอง ไม่ใช่จากบล็อก `[[plugin]]` — ไฟล์ config ที่เขียนไว้ก่อนมี
plugin จึงใช้ได้ต่อโดยไม่ต้องแก้แม้แต่บรรทัดเดียว และไม่มี `kind = "rate-limit"`
ให้ประกาศเอง เพราะ limiter ที่แชร์ข้าม route คนละความหมายกับ limiter ต่อ route

**ข้อแลกเปลี่ยนที่ต้องพูดตรง ๆ:** ตำแหน่งของ built-in ใน chain ยังเป็นค่าตายตัว
"ลำดับอ่านออกจาก config" จึงจริงแค่ครึ่งเดียว — จริงสำหรับ plugin ที่ route ลิสต์เอง
แต่ built-in อยู่ตรงไหนยังต้องมาอ่านโค้ด

### ทำไม compression ไม่ย้าย

มันไม่ใช่ขั้นตอนหนึ่งของการจัดการ request แต่เป็น pingora module ที่ติดตั้งบน connection
ตั้งแต่ก่อนมี request เข้ามา และทำงานด้วยการห่อ body stream ไม่ใช่ด้วยการถูกเรียกที่ phase
การดัด API ให้รองรับมันแปลว่าต้องคิด hook ที่มีผู้ใช้เพียงหนึ่งเดียว —
และมันตั้งค่าที่ `[compression]` ระดับ global ไม่ใช่ต่อ route จึงไม่ควรอยู่ใน chain ต่อ route
ตั้งแต่ต้น บันทึกเหตุผลไว้ที่ `init_downstream_modules` ใน `src/proxy.rs`

## Acceptance criteria

- [x] ห้าตัวเดิมวิ่งผ่าน plugin API หรือมีบันทึกเหตุผล — ย้าย 4, compression มีเหตุผลบันทึกไว้
- [x] config เดิมทำงานเหมือนเดิมโดยไม่ต้องแก้ — built-in สร้างจาก config ของ route เอง
      ไม่มี schema ใหม่ที่บังคับ
- [~] `scripts/bench.sh` before/after — **วัดไม่ได้บนเครื่องนี้** ดูหัวข้อถัดไป
- [x] `request_filter` สั้นลง — ส่วนที่จัดการ route เหลือเรียก chain ครั้งเดียว
      แทน rate limit + concurrency + cache lookup ที่เขียนเรียงกัน
- [x] test เดิมยังผ่านโดยไม่ต้องแก้ assertion — **ไม่มีไฟล์ใน `tests/` ถูกแก้แม้แต่ไฟล์เดียว**
      221 เทสต์ผ่านทั้งก่อนและหลัง รวม `e2e_cache` (8), `e2e_rate_limit` (4),
      `e2e_headers` (7), `e2e_sticky` (3), `e2e_compression` (5)

## เรื่อง perf: วัดแล้ว แต่ยังสรุปไม่ได้ และบอกตรง ๆ ว่าสรุปไม่ได้

เครื่องนี้ไม่มี `oha`/`hey`/`wrk` จึงเขียน load generator เองแล้ววัดด้วย release build
ทั้งสองฝั่ง บน config ที่**เปิด built-in ที่ย้ายครบทุกตัวพร้อมกัน**
(header rules ทั้งสองทาง + rate limit + concurrency + cache)

รอบแรกแบบไม่ pin: แกว่ง **±60% ระหว่างการรันซ้ำของไบนารีตัวเดียวกัน**
(before 20,243 → 33,278 rps) ซึ่งกลบความต่างที่อยากวัดจนหมด
สาเหตุวัดได้: ยิงตรงไปที่ upstream โดยไม่ผ่าน prx เลยได้เพดานแค่ ~39,000 rps
ขณะที่ผลที่วัดได้อยู่ที่ 20,000–33,000 — client กับ upstream (Python ทั้งคู่) และ prx
แย่ง CPU กันบนเครื่อง 4 core เดียวกัน สิ่งที่วัดได้จึงเป็นการแย่ง CPU ไม่ใช่ prx

รอบสอง pin ด้วย `taskset` (upstream=core 0, prx=core 1-2, client=core 3)
สลับ A/B สามรอบ ทิ้ง warm-up:

| | rps | p50 (ms) | p99 (ms) |
|---|---|---|---|
| before | 26,985 · 29,618 · 28,848 | 0.998 · 0.958 · 0.900 | 2.226 · 2.306 · 2.202 |
| after | 32,294 · 28,161 · 21,161 | 0.859 · 0.996 · 1.493 | 1.975 · 2.677 · 1.909 |

median: before **28,848** / after **28,161** — ต่างกัน 2.4% ขณะที่ before เองแกว่ง 10%
และ after แกว่ง 53% ระหว่างการรันของไบนารีตัวเดียวกัน

**สรุปที่พูดได้:** ไม่พบการถอยที่วัดได้ แต่ชุดข้อมูลนี้ก็ตัดความเป็นไปได้ที่จะมีการถอย
เล็ก ๆ ไม่ได้เช่นกัน **ข้อนี้จึงยังเปิดอยู่** ต้องวัดบนเครื่องที่มี load generator จริง
และ core พอ — เป็นเรื่องเดียวกับที่ T002 ค้างอยู่
การอ้างว่า "ไม่ถอย" จากตัวเลขชุดนี้แบบหน้าตาเฉยคือการเอาสัญญาณรบกวนมาเล่าเป็นผลการวัด

สิ่งที่ยืนยันได้จริงคือ **route ที่ไม่มีอะไรเปิดเลยยังจ่าย 1 bit test เท่าเดิม**
(ตาม T501) เพราะ chain ของมันยังว่างเปล่า — ไม่มี built-in ตัวไหนถูกใส่เข้าไปถ้า config
ไม่ได้เปิดมัน

## วิธีทดสอบ

- `cargo test` ชุดเดิมทั้งหมด 221 เทสต์ โดยไม่แก้ assertion ข้อไหนเลย
- `npm run smoke` กับไบนารีจริง

## Out of scope

- เพิ่มความสามารถใหม่ให้ของเดิม — งานนี้คือย้าย ไม่ใช่ปรับปรุง
- ทำให้ตำแหน่งของ built-in ใน chain ตั้งค่าได้ — รอจนกว่าจะมีคนต้องการจริง
