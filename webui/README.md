# PRX WebUI

SPA สำหรับจัดการ config ของ `prx` ด้วย Svelte 5 + TypeScript + Tailwind 4 + shadcn-svelte

## Run

```bash
cd webui
npm install
npm run dev
```

## Build

```bash
npm run build          # vite build + ตรวจ bundle budget
npm run check          # svelte-check + ตรวจ contrast ของ token
npm run styleguide     # เปิดหน้ารวม component ทั้งหมด (dev เท่านั้น)
npm run shell:check    # ขับ app shell ใน Chromium: deep link / keyboard / 375px / palette
npm run routes:check   # ขับหน้า Routes: search budget / คำเตือน / ฟอร์ม / bulk / tester
npm run styleguide:check  # วัด contrast จากหน้าที่ render จริง
npm run smoke          # ขับ UI ที่ฝังใน binary จริง (ต้องมี prx รันอยู่)
```

## Features

- จัดการ `server`, `observability`, `route`, `upstream` แบบฟอร์ม
- เพิ่ม/ลบ route และ upstream ได้
- Validation พื้นฐานให้สอดคล้องกับกฎหลักของ `prx`
- โหลด config จาก Admin API (`GET /web/config?format=json`)
- เช็ค Route Health จาก Admin API (`GET /web/health/routes`)
- บันทึก config กลับผ่าน Admin API (`PUT /web/config`)
- Export / Import เป็น JSON
- แสดงผล TOML preview พร้อม copy ได้ทันที

---

# App shell

## Routing

ใช้ URL จริง (History API) ไม่ใช่ hash — `/routes/api-v1` refresh แล้วยังอยู่ที่เดิม
และแปะลิงก์ส่งให้คนอื่นได้ ตัว router อยู่ใน `src/lib/stores/navigation.ts`

| path | หน้า |
| ---- | ---- |
| `/` | Dashboard |
| `/routes` · `/routes/:name` | Routes (list / detail) |
| `/services` · `/services/:name` | Services (list / detail) |
| `/tls` | TLS — placeholder จนถึง T308 |
| `/settings` | Settings |
| `/audit` | Audit — placeholder จนถึง T206 |

ฝั่ง server: `handle_webui_get` ใน `src/admin.rs` คืน `index.html` ให้ทุก path
ที่ไม่ได้อยู่ในโฟลเดอร์ asset ที่ฝังมา (`assets/`, `fonts/`) ส่วน asset ที่หาไม่เจอ
ยังคืน 404 ตามเดิม — ตอบ HTML ให้ `.js` ที่หายไปจะกลายเป็น syntax error ที่ debug ยากกว่าเดิม
(เงื่อนไขเดิมคือ "path ไม่มีจุด" ซึ่งทำให้ route ที่ตั้งชื่อตามโดเมน เช่น `api.example.com` โดน 404)

**กติกาของ detail page:** URL เป็นเจ้าของ selection หน้าเพจไม่ set เอง
เวลาเปิด route หน้าเพจ `dispatch('select', name)` → shell เปลี่ยน URL → prop กลับเข้าหน้าเพจ
ทิศทางเดียวแบบนี้ทำให้ deep link ไม่ถูก render แรก (ที่ config ยังไม่มา) เขียนทับ
ส่วนชื่อที่ไม่มีใน config จะเด้งกลับ list พร้อม toast บอกเหตุผล ไม่ปล่อยให้ URL โกหก

## ส่วนประกอบ

- **Sidebar** (`layout/Sidebar.svelte`) — เมนูแบ่งเป็น 3 กลุ่ม, badge จำนวน route ที่ไม่ healthy
  และ upstream ที่ down, ยุบเหลือไอคอนได้ (จำใน localStorage `prx-sidebar-collapsed`,
  ตอนยุบใช้ tooltip เป็น label) และต่ำกว่า `md` จะหายไปกลายเป็น drawer (`Sheet`)
