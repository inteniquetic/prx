# T101 — ตัด allocation ต่อ request ใน request path

**Phase:** 1 · Data plane
**Status:** done
**Size:** M (~1d)
**Depends on:** T003
**Files:** `src/proxy.rs`, `src/runtime.rs`

## เป้าหมาย

ลด heap allocation ต่อ 1 request ให้เหลือน้อยที่สุด (เป้า: 0 allocation ใน happy path ที่ไม่ retry)
เพราะที่ RPS สูง malloc/free คือ CPU และคือ p99

## สถานะปัจจุบัน (โค้ดจริง)

ใน `src/proxy.rs:169` `request_filter()` ทำ allocation อย่างน้อย 5 ครั้งต่อ request:

- `src/proxy.rs:171` `ctx.snapshot = Some(snapshot.clone())` — clone Arc (ถูก แต่ดู T103)
- `src/proxy.rs:179` `normalize_host(...)` → `String` ใหม่ทุก request (`src/runtime.rs:366` ทำ `to_ascii_lowercase()` + `to_string()`)
- `src/proxy.rs:180` `req_header.uri.path().to_string()`
- `src/proxy.rs:204` `ctx.route_name = Some(route.name.clone())` — clone String ทุก request
- `src/proxy.rs:187,192,213` `"health".to_string()` ฯลฯ
- `src/runtime.rs:143` `matches_host()` ทำ `format!(".{suffix}")` ทุกครั้งที่เจอ wildcard route

## ขอบเขตงาน

1. `route_name`: เปลี่ยน `RouteRuntime.name` เป็น `Arc<str>` และ `ctx.route_name: Option<Arc<str>>`
   (clone = bump refcount ไม่ alloc) หรือเก็บแค่ `route_idx` แล้วค่อย resolve ตอน `logging()`
2. `host`: ทำ `normalize_host_into(&str, &mut SmallString)` หรือใช้เทคนิค "borrow ถ้าไม่ต้องแก้"
   — ส่วนใหญ่ header host เป็น lowercase อยู่แล้วและไม่มี port → คืน `Cow<str>` แบบ borrowed
3. `path`: ไม่ต้อง `to_string()` — เก็บ `(start, len)` หรือ resolve route ทันทีในสโคปเดียวกัน
   แล้วเก็บเฉพาะสิ่งที่ `logging()` ต้องใช้
4. `matches_host` wildcard: precompute suffix ที่มี `.` นำหน้าไว้ตอน build `RuntimeConfig` (ดู T102 ถ้าทำคู่กัน)
5. static string ทั้งหมด (`"health"`, `"ready"`, `"no_route"`, `"unknown"`) → `&'static str` / `Arc<str>` ที่ cache ไว้
6. `hash_seed` (`src/proxy.rs:183`) คำนวณทุก request แม้ LB ไม่ใช่ `hash` → ทำ lazy เฉพาะตอน `LbStrategy::Hash`

## Acceptance criteria

- [ ] micro-bench จาก T003 ของ `select_route` + request path ดีขึ้น ≥ 20%
- [ ] เพิ่มเทสต์ที่ยืนยัน behavior เดิม: host มี port, host uppercase, IPv6 `[::1]:8080`, wildcard `*.example.com`
- [ ] flamegraph หลังแก้: `malloc`/`__rust_alloc` หายจาก top-10
- [ ] `make gate` ผ่าน

## วิธีทดสอบ

```bash
cargo bench -- routing
cargo test --all-targets
make bench SCENARIO=h1-keepalive   # เทียบกับ baseline
```

## Out of scope

- เปลี่ยนอัลกอริทึม matching (T102)

## ผลลัพธ์ที่ส่งมอบ

- `RouteRuntime.name` เป็น `Arc<str>` และ `ctx.route_name` เป็น `Option<Arc<str>>` → clone = refcount ไม่ใช่ alloc
- ชื่อคงที่ (`health`, `ready`, `no_route`, `method_not_allowed`, `unknown`) สร้างไว้ล่วงหน้าใน `PrxProxy`
- `normalize_host` คืน `Cow<str>` → ยืมได้เมื่อ host เป็นตัวเล็กและไม่มี port (เคสปกติ)
- `ctx.host` / `ctx.path` ถูกลบทิ้ง — `logging()` และ error path อ่านจาก `session` โดยตรง
- `hash_key` ถูกเรียกเฉพาะ service ที่ใช้ `lb = "hash"` (เดิมคำนวณทุก request)
- `matches_host` ที่มี `format!` ถูกแทนด้วย index ของ T102
- `request_filter` ตอนนี้ไม่มี allocation เหลือ: `.clone()` ทุกตัวที่เหลือเป็น `Arc`

## ผลวัด (median, container 4 vCPU)

| benchmark | ก่อน | หลัง |
|---|---|---|
| `normalize_host/plain` | 43.0 ns | 26.5 ns (1.6×) |
| `normalize_host/with_port` | 68.1 ns | 52.4 ns (1.3×) |
| `normalize_host/ipv6` | 35.8 ns | 19.2 ns (1.9×) |

ผลของ `select_route` อยู่ใน T102 เพราะสองงานนี้แก้โค้ดชุดเดียวกัน

**ยังไม่ได้ทำ:** flamegraph ยืนยันว่า `__rust_alloc` หลุดจาก top-10 (ต้องใช้ perf ซึ่งรันในคอนเทนเนอร์นี้ไม่ได้)
