# prx benchmarks

เอกสารนี้คือแหล่งอ้างอิงเดียวของคำเคลมเรื่องประสิทธิภาพทั้งหมดของ prx
ถ้าตัวเลขไหนไม่มีในนี้ = ยังไม่ได้วัด ห้ามเอาไปเคลม

สถานะ: [T002](tasks/T002-baseline-report.md) — **micro-benchmark วัดแล้ว / proxy-vs-proxy ยังไม่ได้วัด**
อัปเดตล่าสุด: หลัง [T101](tasks/T101-zero-alloc-request-path.md) + [T102](tasks/T102-route-matcher-index.md)

## สรุปสั้น

| สิ่งที่อยากรู้ | สถานะ |
|---|---|
| prx เร็วกว่า nginx/haproxy ไหม | **ยังไม่รู้** — ต้องรัน `scripts/bench.sh --all` บนเครื่องที่มี Docker |
| prx + WAF เร็วกว่า nginx + ModSecurity / Coraza ไหม | **ยังไม่รู้** — harness และคู่แข่งพร้อมแล้ว ([§7](#7-waf-prx--waf-vs-nginx--modsecurity-vs-caddy--coraza-ยังไม่ได้วัด)) รอ WAF ของ prx (T505–T507) และเครื่องอ้างอิง |
| route matching แพงแค่ไหน | วัดแล้ว: 55–84 ns ต่อ request ไม่ว่าจะมี 10 หรือ 1000 routes |
| ปัญหา wildcard ที่เจอตอนแรก | แก้แล้ว — 59.3 µs → 81.9 ns (724×) |
| reload config แพงแค่ไหน | วัดแล้ว: ~10.8 ms ที่ 1000 routes (ส่วนใหญ่คือ TOML parse) |

## 1. Micro-benchmarks (วัดแล้ว)

**สภาพแวดล้อมที่วัด:** container 4 vCPU, Linux 6.18, rustc 1.94.1, release build,
criterion `--warm-up-time 1 --measurement-time 3 --sample-size 30`

> ⚠️ เครื่องที่วัดเป็น container ที่แชร์ทรัพยากร ตัวเลขสัมบูรณ์จึงใช้เทียบข้ามเครื่องไม่ได้
> แต่ **สัดส่วนระหว่างเคส** (ซึ่งเป็นประเด็นของหน้านี้) เชื่อถือได้
> ค่า baseline ดิบอยู่ที่ `bench/results/baseline/micro-bench.json`

### 1.1 `select_route` — ต้นทุนการหา route ต่อ 1 request

ก่อน = linear scan เดิม, หลัง = route index ของ T102 (median, ns)

| เคส | 10 routes | 100 routes | 1000 routes |
|---|---|---|---|
| route แรกในลิสต์ | 52.0 → **67.9** (ช้าลง) | 335.9 → **67.4** (5.0×) | 3,111 → **67.8** (45.9×) |
| route สุดท้ายในลิสต์ | 94.6 → **54.8** (1.7×) | 479.7 → **62.6** (7.7×) | 4,268 → **58.0** (73.6×) |
| ไม่มี host ตรง → default | 82.3 → **55.7** (1.5×) | 364.1 → **55.7** (6.5×) | 3,143 → **55.2** (56.9×) |
| host มี port + ตัวใหญ่ | 79.7 → **79.7** (เท่าเดิม) | 365.8 → **78.2** (4.7×) | 3,148 → **78.6** (40.1×) |
| wildcard host (`*.x`) | 667 → **81.4** (8.2×) | 5,993 → **83.7** (71.6×) | 59,341 → **81.9** (724×) |

เวลาที่ใช้แทบไม่ขึ้นกับจำนวน route แล้ว (55–84 ns ตั้งแต่ 10 ถึง 1000 routes)

### 1.2 ฟังก์ชันย่อยใน request path

| ฟังก์ชัน | ก่อน | หลัง | หมายเหตุ |
|---|---|---|---|
| `normalize_host` (host ปกติ) | 43.0 ns | **26.5 ns** | คืน `Cow::Borrowed` ไม่ต้องจัดสรร |
| `normalize_host` (มี port) | 68.1 ns | **52.4 ns** | จัดสรรครั้งเดียวแทนสองครั้ง |
| `normalize_host` (IPv6) | 35.8 ns | **19.2 ns** | |
| `hash_key(host, path)` | 32.6 ns | 32.6 ns | เรียกเฉพาะ service ที่ใช้ `lb = "hash"` แล้ว |

### 1.3 Config reload

| งาน | 10 routes | 100 routes | 1000 routes |
|---|---|---|---|
| parse + validate TOML | 92.5 µs | 881 µs | 8.86 ms |
| `RuntimeConfig::from_config` (รวมสร้าง index) | 13.8 µs | 155 µs | **1.91 ms** (เดิม 1.32 ms) |
| **รวมต่อ 1 reload** | ~0.11 ms | ~1.0 ms | **~10.8 ms** |

## 2. อ่านตัวเลขข้างบนยังไง

### แก้แล้ว: wildcard host ไม่จัดสรรหน่วยความจำต่อ route อีกแล้ว

เดิม `matches_host()` เรียก `format!(".{suffix}")` ทุกครั้งที่เทียบกับ wildcard route หนึ่งตัว
ที่ 1000 wildcard routes จึงกิน 59.3 µs ต่อ request (เพดาน ~16.8k rps/core)

ตอนนี้ wildcard เก็บใน map แล้ว lookup ด้วยการไล่ตัด label ทีละชั้น
(`a.b.example.com` → `b.example.com` → `example.com` → `com`) = hash ไม่กี่ครั้งตามจำนวน label
ไม่ขึ้นกับจำนวน route → **81.9 ns (เร็วขึ้น 724 เท่า)**

### แก้แล้ว: route matching ไม่เป็นเชิงเส้นอีกแล้ว

host เข้า hash map (FxHash) แล้ว path เดินบน radix trie → O(len(path)) ไม่ใช่ O(จำนวน route)
ที่ 1000 routes เร็วขึ้น 40–74 เท่า และเวลาที่ใช้คงที่ไม่ว่าจะมี 10 หรือ 1000 routes

### แก้แล้ว: allocation ต่อ request

`request_filter` ไม่มี allocation เหลือแล้ว — `.clone()` ที่เหลือทั้งหมดเป็น `Arc` refcount
(`normalize_host` คืน `Cow::Borrowed`, ชื่อ route เป็น `Arc<str>`, ไม่ copy path/host เข้า ctx,
และ `hash_key` ถูกเรียกเฉพาะ service ที่ใช้ `lb = "hash"`)

### สิ่งที่แลกไป (ต้องรู้ไว้)

1. **สร้าง index แพงขึ้น**: `RuntimeConfig::from_config` ที่ 1000 routes 1.32 ms → 1.91 ms (+45%)
   เกิดเฉพาะตอน reload อยู่นอก request path และยังต่ำกว่างบ 5 ms ที่ T102 ตั้งไว้
   รวมกับ TOML parse แล้ว reload 1000 routes ใช้ ~10.8 ms
2. **เคสดีที่สุดของ scan เดิมช้าลงเล็กน้อย**: "route แรกจาก 10 routes" 52 ns → 56–68 ns
   (วัดได้ไม่นิ่งบนคอนเทนเนอร์ 4 vCPU นี้ ต่างกันได้ ~20% ระหว่างรอบ)
   เพราะต้อง hash host ก่อนแทนที่จะเทียบ route แรกแล้วจบ
   เคสอื่นที่ 10 routes เร็วขึ้นหมด (route สุดท้าย 1.7×, default 1.5×, wildcard 8.2×)
   ผมเลือกไม่เพิ่ม code path พิเศษสำหรับตารางเล็กเพื่อไล่ ~10 ns เพราะได้ไม่คุ้มความซับซ้อน

### ยังไม่ได้แก้

`config_parse_validate/1000` = 8.86 ms ยังเป็นต้นทุนหลักของ reload
ถ้าอยากให้ UI ตอบสนองเร็วขึ้นตอนกด Save ที่ config ใหญ่มาก ต้องไปดูตรง TOML parse
(เกี่ยวกับ [T204](tasks/T204-atomic-apply-reload.md) และ [T307](tasks/T307-toml-editor-diff-apply.md))

## 3. Proxy vs nginx vs haproxy (ยังไม่ได้วัด)

harness พร้อมแล้ว (`bench/`) แต่ยังไม่มีตัวเลข เพราะสภาพแวดล้อมที่พัฒนาอยู่ไม่มี Docker

วิธีเติมตารางนี้:

```bash
scripts/bench.sh --all h1-keepalive
scripts/bench.sh --all many-conns
scripts/bench.sh --all h2
cp bench/results/*-<sha>.json bench/results/baseline/
```

| Scenario | Metric | prx | nginx | haproxy |
|---|---|---|---|---|
| h1-keepalive | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h1-keepalive | p99 | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h1-keepalive | CPU µs/req | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| many-conns (10k) | peak RSS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| h2 | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |
| large-body (64KB) | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่วัด_ |

**เป้าหมายที่ตั้งไว้** (จาก `docs/tasks/README.md`): RPS ต่อ core เท่าหรือดีกว่า,
p99 ดีกว่า ≥ 20% ตอน connection reuse สูง, peak RSS ที่ 10k conns ต่ำกว่า ≥ 30%

## 4. ข้อจำกัดของวิธีวัดนี้

- วัดบนเครื่องเดียว ผ่าน loopback/bridge network — ไม่มี latency ของเครือข่ายจริง
  ผลจึงเป็น "ขอบบนของ software overhead" ไม่ใช่ตัวเลข production
- backend จำลองตอบจาก memory ทันที — งานจริงที่ backend ช้ากว่านี้มาก ส่วนแบ่งของ proxy จะเล็กลงตามไปด้วย
- ไม่ได้วัด TLS handshake (ต้องรอ [T111](tasks/T111-tls-perf-multicert.md))
- ปิด access log ทั้งหมดเพื่อความเป็นธรรม — เปิดแล้วทุกตัวจะช้าลงคนละแบบ (ดู [T403](tasks/T403-access-log-async-json.md))
- ยังไม่มีตัวเลขจากเครื่อง bare-metal

## 5. สิ่งที่เจอระหว่างทำ Phase 0 (ไม่ใช่เรื่อง performance แต่ต้องรู้)

- `vendor/pingora-core` คอมไพล์ไม่ผ่านกับ libc ที่ lock ไว้ (`initgroups` รับ `gid_t` บน Linux
  แต่โค้ดส่ง `c_int`) — แก้แล้ว ไม่งั้นวัดอะไรไม่ได้เลย
  (ภายหลัง T005 อัปเป็น pingora 0.9 ซึ่ง upstream แก้เรื่องนี้แล้ว จึงลบ `vendor/` ทิ้งทั้งหมด)
- `cargo audit` แดงอยู่ก่อนแล้ว: `pingora-cache` 0.7.0 (cache poisoning, 8.4 high) และ `h2` 0.4.13
  → แก้แล้วใน [T005](tasks/T005-dependency-security-upgrade.md)
- `metrics::observe_request` ถูกเรียกอยู่ใต้ `if !self.access_log { return; }` ใน `logging()`
  แปลว่า **ปิด access log แล้ว metrics ของ request หายไปด้วย** → บันทึกไว้ที่ [T401](tasks/T401-metrics-expansion.md)

## 6. วิธี reproduce

อ่าน [`bench/README.md`](../bench/README.md) — มีข้อกำหนดเครื่อง, วิธีรัน, และเหตุผลว่าทำไมผลบนแล็ปท็อปเชื่อไม่ได้

## 7. WAF: prx + WAF vs nginx + ModSecurity vs Caddy + Coraza (ยังไม่ได้วัด)

แผน เกณฑ์ชนะ (W1–W6) และวิธีวัดทั้งหมดอยู่ที่ [`WAF-BENCH-PLAN.md`](WAF-BENCH-PLAN.md) — เกณฑ์ถูกเขียนไว้**ก่อน**วัด
harness พร้อมและรันครบทุก target แล้ว (Phase A) แต่ตัวเลขที่มีตอนนี้มาจากแล็ปท็อป (macOS + OrbStack)
ซึ่งตามกติกาของหน้านี้**ห้ามเอามาเคลม** จึงไม่ใส่ในตาราง — ดูได้ที่หัวข้อ "Phase A execution log" ของแผน

ทุก target โหลด SecLang config ไฟล์เดียวกัน (`bench/waf/main.conf`) และ CRS **v4.21.0 (`2ac6c00`)** ชุดเดียวกัน
PL1, anomaly threshold 5, blocking, ไม่ตรวจ response body, ปิด audit log — จุดที่ engine ใดทำตามไม่ได้ต้องจดใน `bench/waf/DEVIATIONS.md` (ตอนนี้ว่าง)

```bash
bench/waf/fetch-crs.sh
python3 bench/waf/gen-corpus.py bench/waf/crs bench/waf/corpus 127.0.0.1:18080
TARGETS="nginx nginx-modsec caddy caddy-coraza prx" scripts/bench.sh --all waf-get          # saturation → RPS, CPU
RATE=<n> TARGETS="..." scripts/bench.sh --all waf-get                                       # fixed rate → p99
bench/waf/verdicts.sh <target> && bench/waf/verdicts.sh --diff <a> <b>                      # W1
```

สิ่งที่ยืนยันแล้วและไม่ขึ้นกับเครื่อง: **ModSecurity กับ Coraza ตัดสิน block/allow ตรงกันทั้ง 1,861 URL ของ corpus**
(benign 1,000 → 200 ทั้งหมด, attack 861 → บล็อก 580) prx-waf จึงต้องตรงครบทุกข้อก่อนจะมีสิทธิ์ถูกวัดความเร็ว

| Scenario | Metric | nginx-modsec | caddy-coraza | prx-waf |
|---|---|---|---|---|
| waf-get | RPS (2 cores) | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-get | p99 ที่ rate เดียวกัน | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-get | WAF tax: CPU µs/req ที่เพิ่ม | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-attack-mix | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-attack-only | RPS | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-form | RPS / p99 | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-json-16k (970 leaf) | RPS / p99 | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| waf-json-128k (7,815 leaf) | วินาทีต่อ request | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |
| — | PSS ที่ CRS โหลดครบ (idle / peak ตอนตรวจ body พร้อมกัน) | _ยังไม่วัด_ | _ยังไม่วัด_ | _ยังไม่มี_ |