- **Topbar** (`layout/Topbar.svelte`) — breadcrumb, ปุ่มเปิด palette, ป้าย "draft ยังไม่ apply",
  สถานะการต่อ admin API, เมนู theme (radio 3 ตัวเลือก), เมนู account
- **Command palette** (`layout/CommandPalette.svelte`) — `Cmd/Ctrl + K`
  ค้น route จากชื่อ/host/path/service, ค้น service จากชื่อ/address ของ upstream,
  กระโดดไปหน้า และ action ด่วน (เพิ่ม route, เช็ค health, apply/review draft)
  กรองเองไม่ใช้ตัวกรองของ bits-ui เพราะต้อง match host ด้วยและต้องตัดผลลัพธ์ที่แสดง
  ให้เหลือกลุ่มละ 7 แถว — config 500 route จึงเปิดได้ใน ~46ms (budget 100ms)
- **สถานะการเชื่อมต่อ** (`stores/connection.ts`) — ทุก request ผ่าน `$lib/api/admin`
  รายงานผลเข้า store นี้ และมี heartbeat ถาม admin API ทุก 15 วินาที
  (backoff ถึง 60 วินาทีเมื่อล่ม, หยุดเมื่อ tab ไม่ได้อยู่หน้าจอ)
  พลาด 3 ครั้งติดถึงจะเรียกว่า offline — ครั้งเดียวอาจเป็นแค่ deploy

## คีย์บอร์ด

| คีย์ | ทำอะไร |
| ---- | ------ |
| `Tab` แรกสุด | Skip to content — ข้าม sidebar ทั้งแถบในคีย์เดียว |
| `Cmd/Ctrl + K` | เปิด/ปิด command palette |
| `Enter` บนเมนู/แถวตาราง | เปิดหน้า / เปิด route |
| `Esc` | ปิด dialog, drawer, palette |

`npm run shell:check` ยืนยันว่าเดินจากหน้าเปล่าไปถึงพิมพ์แก้ route ได้ด้วยคีย์บอร์ดล้วน

---

# หน้า Routes

## ตาราง

ค้นหาจากชื่อ/host/path/service (haystack ถูก lowercase ไว้ล่วงหน้าตอน analyze ไม่ใช่ทุก keystroke),
filter ตาม service และสถานะ, เรียงได้ทุกคอลัมน์, แบ่งหน้า 25 แถว (เลือก 10/50/100 ได้)

**งบเวลา:** พิมพ์ค้นหาที่ 500 routes ต้องตอบสนองใน < 50 ms
`npm run routes:check` วัดจาก build จริง (ไม่ใช่ dev server ซึ่งช้ากว่าเพราะ dev check ของ Svelte)
ตอนนี้อยู่ที่ ~32 ms

สิ่งที่ทำให้อยู่ในงบ: ต้นทุนเกือบทั้งหมดคือการวาดแถว ไม่ใช่การกรอง
จึงตัด component ที่แพงออกจากแถว — checkbox ใช้ native, ลิงก์ service เป็น `<button>` ธรรมดา,
`StatusDot` ในตารางใช้ `tooltip={false}` (ไม่งั้นได้ floating-layer provider ต่อแถว)
และเมนู action ของแถวจะ mount เฉพาะแถวที่กดเปิด

## ลำดับการ match และคำเตือน

คอลัมน์ Precedence บอกว่า route นั้นชนะด้วยกติกาข้อไหน (exact host > wildcard > any host)
ส่วน `src/lib/routeAnalysis.ts` หา route ที่ **ไม่มีวันถูก match**: host+path_prefix ซ้ำกับ route
ที่อยู่เหนือกว่าและ method ครอบคลุมกัน — ขึ้นไอคอนเตือนในตารางพร้อมบอกว่าใครบัง

