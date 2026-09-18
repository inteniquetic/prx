# T112 — ACME auto TLS (Let's Encrypt)

**Phase:** 1 · Data plane
**Status:** done
**Size:** L (~2d)
**Depends on:** T111
**Files:** `src/acme.rs`, `src/config.rs`, `src/proxy.rs`, `src/tls.rs`, `src/admin.rs`, `src/main.rs`, `tests/e2e_acme.rs`

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

## ผลลัพธ์ที่ส่งมอบ

- `src/acme.rs` — order/renew loop อยู่บน thread + runtime ของตัวเอง ไม่แย่งงานกับ request path
  ใช้ `instant-acme`; เช็คทุก 12 ชม., retry backoff 60s → 3600s
- `ChallengeStore` ตอบ `/.well-known/acme-challenge/<token>` บน plaintext listener
  **ก่อน** health/ready/route matching
- **prefix เป็นของ prx เมื่อเปิด ACME เท่านั้น** — ตอนแรกเช็คว่า "มี challenge ค้างอยู่ไหม" ซึ่งผิด:
  ระหว่าง order สอง order store ว่าง แล้ว token ที่ไม่รู้จักจะหลุดไปหา upstream
  ตอนนี้ใช้ flag `is_active()` → ไม่รู้จัก = `404` เสมอ, ปิด ACME = route ตามปกติ
  (e2e จับบั๊กนี้ได้)
- **Hot swap จริง** — `CertResolver` ย้ายไปอยู่ใน `ArcSwap` (`ResolverState`), `install()` สลับ cert
  เข้า listener ที่รันอยู่โดยไม่รีสตาร์ทและไม่แตะ connection เดิม
- เขียน `account.json` / `key.pem` สิทธิ์ `0600` (ไดเรกทอรี `0700`), โหลดกลับตอน start
  → รีสตาร์ทแล้วเสิร์ฟได้ทันที ไม่ต้องสั่ง order ใหม่
- `GET /web/tls/status` → directory, domains, `staging`, last attempt/success, `last_error`,
  วันหมดอายุของทุก cert ที่ listener เสิร์ฟอยู่
- `directory_url` **default เป็น staging** ของ Let's Encrypt — config ผิดจะไม่เผา rate limit ของ production
- `ca_root_path` สำหรับ ACME CA ส่วนตัว (step-ca, Pebble) ที่ใช้ cert ของตัวเอง
- validate ตอนโหลด config: ต้องมีอย่างน้อย 1 โดเมน, `renew_before_days` 1–89,
  และ **ปฏิเสธ wildcard** เพราะ http-01 ทำไม่ได้ (ต้อง dns-01)

## Acceptance criteria

- [x] ทดสอบครบ flow กับ [pebble](https://github.com/letsencrypt/pebble) ใน integration test (ไม่ยิงของจริงใน CI)
      — `tests/e2e_acme.rs` ยก pebble + pebble-challtestsrv บน random port, ออก cert จริง แล้วเช็คด้วย
      `openssl s_client` ว่า listener เสิร์ฟ cert ที่ issuer เป็น Pebble Intermediate CA จริง
      (ไม่มี binary = skip, CI ไม่พัง)
- [x] cert ใหม่ถูกใช้โดยไม่ drop connection ที่ค้างอยู่ — prx ไม่ถูกรีสตาร์ทเลยหลัง startup
      ในเทสต์ แล้ว handshake ถัดไปได้ cert ใหม่
- [x] ACME ล้มเหลวต้องไม่ทำให้ proxy ตาย — ใช้ cert เดิมต่อและรายงานใน `/web/tls/status`
      (เทสต์ `acme_failure_keeps_the_proxy_serving` ชี้ directory ไปพอร์ตที่ไม่มีอะไรฟัง)
- [x] เอกสารบอกข้อกำหนด (ต้องเข้าถึงพอร์ต 80 จากภายนอก, DNS ต้องชี้มาแล้ว) — `docs/CONFIG-WIKI.md` §3.3a1

## Out of scope

- DNS-01, wildcard cert, cert จาก Vault

## ที่ยังไม่ได้ทำ

- หน้า WebUI สำหรับกรอกโดเมน — ตอนนี้แก้ผ่าน config/admin API; `/web/tls/status` มีข้อมูลให้หน้า UI แล้ว
- metric ของเหตุการณ์ออก/ต่ออายุ cert (ข้อ 6 ของขอบเขต) — ตอนนี้มีแค่ log + `/web/tls/status`
  กับ `prx_tls_cert_expiry_seconds{domain}` ที่มาจาก T111
