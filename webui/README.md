# PRX WebUI

SPA สำหรับจัดการ config ของ `prx` ด้วย Svelte + TypeScript + TailwindCSS

## Run

```bash
cd webui
npm install
npm run dev
```

## Build

```bash
npm run build
npm run preview
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
