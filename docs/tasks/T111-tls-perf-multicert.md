# T111 — TLS: session resumption, ALPN, multi-cert SNI

**Phase:** 1 · Data plane
**Status:** todo
**Size:** L (~2d)
**Depends on:** T104
**Files:** `src/config.rs`, `src/main.rs`, `src/tls.rs` (ใหม่), `docs/CONFIG-WIKI.md`

## เป้าหมาย

TLS handshake คือค่าใช้จ่ายที่แพงที่สุดต่อ connection ใหม่ — ถ้า resumption ไม่ทำงาน ตัวเลข benchmark ตอน `h1-close`
จะแพ้ nginx ทันที และหลายโดเมนต้องใช้ cert คนละใบ

## สถานะปัจจุบัน

`TlsConfig` (`src/config.rs:170`) มีแค่ `listen`, `cert_path`, `key_path`, `enable_h2` — cert ใบเดียว
ไม่มี knob เรื่อง protocol version, cipher, resumption, OCSP และ README ระบุเองว่า "TLS provider ขึ้นกับ feature ของ Pingora"

## ขอบเขตงาน

1. ขยายเป็นหลาย cert:

```toml
[[server.tls.cert]]
domains = ["example.com", "*.example.com"]
cert_path = "./certs/example.crt"
key_path = "./certs/example.key"
```

พร้อม `default_cert` สำหรับ SNI ที่ไม่ match
2. Knobs: `min_version` (default 1.2), `alpn = ["h2", "http/1.1"]`, `session_tickets` (default on),
   `ticket_key_rotation_hours`, `session_cache_size`, `ocsp_stapling`
3. ตรวจสอบว่า TLS backend ที่ build อยู่ (boringssl/openssl/rustls ตาม feature ของ Pingora) รองรับอะไร
   แล้ว log ชัดเจนตอน start ว่าเปิดอะไรอยู่จริง
4. Validate ตอนโหลด config: cert/key อ่านได้, key ตรงกับ cert, ยังไม่หมดอายุ (warn ถ้าเหลือ < 14 วัน)
5. Metrics: `prx_tls_handshake_total{version,resumed}`, `prx_tls_handshake_duration_seconds`,
   `prx_tls_cert_expiry_seconds{domain}` (ใช้ทำ alert ได้)

## Acceptance criteria

- [ ] scenario `tls-h2` : resumption rate > 90% เมื่อ client รองรับ และเห็นใน metric
- [ ] SNI ต่างโดเมนได้ cert คนละใบถูกต้อง (เทสต์ด้วย `openssl s_client -servername`)
- [ ] cert หมดอายุ/key ไม่ตรง → refuse config ตั้งแต่ validate พร้อมข้อความที่บอกไฟล์ไหน
- [ ] bench `h1-close` + TLS เทียบ nginx บันทึกใน `docs/BENCHMARKS.md`

## Out of scope

- ACME (T112), mTLS ฝั่ง client (follow-up)
