# T509 — WAF ใน Web UI + audit + ปรับ false positive

**Phase:** 5 · Plugin & WAF
**Status:** todo (รอผล T504)
**Size:** M (~1d)
**Depends on:** T507
**Files:** `src/admin.rs`, `webui/src/lib/components/waf/*`, `webui/src/lib/i18n/*`

## เป้าหมาย

ปรับจูน WAF ได้โดยไม่ต้อง ssh — เพราะ WAF ที่ปรับจูนไม่ได้คือ WAF ที่จะถูกปิดในสัปดาห์แรก

## สถานะปัจจุบัน

Web UI มีครบแล้วตั้งแต่ T301–T309: draft เดียว, diff ก่อน apply, i18n th/en, a11y floor
WAF ต้องเข้าไปอยู่ในกรอบเดิมทั้งหมด ไม่ใช่สร้างทางลัดของตัวเอง

## ขอบเขตงาน

1. **หน้า WAF** — สถานะ (โหมด, paranoia level, กฎที่โหลดได้/พัง), เปิดปิดต่อ route,
   และ**กฎที่คอมไพล์ไม่ได้ต้องเห็นได้** ตามกติกาข้อ 5 ของแผน
2. **Audit: อะไรโดนบล็อกไปบ้าง** — rule id, ตัวแปรที่โดน, ค่าที่ทำให้โดน (ปิดบังข้อมูลอ่อนไหว),
   route, เวลา, และ anomaly score ที่สะสมได้
3. **Workflow ปรับ false positive** — จากรายการที่โดนบล็อก กดสร้าง exclusion ได้เลย
   แล้ว exclusion นั้น**ลง draft** ไปออกทาง diff + Apply เหมือนการแก้ config อื่นทุกอย่าง
   นี่คือฟีเจอร์ที่ทำให้ WAF ใช้ได้จริง และเป็นเหตุผลที่สถาปัตยกรรม draft ของ T307 คุ้มค่า
4. **Dashboard tile** — จำนวนที่บล็อก/ตรวจพบในหน้า Dashboard ที่มีอยู่ ต่อเข้า SSE ของ T207
5. **เตือนเรื่องโหมด** — ถ้าอยู่ `detection` ต้องบอกชัดว่า "กำลังดูเฉยๆ ไม่ได้บล็อก"
   UI ที่ปล่อยให้คนเข้าใจผิดว่าตัวเองได้รับการป้องกันอยู่ คือ UI ที่อันตรายกว่าไม่มี WAF
6. i18n ครบทั้ง th/en (`i18n:check` บังคับอยู่แล้ว), a11y ผ่าน `a11y:check`, และมี `waf:check`
   แบบเดียวกับหน้าอื่น

## Acceptance criteria

- [ ] `npm run i18n:check`, `a11y:check` ผ่าน และมี `waf:check` ที่ขับหน้าจริง
- [ ] สร้าง exclusion จากรายการ block แล้วลง draft → diff → apply ได้ครบวง
- [ ] กฎที่โหลดไม่สำเร็จแสดงพร้อมไฟล์/บรรทัด/เหตุผล
- [ ] โหมด detection มีป้ายที่อ่านผิดไม่ได้
- [ ] audit log ไม่แสดงข้อมูลอ่อนไหว (เทสต์ด้วย request ที่มี password/token)
- [ ] rebuild `webui/dist` และ commit ตาม DoD ของ repo

## Out of scope

- เก็บ audit log ระยะยาว / ส่งออกไป SIEM — ต่อยอดจาก T206
