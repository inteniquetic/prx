# T309 — UX polish + i18n (ไทย/อังกฤษ)

**Phase:** 3 · Web UI
**Status:** ✅ done
**Size:** M (~1d)
**Depends on:** T304, T305
**Files:** `webui/src/lib/i18n/*`, `webui/src/lib/configTemplates.ts`,
`webui/src/lib/components/onboarding/*`, `webui/src/lib/components/layout/ErrorScreen.svelte`,
`webui/src/lib/components/ui/skeleton/skeleton-table.svelte`, `webui/src/lib/configNormalize.ts`,
`webui/scripts/{i18n,a11y,ux}-check.mjs`, ทุกหน้า

## เป้าหมาย

ทำให้ "ใช้ง่าย" เป็นจริง ไม่ใช่แค่สวย — คนที่ไม่เคยเขียน config ของ nginx ก็ต้องตั้ง route ได้

## สิ่งที่ทำ

### i18n: ไม่มีไลบรารี แต่มีตัวตรวจ

`webui/src/lib/i18n/index.ts` ทั้งไฟล์ยาวไม่ถึง 150 บรรทัด — store เก็บภาษาที่เลือก,
lookup ที่ fallback เป็นอังกฤษ, และ `{placeholder}` interpolation เท่านั้น
ของที่ทำให้มันเชื่อถือได้ไม่ใช่ไลบรารี แต่เป็น `npm run i18n:check` ซึ่งล้มบิลด์เมื่อ:

- key มีในภาษาหนึ่งแต่ไม่มีในอีกภาษา (ตอนนี้ 675 key เท่ากันทั้ง `en` และ `th`)
- key ซ้ำในดิกชันนารีเดียวกัน
- `{placeholder}` ในสองภาษาไม่ตรงกัน (แปลแล้วลืม `{count}` = ตัวเลขหายไปจากประโยค)
- นับจำนวนแล้วมี `.one` ไม่มี `.other`
- คอมโพเนนต์เรียก key ที่ไม่มีใครนิยาม หรือดิกชันนารีมี key ที่ไม่มีใครใช้
- **มีข้อความอังกฤษเหลืออยู่ในเทมเพลต** — ทั้งข้อความ, `placeholder`/`aria-label`/`title`,
  และข้อความใน `toast.*()` ข้อนี้คือข้อที่ task ขอจริงๆ ("ตรวจว่าไม่มีข้อความ hardcode เหลือ")
  และเป็นข้อที่เขียนยากที่สุด เพราะต้องแยก "คำอังกฤษที่เป็น copy" ออกจาก
  "ค่าตัวอย่างและศัพท์เทคนิคที่ไม่แปล" (`127.0.0.1:3000`, `round_robin`, `TLS`, `HTTP/2`)
  จึงใช้กฎ (ค่าที่หน้าตาเป็น address/identifier/ศัพท์ในไฟล์ config ได้รับการยกเว้น)
  แทนรายการ allowlist ยาวๆ ที่จะกลายเป็นที่ซุกข้อความที่ลืมแปล

`ui/` (พวก primitive ของ bits-ui) และหน้า styleguide ถูกกันออกจากการตรวจ — ไม่ใช่ product surface

การจัดรูปแบบตามภาษาด้วย: `formatDateTime`/`formatDate` ใช้ `Intl` แต่บังคับ `-u-ca-gregory`
เพราะ `th-TH` ดีฟอลต์เป็นพุทธศักราช ซึ่งถูกสำหรับเว็บทั่วไปและผิดเมื่อวางข้าง
วันหมดอายุ cert ที่มาจาก log ของเซิร์ฟเวอร์

เมนูเลือกภาษาอยู่บน topbar และ `setLocale` เขียน `<html lang>` ด้วย —
screen reader เลือกเสียงอ่านจาก attribute นั้น UI ไทยที่อ่านด้วยเสียงอังกฤษคือบั๊กที่ข้อนี้กัน

