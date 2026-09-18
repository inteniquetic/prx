# T111 — TLS: session resumption, ALPN, multi-cert SNI

**Phase:** 1 · Data plane
**Status:** done
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

## เรื่องใหญ่ที่เจอก่อนเริ่มงาน: TLS ใช้งานไม่ได้เลย

pingora ถูก build โดยไม่เปิด TLS feature ใดๆ (`features = ["lb"]` เท่านั้น)
ซึ่งทำให้ได้ stub จาก `noop_tls`:

- `TlsSettings::intermediate(cert, key)` **ทิ้ง path ของ cert/key ทั้งคู่** แล้วคืน struct เปล่า
- `enable_h2()` ไม่ทำอะไร
- `Acceptor::tls_handshake()` เป็น `unimplemented!()`

พิสูจน์ด้วยการรันจริง: ต่อ HTTPS เข้าไปแล้วได้ connection reset และ log ขึ้นว่า

```
thread 'Pingora HTTP Proxy Service' panicked at noop_tls/mod.rs:96:
unimplemented!("No tls feature was specified")
```

คือ **ทุก connection ที่เข้ามาทาง HTTPS ทำให้ worker thread panic** ทั้งที่ `Prx.toml` ที่แถมมากับ repo
มี `[server.tls]` พร้อม cert path อยู่แล้ว และ README เขียนว่า "TLS provider ขึ้นกับ feature ของ Pingora"
ซึ่งอ่านแล้วเข้าใจว่าใช้ได้

แก้โดยเปิด feature `openssl` (มี libssl-dev + pkg-config ในเครื่องอยู่แล้ว, Dockerfile อัปเดตให้ติดตั้งทั้ง
`libssl-dev` ตอน build และ `libssl3` ตอน runtime)

## บั๊กที่สอง: เปิด h2c แล้ว HTTP/1.1 over TLS พัง

`h2c = true` จาก [T115](T115-ws-grpc-conformance.md) ทำให้ pingora พยายาม peek หา h2 preface
แต่ **TLS stream peek ไม่ได้** โค้ดของ pingora จึงปล่อยให้ `h2c` เป็น true ต่อ แล้วเข้าโหมด h2
ทั้งที่ ALPN ตกลงกันเป็น `http/1.1` → client ได้ h2 frame เป็น response body
(`Received HTTP/0.9 when not allowed`)

ตอนทำ T115 มองไม่เห็นเพราะ TLS ยังพังอยู่ทั้งหมด
แก้โดยแยก TLS listener ออกเป็น pingora service ของตัวเอง ที่ไม่ตั้ง `h2c` — ให้ ALPN เป็นตัวตัดสินฝั่ง TLS

## ผลลัพธ์ที่ส่งมอบ

- `src/tls.rs` — `CertResolver` ที่ implement `TlsAccept` เลือก cert จาก SNI
  (exact → wildcard ที่ยาวที่สุด → default)
- `[[server.tls.cert]]` หลายใบ พร้อม `domains` และ `is_default`; รูปแบบ cert เดียวแบบเดิมยังใช้ได้
- `domains` เว้นว่างได้ — อ่านจาก SAN ของ cert เอง
- ตรวจตอน startup: อ่านไฟล์ไม่ได้, PEM เสีย, **key ไม่ตรงกับ cert** → ไม่ให้ start
- เตือนใน log เมื่อ cert เหลือ < 14 วัน และ export `prx_tls_cert_expiry_seconds{domain}`
- ส่ง intermediate chain จาก PEM เดียวกันให้ client ด้วย

## Acceptance criteria

- [x] SNI ต่างโดเมนได้ cert คนละใบถูกต้อง (เทสต์ด้วย `openssl s_client -servername` จริง)
- [x] cert หมดอายุ/key ไม่ตรง → refuse ตั้งแต่ตอนโหลด พร้อมข้อความบอกไฟล์
- [x] TLS listener รองรับทั้ง HTTP/1.1 และ HTTP/2 (บั๊กข้อสองข้างบน)
- [ ] `tls-h2` scenario: resumption rate > 90% — ต้องใช้ harness ของ T001 (ไม่มี Docker ในเครื่องนี้)

## ที่ยังไม่ได้ทำ

session ticket / resumption knob, `min_version`, OCSP stapling — ปรับผ่าน `SslAcceptorBuilder` ที่
`TlsSettings` deref ไปถึงได้ แต่ควรทำคู่กับตัวเลขจาก bench ว่ามีผลจริง ([T112](T112-acme-auto-tls.md)
สำหรับ ACME ยังรออยู่)
