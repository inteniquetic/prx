# T112 — ACME auto TLS (Let's Encrypt)

**Phase:** 1 · Data plane
**Status:** todo
**Size:** L (~2d)
**Depends on:** T111
**Files:** `src/acme.rs` (ใหม่), `src/config.rs`, WebUI settings page

## เป้าหมาย

ฟีเจอร์ที่ทำให้คนเลือก prx แทน nginx จริงๆ: กรอกโดเมนใน Web UI แล้วได้ HTTPS เอง ต่ออายุเอง

## ขอบเขตงาน

1. Config:

```toml
[server.tls.acme]
enabled = true
email = "ops@example.com"
directory_url = "https://acme-v02.api.letsencrypt.org/directory"
domains = ["example.com", "www.example.com"]
storage_dir = "./acme"
staging = false
renew_before_days = 30
```

2. Challenge: เริ่มที่ `http-01` (ต้อง serve `/.well-known/acme-challenge/` บน listener :80 ก่อน route matching)
   — DNS-01 เป็น follow-up
3. เก็บ account key + cert ลง `storage_dir` สิทธิ์ 0600, โหลด cert ใหม่เข้า runtime แบบ hot-swap ไม่ต้องรีสตาร์ท
4. ตัวจับเวลาเช็คต่ออายุทุก 12 ชม. + retry แบบ exponential backoff และ **ต้องใช้ staging เป็น default ตอน dev**
   เพื่อไม่ให้ชน rate limit ของ Let's Encrypt
5. สถานะ ACME ผ่าน admin API: `GET /web/tls/status` → โดเมน, วันหมดอายุ, ผลครั้งล่าสุด, error ล่าสุด
6. Metrics + log เหตุการณ์ออก/ต่ออายุ cert

## Acceptance criteria

- [ ] ทดสอบครบ flow กับ [pebble](https://github.com/letsencrypt/pebble) ใน integration test (ไม่ยิงของจริงใน CI)
- [ ] cert ใหม่ถูกใช้โดยไม่ drop connection ที่ค้างอยู่
- [ ] ACME ล้มเหลวต้องไม่ทำให้ proxy ตาย — ใช้ cert เดิมต่อและรายงานใน `/web/tls/status`
- [ ] เอกสารบอกข้อกำหนด (ต้องเข้าถึงพอร์ต 80 จากภายนอก, DNS ต้องชี้มาแล้ว)

## Out of scope

- DNS-01, wildcard cert, cert จาก Vault