ฟอนต์: subset ของ Inter (latin) + IBM Plex Sans Thai (thai) รวม 93 KB
คุมด้วย budget เดียวกับ JS/CSS เพราะป้ายที่ผสมไทยกับอังกฤษต้องนั่งบน baseline เดียวกัน

### Onboarding: เทมเพลต + wizard ที่ออกทางเดียวกับทุกอย่าง

`configTemplates.ts` มีสามรูปแบบที่ `docs/CONFIG-PLAYBOOK.md` เปิดหัวไว้ (single backend,
blue-green, path-based API gateway) เขียนด้วย schema ที่ prx อ่านได้จริงวันนี้ พร้อมคอมเมนต์

`SetupWizard.svelte` ถาม 3 ข้อ (ชื่อ service → upstream → host/path) แสดงไฟล์ที่คำตอบรวมกันได้
แล้ว**ใส่ลง draft** — ไม่ใช่ apply ทั้งเทมเพลตและ wizard จบที่ diff + Apply ชุดเดียวกับทุกการแก้ config
ซึ่งเป็นทั้งเรื่องความปลอดภัย (config ที่ไม่มีใครอ่านคือที่มาของ outage แรก)
และเป็นวิธีเรียนรู้ไฟล์ที่เร็วที่สุด — เทมเพลตคือคำอธิบาย

wizard เสนอตัวเองครั้งเดียวเมื่อ config ไม่มี service เลย ปฏิเสธแล้วจำไว้ (localStorage)
เป็นข้อเสนอ ไม่ใช่ประตูที่ต้องผ่าน

### Loading / error

- `SkeletonTable` แทน spinner ในหน้า Routes/Services และ `MetricTile` มี prop `loading`
  ที่เรนเดอร์ skeleton ขนาดเท่าตัวเลขจริง — จุดประสงค์คือไม่ให้หน้าขยับตอนข้อมูลมาถึง
- `<svelte:boundary>` รอบเนื้อหาหน้า + `ErrorScreen.svelte`: บอกว่าเกิดอะไร,
  บอกว่า proxy ยังวิ่งอยู่ไม่ว่าหน้านี้จะพังยังไง, ให้ปุ่มลองใหม่/โหลดหน้าใหม่,
  และเก็บรายละเอียดไว้ให้คนที่จะไปแจ้งบั๊ก แทนหน้าขาวกับ stack trace ใน console

### A11y

axe-core ผ่านทุกหน้า ทั้งสองธีม โดยไม่มี critical/serious violation ของที่ต้องแก้จริงๆ:

- `<html lang>` ไม่เคยถูกตั้ง (critical)
- thumb ของ slider ไม่มีชื่อ — `aria-label` ถูกส่งไปที่ root ไม่ใช่ที่ thumb (serious)
- command palette: input ประกาศตัวเป็น `combobox` แต่ไม่มี `aria-expanded`/`aria-controls`
  และ list ที่ scroll ได้ไม่มี `tabindex` (serious)
- heading ข้ามระดับ (h1 → h3) และมี `<main>` สองอันซ้อนกัน (moderate)

### ผลข้างเคียงที่แก้ไปด้วย: proxy เปล่าไม่เคยดูเปล่า

ตอนเขียนเช็ค ux พบว่า wizard ไม่ยอมเปิดกับ config ที่ไม่มี service เลย ต้นเหตุคือ
`normalizePrxConfig` เติม service ตัวอย่าง (`service-1` → `127.0.0.1:9000`) ให้เมื่อ
`services` ว่าง — ของเก่าจากยุคที่ฟอร์มว่างๆ ใช้งานไม่ได้ ผลคือ prx ที่ยังไม่ได้ตั้งค่า
**แสดง service ที่ไม่มีอยู่ในไฟล์** ในหน้า Services, empty state ไม่เคยโผล่,
และ wizard ไม่มีทางเสนอตัวเอง แก้ให้ "ไม่มี" กับ "ว่าง" ต่างกัน: array ที่ว่างก็ปล่อยว่าง
เติมดีฟอลต์เฉพาะตอนที่คีย์นั้นไม่มีในข้อมูลที่ได้มาจริงๆ

