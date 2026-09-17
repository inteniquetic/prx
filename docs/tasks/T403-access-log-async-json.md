# T403 — Access log แบบ JSON + non-blocking + sampling

**Phase:** 4 · Observability & Ops
**Status:** todo
**Size:** M (~1d)
**Depends on:** T101
**Files:** `src/logging.rs` (ใหม่), `src/proxy.rs`, `src/config.rs`

## เป้าหมาย

access log ที่ไม่ฆ่า throughput — เป็นจุดที่ proxy หลายตัวเสีย RPS ไปครึ่งหนึ่งโดยไม่รู้ตัว

## สถานะปัจจุบัน

`logging()` (`src/proxy.rs:373`) สร้าง String หลายตัวต่อ request (`ctx.route_name.clone()`, `"unknown".to_string()`)
และเขียนผ่าน `tracing-subscriber` แบบ default ซึ่ง blocking ต่อ stdout

## ขอบเขตงาน

1. Config:

```toml
[observability.access_log]
enabled = true
format = "json"          # json | combined | custom
output = "stdout"        # stdout | file
path = "./logs/access.log"
sample_ratio = 1.0
buffer_lines = 8192
fields = ["ts","method","host","path","status","duration_ms","upstream","route","bytes_sent","request_id","retry_count","cache"]
```

2. Writer แบบ non-blocking: channel ไป writer task + buffer มีขอบเขต
   เมื่อเต็มให้ **drop แล้วนับ** `prx_access_log_dropped_total` (ห้าม block request path เด็ดขาด)
3. Serialize ให้ถูก: escape ครบ, path/header ที่ผู้ใช้ส่งมาต้องไม่ทำให้ JSON เพี้ยนหรือ inject บรรทัดปลอม
4. Sampling + always-log-on-error
5. File output: rotate ตามขนาด + reopen เมื่อได้ SIGHUP (ให้ logrotate ใช้ได้)
6. เอกสาร: ตัวอย่าง query ด้วย `jq`, และตัวเลข throughput ตอนเปิด/ปิด log

## Acceptance criteria

- [ ] bench: เปิด access log แล้ว RPS ตกไม่เกิน 5% (baseline คือปิด)
- [ ] path ที่มี `"`/newline/unicode → JSON ยังถูกต้อง (มีเทสต์ fuzz สั้นๆ)
- [ ] เมื่อ disk ช้า/เต็ม → proxy ยังตอบ request ปกติ log ถูก drop และนับไว้
- [ ] rotate + SIGHUP ทำงานจริง

## Out of scope

- ส่ง log ตรงไป Loki/Elasticsearch
