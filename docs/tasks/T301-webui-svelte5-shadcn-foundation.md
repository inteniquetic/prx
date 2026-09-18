# T301 — ฐาน WebUI: Svelte 5 + shadcn-svelte + design token

**Phase:** 3 · Web UI
**Status:** todo
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

## Acceptance criteria

- [ ] `npm run build` + `npm run check` ผ่าน ไม่มี error/warning ค้าง
- [ ] ทุกหน้าเดิมยังทำงานเหมือนเดิม (ยังไม่เปลี่ยน UX ในงานนี้)
- [ ] dark/light toggle ทำงานถูกต้อง ไม่มี flash ตอนโหลด (inline script ตั้ง class ก่อน paint)
- [ ] ภาษาไทยแสดงผลสวย ไม่มี fallback font ที่อ่านยาก
- [ ] ไม่มี request ออกอินเทอร์เน็ตจากหน้าเว็บเลย (ตรวจใน devtools)

## Out of scope

- ออกแบบหน้าใหม่ (T303–T308)