ไฟล์นี้เป็นการเขียนกติกาของ `src/router.rs` ซ้ำอีกรอบ ซึ่งเป็นสิ่งที่ต้องระวัง
จึงจำกัดไว้แค่การอ่านตารางแบบ static ส่วนคำตอบที่ผู้ใช้จะเอาไปตัดสินใจจริงมาจาก **route tester**
ที่ถาม matcher ตัวจริงบน server

## Route tester

`POST /web/routes/test` ส่ง method + host + path แล้ว server รัน `select()` ตัวเดียวกับที่ proxy ใช้
บน snapshot เดียวกัน คืนมาว่า match route ไหน ด้วยกติกาข้อไหน ไปที่ service ไหน
และ upstream แต่ละตัวสถานะอะไร — ตัวที่จะถูกเลือกถัดไปมี badge "next"
ถ้ากลยุทธ์เป็นแบบสุ่ม (random / least_conn / p2c_ewma) จะบอกตรงๆ ว่าไม่มีคำตอบล่วงหน้า

การถามคำถามนี้ไม่ขยับ cursor ของ round-robin (ใช้ `peek_upstream` ที่อ่านอย่างเดียว)
— เปิดหน้า UI แล้วทำให้ทราฟฟิกจริงเบี้ยวไม่ได้

## ฟอร์ม

อยู่ใน `Sheet` แบ่ง 4 แท็บ: Matching / Headers / Limits / Cache
(แท็บที่มี error จะมีจุดกำกับ) validate ฝั่ง client ตาม `PrxConfig::validate` ใน `src/config.rs`
แล้วถ้า server ปฏิเสธ ข้อความจะไปแปะที่ field ที่มันพูดถึง (`mapServerError`)
พร้อมแสดงเป็น alert ด้านบนเสมอ เพราะ field นั้นอาจอยู่คนละแท็บหรือถูกซ่อนอยู่

**ยังไม่ได้ generate จาก JSON Schema** — T205 (schema + typed client) ยังไม่เสร็จ
`routeValidation.ts` จึงเขียนกฎซ้ำไว้เอง และผูกไว้กับ `src/config.rs` ด้วยความตั้งใจว่า
เมื่อ T205 มาถึงไฟล์นี้จะเหลือแค่ wrapper บางๆ

## CRUD กับ draft

route ใช้ `/admin/routes*` ซึ่ง **apply ทันที** — ไม่ใช่ draft
ส่วน draft ในแถบบน (T303) มาจากการแก้ config ในหน้า Settings ซึ่งยังต้องกด Save
และตอนนี้ปิดแท็บทั้งที่มี draft ค้างจะโดน browser ถามก่อน

---

# Design language

กติกาที่ทุกหน้าใช้ร่วมกัน เพื่อไม่ให้แต่ละหน้าต่างคนต่างเขียน Tailwind
**ห้าม hardcode สี** — ทุกสีมาจาก token ใน `src/app.css` เสมอ

## Spacing

ใช้ spacing scale ของ Tailwind (`--spacing` = 0.25rem) แต่เลือกใช้แค่ชุดนี้:

| step | ใช้กับ |
| ---- | ------ |
| `1` / `1.5` | ระยะระหว่างไอคอนกับข้อความ |
| `2` | ระหว่าง control ที่อยู่กลุ่มเดียวกัน (ปุ่มติดกัน, label กับ input) |
| `3` | padding ของ cell ในตาราง |
| `4` | padding ภายใน card / dialog / sheet |
| `6` | ระยะระหว่าง section ภายในหน้า |
| `8`–`10` | padding ของ page shell |

ไม่ใช้เลขนอกชุดนี้นอกจากจำเป็นจริงๆ (เช่น ชดเชย optical alignment)

## Radius

ฐานคือ `--radius: 0.625rem` แล้วแตกเป็น 4 ระดับ