## ทดสอบ

- `npm run i18n:check` — parity ของ key, placeholder, รูปเอกพจน์/พหูพจน์, key ที่ไม่มีคนใช้,
  และข้อความอังกฤษที่ยังค้างอยู่ในเทมเพลต: 675 key ทั้งสองภาษา ไม่มีข้อความตกหล่น
- `npm run a11y:check` — axe บน 6 หน้า × 2 ธีม + หน้า settings ภาษาไทย,
  บวกสองอย่างที่ axe มองไม่เห็นเอง: กด Tab 25 ครั้งใน dialog แล้ว focus ต้องไม่หลุดออกไปข้างหลัง,
  และแถบสถานะที่อัปเดตเองต้องเป็น `aria-live`
- `npm run ux:check` — ขับเบราว์เซอร์จริง: proxy เปล่าเปิด wizard เอง, ตอบ 3 ข้อแล้วได้ diff
  ที่มี config นั้นจริง, Apply แล้วไฟล์ถูกเขียน, สลับเป็นไทยแล้ว `<html lang>` เปลี่ยน
  และ sidebar/หน้า settings ไม่มีอังกฤษเหลือ, config ที่มาช้าแสดง skeleton แล้วหัวหน้าไม่ขยับ
  ตอนข้อมูลมาถึง, และไม่มี error หลุดขึ้น console
- `npm run check` (svelte-check 804 ไฟล์ 0 error + contrast 82 คู่โทเคน),
  `npm run styleguide:check` (1720 text run ผ่าน AA ทั้งสองธีม)
- `shell:check`, `routes:check`, `services:check`, `dashboard:check`, `diff:check`,
  `config:check`, `settings:check` ผ่านทั้งหมด
- Budget: JS 229.2/250 KB, editor chunk 117.8/150 KB, CSS 15.1/60 KB, fonts 93.3/200 KB
  (i18n + ฟอนต์ไทยกินที่ไปพอควร เหลือ headroom บาง — chunk ถัดไปควร lazy)

## Acceptance criteria

- [x] สลับภาษาแล้วไม่มีข้อความตกหล่น (มีเทสต์ตรวจ key ที่ขาด) — `i18n:check` ตรวจทั้ง key ที่ขาด
      และข้อความที่ไม่เคยผ่าน i18n เลย; `ux:check` ยืนยันในเบราว์เซอร์จริงอีกชั้น
- [x] axe-core ไม่มี critical/serious violation — `a11y:check` รายงาน 0 ทั้ง 6 หน้า × 2 ธีม
      รวม dialog และ command palette
- [x] Wizard: จาก config เปล่า ตั้งค่าจนทราฟฟิกวิ่งได้ใน < 2 นาที — `ux:check` เดินครบวง
      (เปล่า → 3 คำถาม → diff → Apply → ไฟล์ถูกเขียน) ใน ~0.8 วินาที
      **ยังไม่ได้ทดสอบกับคนที่ไม่เคยใช้ prx จริงๆ** ซึ่งเป็นสิ่งที่เครื่องทดแทนไม่ได้ —
      เช็คนี้พิสูจน์ได้แค่ว่าทางเดินมีอยู่และไม่พัง
- [x] ไม่มี layout shift ตอนโหลดข้อมูล — skeleton มีขนาดเท่าของจริง และ `ux:check`
      หน่วง `/web/config` ไว้ 1.2 วินาทีแล้ววัดว่าหัวตารางอยู่ที่เดิมก่อนและหลังข้อมูลมาถึง

## Out of scope

- ภาษาที่สามขึ้นไป
- Usability test กับผู้ใช้จริง (ต้องมีคน ไม่ใช่เช็ค)
