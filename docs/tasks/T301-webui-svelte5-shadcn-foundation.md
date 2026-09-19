# T301 — ฐาน WebUI: Svelte 5 + shadcn-svelte + design token

**Phase:** 3 · Web UI
**Status:** done
**Size:** L (~2d)
**Depends on:** —
**Files:** `webui/package.json`, `webui/svelte.config.js`, `webui/tailwind.config.ts`, `webui/src/app.css`, `webui/components.json`

## เป้าหมาย

วางฐานให้ UI ทั้งหมดใช้ shadcn-svelte ได้จริง ก่อนจะรื้อทีละหน้า

## สถานะปัจจุบัน

`webui/package.json` ใช้ **Svelte 4.2** + `@sveltejs/vite-plugin-svelte@3` + Tailwind 3
component ทุกตัวเขียนเองด้วย Tailwind ดิบ (`webui/src/lib/components/dashboard/*.svelte`)
ยังไม่มี shadcn-svelte / bits-ui / design token เลย และมี dark mode ที่ทำเองอยู่แล้ว (commit `5171bc2`)

## ขอบเขตงาน

1. อัปเกรด Svelte 4 → 5 (runes) และ vite plugin ให้ตรงเวอร์ชัน
   - ไล่แก้ `export let` → `$props()`, store `$:` → `$derived`/`$effect` ตามที่จำเป็น
   - ทำเป็น commit แยกจากการติดตั้ง shadcn เพื่อให้ review ง่ายและ revert ได้
2. ติดตั้ง `shadcn-svelte` (+ `bits-ui`, `tailwind-variants`, `clsx`, `tailwind-merge`, `lucide-svelte`)
   สร้าง `components.json`, alias `$lib`, และ `webui/src/lib/utils.ts` (`cn()`)
3. Design token ใน `app.css` เป็น CSS variable ตามระบบ shadcn (`--background`, `--foreground`, `--primary`, ...)
   ทั้ง light/dark แล้วย้าย dark mode เดิมมาใช้ token ชุดนี้ (class strategy + เก็บ preference ใน localStorage + เคารพ `prefers-color-scheme`)
4. ตั้ง font (Inter หรือ IBM Plex Sans Thai สำหรับภาษาไทย) แบบ self-host — **ห้ามโหลดจาก CDN**
   เพราะ UI ต้องใช้งานได้ใน network ปิด (asset ถูก embed เข้า binary)
5. ตรวจ bundle size: ตั้ง budget (เช่น JS gzip < 250KB) และ fail build ถ้าเกิน

## ผลลัพธ์ที่ส่งมอบ

แยกเป็น 2 commit ตามที่ task กำหนด

**Commit 1 — Svelte 5**

- svelte 4.2 → 5.57, vite-plugin-svelte 3 → 6, vite 5 → 7, svelte-check 3 → 4
  (ไม่เอา plugin 7 เพราะต้องใช้ vite 8 beta)
- `main.ts` เปลี่ยนเป็น `mount()` — **ของเดิม `new App({ target })` build ผ่านแต่ throw ตอน runtime**
  ถ้าไม่จับก็จะได้หน้าขาวทั้งหน้า
- ขยาย self-closing tag ที่ไม่ใช่ void element 25 จุด, tsconfig เป็น ES2022
- เจอบั๊ก a11y จริง 3 ตัวจาก warning แล้วแก้ ไม่ใช่ปิดเสียง:
  `role="dialog"` ไม่มี `tabindex`; svelte-ignore ใช้ชื่อ rule แบบ Svelte 4 (ขีดกลาง)
  จึงเลิกทำงานไปเงียบๆ; modal ปิดด้วย click handler บน `div` ที่ keyboard เข้าไม่ถึง
  (เปลี่ยน backdrop เป็น `button` มี label); ปุ่มปิดแบบไอคอนล้วนไม่มีชื่อให้ screen reader

**Commit 2 — shadcn + theming**

- Tailwind 3 → 4 (`@tailwindcss/vite`), ลบ `tailwind.config.ts` + `postcss.config.js`
  (palette `ink/fog/tide/aqua/mint` ในนั้นไม่มีใครใช้เลยสักที่)
- `components.json`, alias `$lib`, `cn()`, bits-ui, tailwind-variants, clsx, tailwind-merge
- token ชุดเต็มของ shadcn เป็น CSS variable แบบ oklch ทั้ง light/dark + `success`/`warning`
- **แปลง class สีที่ hardcode 1418 จุดใน 17 ไฟล์** ไปเป็น token — dark ยังหน้าตาเหมือนเดิม
  ส่วน light เพิ่งมีเป็นครั้งแรก
- ปุ่มสลับ light → dark → system ใน sidebar, จำใน localStorage, `system` ตาม OS แบบ realtime
- inline script ใน `index.html` ตั้ง class ก่อน paint → ไม่มี flash
- `color-scheme` ตาม theme → form control กับ scrollbar ของ browser ตามไปด้วย
- font vendor เข้า `public/fonts` เอาเฉพาะ subset ที่ใช้ (Inter latin + IBM Plex Sans Thai thai)
  รวม 93 KB เทียบกับ ~240 KB ถ้า import CSS ของ package ตรงๆ
- `scripts/bundle-budget.mjs` fail build ถ้าเกิน JS gzip 250 KB / CSS 60 KB / font 200 KB
  ตอนนี้ 51.7 / 8.9 / 93.3 KB

**ของแถม — `npm run smoke`** (`webui/scripts/smoke.mjs`)

`build` กับ `check` ผ่านได้ทั้งที่โค้ด throw ตอน mount (การอัป Svelte 5 คือเคสนั้นเป๊ะ)
เลยเขียนตัวขับ UI จริงใน Chromium: เดินครบ 4 หน้า, เปิด/ปิด modal ด้วย Escape,
สลับ theme ครบรอบและเช็คว่าจำได้หลัง reload, เช็คว่า font มาจาก binary ไม่ใช่ CDN,
แล้ว fail ถ้ามี console error หรือ request ออกนอก host

## สถานะที่แก้จากแผนเดิม

task เขียนว่า "ย้าย dark mode เดิมมาใช้ token" — แต่ `grep` ทั้ง `src/` ไม่เจอ
`dark`, `theme`, `localStorage` เลย commit `5171bc2` ที่ชื่อ "feat(webui): dark mode"
แค่ทำให้ทุกอย่างเป็นสีเข้มตายตัว ไม่มี light mode ไม่มีปุ่มสลับ
งานนี้จึงเป็นการ**สร้าง** ไม่ใช่ย้าย

## Acceptance criteria

- [x] `npm run build` + `npm run check` ผ่าน ไม่มี error/warning ค้าง — 103 files, 0/0
- [x] ทุกหน้าเดิมยังทำงานเหมือนเดิม — ยืนยันด้วย `npm run smoke` กับ binary จริง
- [x] dark/light toggle ทำงานถูกต้อง ไม่มี flash ตอนโหลด — inline script + เทสต์ใน smoke
- [x] ภาษาไทยแสดงผลสวย ไม่มี fallback font ที่อ่านยาก — IBM Plex Sans Thai self-host
- [x] ไม่มี request ออกอินเทอร์เน็ตจากหน้าเว็บเลย — smoke fail ถ้ามี request นอก host

## Out of scope

- ออกแบบหน้าใหม่ (T303–T308)