| token | ค่า | ใช้กับ |
| ----- | --- | ------ |
| `rounded-sm` | `--radius - 4px` | menu item, badge เล็ก, ช่องใน dropdown |
| `rounded-md` | `--radius - 2px` | button, input, select, tab |
| `rounded-lg` | `--radius` | dialog, popover, alert |
| `rounded-xl` | `--radius + 4px` | card, metric tile, empty state |
| `rounded-full` | — | status pill, avatar, switch |

## Elevation

เงาบอก "ชั้น" ของ UI ไม่ใช่การตกแต่ง — ชั้นสูงกว่าต้องลอยกว่าเสมอ

| ระดับ | class | ใช้กับ |
| ----- | ----- | ------ |
| 0 | ไม่มีเงา | พื้นหน้า, ตาราง |
| 1 | `shadow-xs` | control ที่อยู่นิ่ง (button, input, switch) |
| 2 | `shadow-sm` | card, metric tile |
| 3 | `shadow-md` | popover, dropdown, select content |
| 4 | `shadow-lg` | dialog, sheet, toast |

## สีเชิงความหมาย

| ความหมาย | สี | token (พื้น / ตัวอักษรบนพื้น / ตัวอักษรบนหน้าเว็บ) | ไอคอน |
| -------- | -- | -------------------------------------------------- | ----- |
| healthy | เขียว | `success` / `success-foreground` / `success-emphasis` | `circle-check` |
| degraded | เหลืองอำพัน | `warning` / `warning-foreground` / `warning-emphasis` | `triangle-alert` |
| down | แดง | `destructive` / `destructive-foreground` / `destructive-emphasis` | `circle-x` |
| circuit-open | ส้ม | `circuit` / `circuit-foreground` / `circuit-emphasis` | `zap-off` |
| disabled / unknown | เทา | `muted` / `muted-foreground` | `circle-slash` / `circle-help` |
| info / action | ฟ้า | `primary` / `primary-foreground` | — |

**สีเป็นตัวช่วย ไม่ใช่ตัวหลัก** ทุกสถานะต้องอ่านออกได้โดยไม่ต้องแยกสี:
`StatusDot` ใช้ไอคอนคนละรูปต่อสถานะ และมีคำกำกับ (`showLabel` หรือ `sr-only` เสมอ),
`MetricTile` ใช้ลูกศรขึ้น/ลง/นิ่งพร้อมเครื่องหมาย +/−,
field ที่ผิดมีไอคอนกับข้อความ ไม่ใช่แค่ขอบแดง

ทำไมต้องมี `*-emphasis` แยกจาก `*`:
สีพื้น (เช่น `--success`) ทำมาให้ตัวอักษรสีขาว/ดำทับแล้วอ่านออก
พอเอาสีนั้นไปเป็น **ตัวอักษร** บนพื้นขาวกลับ contrast ไม่พอ
`*-emphasis` คือเฉดเดียวกันที่ปรับให้ผ่าน 4.5:1 บนพื้นหน้าและบน card ทั้งสอง theme

## รูปแบบตัวเลข

ทุกตัวเลขผ่าน `src/lib/format.ts` ไม่เรียก `toFixed()` เองในหน้า

| ชนิด | ฟังก์ชัน | ผลลัพธ์ |
| ---- | -------- | ------- |
| latency | `formatLatency` | `0.42 ms` · `7.3 ms` · `1,240 ms` — เป็น ms เสมอ ทศนิยมลดลงเมื่อค่าโตขึ้น |
| throughput | `formatThroughput` | `8.4 req/s` · `2.4k req/s` · `1.2M req/s` |
| ขนาดข้อมูล | `formatBytes` | IEC เท่านั้น: `900 B` · `1.5 KiB` · `5.4 GiB` (1 KiB = 1024 B) |
| จำนวนนับ | `formatCount` | `12,480` |
| สัดส่วน | `formatPercent` | `99.76%` |
| ส่วนต่าง | `formatDelta` | `+180` · `-3.4` |

ตัวเลขที่เรียงกันเป็นคอลัมน์ให้ใส่ `tabular-nums` เสมอ

## Component

`src/lib/components/ui/` — component จาก shadcn-svelte (bits-ui + tailwind-variants)
เขียนไว้ในโปรเจกต์ ไม่ใช่ dependency: `button`, `card`, `input`, `textarea`, `label`,
`select`, `switch`, `checkbox`, `badge`, `table`, `dialog`, `sheet`, `dropdown-menu`,
`tabs`, `tooltip`, `separator`, `skeleton`, `alert`, `sonner`, `command`, `popover`, `form`

ในโฟลเดอร์เดียวกันมี component เฉพาะของ prx ที่ใช้ซ้ำทุกหน้า
(ไม่ได้มาจาก registry ของ shadcn จึงไม่ถูก `shadcn-svelte add` เขียนทับ):

| component | ใช้ตอน |
| --------- | ------ |
| `status-dot` | สถานะ upstream พร้อม tooltip บอกเหตุผล |
| `metric-tile` | ตัวเลขใหญ่ + delta + sparkline |
| `copy-button` | copy ค่า พร้อม fallback ตอนไม่ได้อยู่บน secure origin |
| `empty-state` | ตอนยังไม่มีข้อมูล พร้อมทางออก |
| `confirm-dialog` | ยืนยันก่อนทำสิ่งที่ย้อนไม่ได้ |
| `key-value-list` | รายละเอียดแบบ key/value |

import แบบ namespace สำหรับตัวที่มีหลายชิ้น และแบบชื่อตรงสำหรับตัวเดียว:

```svelte
import * as Card from '$lib/components/ui/card';
import { Button } from '$lib/components/ui/button';
```

`form` ของที่นี่ไม่ได้ใช้ `formsnap`/`sveltekit-superforms` เพราะทั้งคู่ผูกกับ SvelteKit
แต่ UI นี้เป็น SPA ธรรมดา — `Form.Field` จึงรับ `errors` เป็น prop
แล้วเดินสาย `id` / `aria-describedby` / `aria-invalid` ให้ control เอง

## Styleguide

`npm run styleguide` เปิด `/?styleguide` ซึ่งรวม component ทุกตัว, token, spacing,
radius, elevation และรูปแบบตัวเลขไว้หน้าเดียว ทั้ง light และ dark

หน้านี้อยู่หลัง `import.meta.env.DEV` ใน `src/main.ts` — production build
แทนค่าเป็น `false` แล้ว Rollup ตัดทั้ง branch รวมถึง dynamic import ทิ้ง
ดังนั้น styleguide **ไม่เคยเข้าไปอยู่ใน binary** (ตรวจได้: `grep -r styleguide dist/` ต้องไม่เจอ)

## ความเข้าถึง (accessibility)

เป้าคือ WCAG 2.2 AA และมีสองด่านที่ทำให้ตกไม่ได้:

- `npm run contrast` — อ่าน token จาก `app.css` ตรงๆ แล้วคำนวณ contrast
  ของคู่ที่ component ใช้จริง ทั้ง light/dark (ข้อความ 4.5:1, ส่วนที่บอกว่า
  control อยู่ตรงไหน/สถานะอะไร 3:1) รันอยู่ใน `npm run check` ด้วย
- `npm run styleguide:check` — เปิด styleguide ใน Chromium จริง เดินทุกก้อนข้อความ
  (เปิด popover / menu / tooltip / dialog / sheet ทีละอัน) แล้ววัด contrast จาก
  `getComputedStyle` ซึ่งเป็นค่าที่คนหน้าจอเห็นจริง จับเคสที่ token ถูกแต่
  component หยิบคู่ผิด หรือมี layer โปร่งแสงซ้อนกัน

เส้นขอบที่เป็นการตกแต่งล้วน (ขอบ card, เส้นคั่นแถว) ไม่ถูกบังคับ 3:1
เพราะไม่ได้แบกความหมาย — แต่ขอบ input, focus ring และ marker สถานะถูกบังคับ
