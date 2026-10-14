# prx benchmarks

เอกสารนี้คือแหล่งอ้างอิงเดียวของคำเคลมเรื่องประสิทธิภาพทั้งหมดของ prx
ถ้าตัวเลขไหนไม่มีในนี้ = ยังไม่ได้วัด ห้ามเอาไปเคลม

สถานะ: [T002](tasks/T002-baseline-report.md) — **micro-benchmark วัดแล้ว / proxy-vs-proxy วัดแล้ว: h1-keepalive, large-body, many-conns@2000, h1-close (ที่ `62109bc`) และ re-bench ทั้งหมดรวม many-conns@10000 ที่ `11ebe66` ([§3.6](#36-final-re-bench-after-o2--o7-sha-11ebe66)); ยังไม่มี h2**
อัปเดตล่าสุด: หลัง [T101](tasks/T101-zero-alloc-request-path.md) + [T102](tasks/T102-route-matcher-index.md)

## สรุปสั้น

| สิ่งที่อยากรู้ | สถานะ |
|---|---|
| prx เร็วกว่า nginx/haproxy ไหม | **ยังไม่ — RPS แพ้ทุก scenario ที่วัด** ที่ `11ebe66` (F1): prx/haproxy h1-keepalive 0.80×, large-body 0.98×, many-conns@2000 0.54×, many-conns@10000 0.96× (prx/nginx 0.79× / 0.86× / 0.49× / 0.86×); ดีขึ้นจาก `62109bc` +22–33% RPS ดู [§3.6](#36-final-re-bench-after-o2--o7-sha-11ebe66) (baseline เดิม: [§3](#3-proxy-vs-nginx-vs-haproxy-p0-baseline), [§3.5](#35-ที่ที่-prx-แพ้-วัดได้ที่-62109bc)) |
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

## 3. Proxy vs nginx vs haproxy (P0 baseline)

วัดที่ commit `62109bc` — proxy code เหมือน `2dc517a` ทุกบรรทัด (`git diff 2dc517a..HEAD -- src Cargo.* vendor` ว่าง; ต่างกันแค่ bench/docs)
ทุก scenario: **20 วินาที** ต่อรอบ, ทุกไฟล์ `success_rate` = 1.0 ยกเว้นที่ระบุ, RSS = `resources.peak_rss_kb` (เป็น RSS ไม่ใช่ PSS, MB = kB/1000)

### Reference machine

| Field | Value | Source command |
|---|---|---|
| CPU model | Apple M4 Pro | `sysctl -n machdep.cpu.brand_string` |
| Core count | 12 | `sysctl -n hw.ncpu` |
| RAM | 25769803776 bytes (~24 GiB) | `sysctl -n hw.memsize` |
| macOS version | macOS 26.6.2 (build 25G83) | `sw_vers` |
| Docker version | Docker Engine 29.4.0 (client 29.4.0, context: orbstack) | `docker version` |
| OrbStack VM cores | 12 | `docker info \| grep -E 'CPUs\|Total Memory'` |
| OrbStack VM RAM | 11.73 GiB | `docker info \| grep -E 'CPUs\|Total Memory'` |
| oha version | 1.16.0 | `oha --version` |
| git sha | 62109bc baseline; 11ebe66 F1 (62109bc proxy code = 2dc517a) | `git rev-parse --short HEAD` |

วิธีเติมตารางนี้:

```bash
scripts/bench.sh --all h1-keepalive   # รัน 3 รอบ -> -r1..-r3
scripts/bench.sh --all large-body
scripts/bench.sh --all many-conns
cp bench/results/*-<sha>*.json bench/results/baseline/
```

### 3.1 P0 baseline: median ของ 3 รอบ (host-measured, oha บน macOS host)

ค่าในตาราง = median ของ `-r1..-r3`; ช่วง RPS = min–max ของ 3 รอบ

| Scenario | Target | RPS median (min–max) | p50 / p99 (ms) | CPU µs/req | peak RSS | Source |
|---|---|---|---|---|---|---|
| h1-keepalive (64 conns, 1 KB) | prx | 39,071 (38,277–39,288) | 1.552 / 3.209 | 50.0 | 32.7 MB | `bench/results/baseline/h1-keepalive-prx-62109bc-r?.json` |
| h1-keepalive (64 conns, 1 KB) | nginx | 66,126 (64,551–66,462) | 0.497 / 6.668 | 27.6 | 34.1 MB | `bench/results/baseline/h1-keepalive-nginx-62109bc-r?.json` |
| h1-keepalive (64 conns, 1 KB) | haproxy | 65,240 (65,236–65,456) | 0.508 / 5.449 | 26.5 | 20.5 MB | `bench/results/baseline/h1-keepalive-haproxy-62109bc-r?.json` |
| large-body (64 conns, 64 KB) | prx | 25,528 (24,966–25,740) | 2.401 / 4.987 | 76.6 | 33.1 MB | `bench/results/baseline/large-body-prx-62109bc-r?.json` |
| large-body (64 conns, 64 KB) | nginx | 35,835 (35,752–35,859) | 0.806 / 7.692 | 55.4 | 34.3 MB | `bench/results/baseline/large-body-nginx-62109bc-r?.json` |
| large-body (64 conns, 64 KB) | haproxy | 32,519 (32,124–33,245) | 0.966 / 8.528 | 56.8 | 22.2 MB | `bench/results/baseline/large-body-haproxy-62109bc-r?.json` |
| many-conns (2000 conns, 1 KB) | prx | 28,402 (28,106–28,467) | 76.834 / 175.881 | 68.5 | 237.6 MB | `bench/results/baseline/many-conns-prx-62109bc-r?.json` |
| many-conns (2000 conns, 1 KB) | nginx | 78,333 (77,814–79,809) | 7.742 / 254.864 | 24.8 | 46.5 MB | `bench/results/baseline/many-conns-nginx-62109bc-r?.json` |
| many-conns (2000 conns, 1 KB) | haproxy | 70,578 (70,044–71,165) | 16.787 / 224.425 | 27.9 | 72.9 MB | `bench/results/baseline/many-conns-haproxy-62109bc-r?.json` |

ตัวเลข h1-keepalive รอบเก่าที่ `2dc517a` (รันครั้งเดียว: prx 38,250 / nginx 69,133 / haproxy 61,962 RPS) **ถูกแทนที่ด้วย median ข้างบนแล้ว** — ไฟล์เดิมยังอยู่: `bench/results/baseline/h1-keepalive-{nginx,haproxy}-2dc517a.json` (ไม่มีไฟล์ของ prx ที่ sha นี้)

### 3.2 h1-close (รันครั้งเดียวต่อ target, 64 conns, 20 วินาที, `--disable-keepalive`)

load generator ของ scenario นี้รัน **ใน docker network** (image `bench-oha`, `bench/oha/Dockerfile`) เพราะ macOS host ใช้ ephemeral port หมด — วินิจฉัยแล้ว (B7: os error 49, ทั้งสาม target ล้ม) (16384 ports × TIME_WAIT 30 วินาที)
จึงเทียบกันได้ระหว่าง target แต่ **เทียบกับ scenario ที่วัดจาก host (3.1) ไม่ได้**

| Target | RPS | p50 / p99 (ms) | CPU µs/req | peak RSS | Source |
|---|---|---|---|---|---|
| prx | 13,125 | 1.691 / 21.607 | 48.4 | 40.9 MB | `bench/results/baseline/h1-close-prx-62109bc.json` |
| nginx | 21,480 | 0.938 / 20.153 | 29.2 | 34.1 MB | `bench/results/baseline/h1-close-nginx-62109bc.json` |
| haproxy | 39,168 | 0.557 / 16.412 | 26.2 | 20.6 MB | `bench/results/baseline/h1-close-haproxy-62109bc.json` |

### 3.3 many-conns @10000 (รันครั้งเดียว, 20 วินาที)

| Target | RPS | p50 / p99 (ms) | CPU µs/req | peak RSS | success_rate | Source |
|---|---|---|---|---|---|---|
| prx | _ยังไม่วัด_ — unstable at the gate: ถูก success gate 0.99 ปฏิเสธ (0.988, 0.9868; 0.861 บน host ที่ไม่สะอาด) และได้ 0.9920 ใน manual run หนึ่งครั้งนอก harness | — | — | — | — | ไม่มีไฟล์ |
| nginx | 63,359 | 35.845 / 1729.991 | 30.7 | 101.0 MB | 0.99655 | `bench/results/baseline/many-conns-nginx-62109bc-c10000.json` |
| haproxy | 34,542 | 257.076 / 686.530 | 57.1 | 298.1 MB | 0.995015 | `bench/results/baseline/many-conns-haproxy-62109bc-c10000.json` |

### 3.4 h2

| Target | RPS | Source |
|---|---|---|
| prx / nginx / haproxy | _ยังไม่วัด_ — gap: ไม่ได้ติดตั้ง `h2load` (ตั้งใจไม่ติดตั้ง) | ไม่มีไฟล์ |

**เป้าหมายที่ตั้งไว้** (จาก `docs/tasks/README.md`): RPS ต่อ core เท่าหรือดีกว่า,
p99 ดีกว่า ≥ 20% ตอน connection reuse สูง, peak RSS ที่ 10k conns ต่ำกว่า ≥ 30%

### 3.5 ที่ที่ prx แพ้ (วัดได้ที่ 62109bc)

ทุกตัวเลขคำนวณจาก median ใน §3.1 / ค่ารันเดียวใน §3.2 — % = (prx / คู่แข่ง − 1), × = prx / คู่แข่ง
สาเหตุ: **ไม่ทราบสาเหตุ — ดู `.team/PLAN-PERF.md` (P1–P4 ยังไม่เสร็จ)** ทุกข้อ ยังไม่มี source line ไหนที่ยืนยันได้ จึงไม่ระบุผู้ต้องสงสัย

| Scenario | Metric | prx | vs haproxy | vs nginx | Task ที่รับผิดชอบ |
|---|---|---|---|---|---|
| h1-keepalive | RPS | 39,071 | −40.1% (65,240) | −40.9% (66,126) | [T104](tasks/T104-socket-worker-tuning.md), [T003](tasks/T003-profiling-and-microbench.md); [T101](tasks/T101-zero-alloc-request-path.md) (applied แล้ว — ช่องว่างยังอยู่) |
| h1-keepalive | CPU µs/req | 50.0 | 1.89× (26.5) | 1.81× (27.6) | [T003](tasks/T003-profiling-and-microbench.md), [T104](tasks/T104-socket-worker-tuning.md); [T101](tasks/T101-zero-alloc-request-path.md) (applied แล้ว — ช่องว่างยังอยู่) |
| large-body | RPS | 25,528 | −21.5% (32,519) | −28.8% (35,835) | [T104](tasks/T104-socket-worker-tuning.md), [T105](tasks/T105-upstream-pool-keepalive.md) |
| large-body | CPU µs/req | 76.6 | 1.35× (56.8) | 1.38× (55.4) | [T003](tasks/T003-profiling-and-microbench.md) |
| many-conns@2000 | RPS | 28,402 | −59.8% (70,578) | −63.7% (78,333) | [T104](tasks/T104-socket-worker-tuning.md), [T105](tasks/T105-upstream-pool-keepalive.md) |
| many-conns@2000 | CPU µs/req | 68.5 | 2.46× (27.9) | 2.76× (24.8) | [T003](tasks/T003-profiling-and-microbench.md) |
| many-conns@2000 | peak RSS | 237.6 MB | 3.3× (72.9) | 5.1× (46.5) | [T105](tasks/T105-upstream-pool-keepalive.md) |
| h1-close (in-network) | RPS | 13,125 | −66.5% (39,168) | −38.9% (21,480) | [T104](tasks/T104-socket-worker-tuning.md) |
| h1-close (in-network) | CPU µs/req | 48.4 | 1.85× (26.2) | 1.66× (29.2) | [T003](tasks/T003-profiling-and-microbench.md) |
| many-conns@10000 | success_rate | unstable at the gate (0.988, 0.9868; 0.861 บน host ที่ไม่สะอาด; manual 0.9920 นอก harness) | — | — | [T104](tasks/T104-socket-worker-tuning.md) |

p50 ของ prx แย่กว่าทั้งสองตัวในทุก scenario ของ §3.1 และ §3.2 ด้วย

**ที่ที่ prx ชนะ (เฉพาะที่ §3 มีตัวเลข):**

| Scenario | Metric | prx | vs haproxy | vs nginx |
|---|---|---|---|---|
| h1-keepalive | p99 | 3.209 ms | −41.1% (5.449) | −51.9% (6.668) |
| large-body | p99 | 4.987 ms | −41.5% (8.528) | −35.2% (7.692) |
| many-conns@2000 | p99 | 175.881 ms | −21.6% (224.425) | −31.0% (254.864) |

h1-close p99 ของ prx (21.607 ms) **แย่กว่า** haproxy (16.412) และ nginx (20.153) — ไม่นับเป็นชัยชนะ
p99 ที่ต่ำกว่าเกิดพร้อม RPS ที่ต่ำกว่า (oha วัดแบบ saturation) จึงยังไม่ใช่การเทียบ p99 ที่ rate เดียวกัน

### 3.6 Final re-bench after O2 + O7 (sha 11ebe66)

วัดที่ commit `11ebe66` (branch `perf/allocator`: O2 `l4_read_buffer_kib` = 16 ใน bench config + O7 jemalloc), harness เดิม, 20 วินาทีต่อรอบ, 3 รอบต่อ cell
ค่า = median ของรอบที่ผ่าน gate; ช่วง RPS = min–max; RSS = `resources.peak_rss_kb` (MB = kB/1000); คำนวณด้วย python (`statistics.median`) จากไฟล์ในคอลัมน์ Source

| Scenario | Target | RPS median (min–max) | p50 / p99 (ms) | CPU µs/req | peak RSS | min success_rate | Source |
|---|---|---|---|---|---|---|---|
| h1-keepalive (64 conns, 1 KB) | prx | 51,808 (51,461–52,838) | 1.100 / 2.995 | 37.3 | 33.5 MB | 1.0 | `bench/results/h1-keepalive-prx-11ebe66-f1-c64-r?.json` |
| h1-keepalive (64 conns, 1 KB) | nginx | 65,826 (65,158–66,172) | 0.493 / 6.824 | 27.5 | 34.1 MB | 1.0 | `bench/results/h1-keepalive-nginx-11ebe66-f1-c64-r?.json` |
| h1-keepalive (64 conns, 1 KB) | haproxy | 64,696 (64,495–65,182) | 0.504 / 5.640 | 26.6 | 20.6 MB | 1.0 | `bench/results/h1-keepalive-haproxy-11ebe66-f1-c64-r?.json` |
| large-body (64 conns, 64 KB) | prx | 31,122 (31,058–31,299) | 1.951 / 4.771 | 63.5 | 34.9 MB | 1.0 | `bench/results/large-body-prx-11ebe66-f1-c64-r?.json` |
| large-body (64 conns, 64 KB) | nginx | 36,097 (35,317–36,155) | 0.805 / 7.742 | 55.1 | 34.2 MB | 1.0 | `bench/results/large-body-nginx-11ebe66-f1-c64-r?.json` |
| large-body (64 conns, 64 KB) | haproxy | 31,829 (31,519–32,215) | 0.933 / 9.032 | 58.0 | 22.1 MB | 1.0 | `bench/results/large-body-haproxy-11ebe66-f1-c64-r?.json` |
| many-conns (2000 conns, 1 KB) | prx | 37,718 (37,289–38,036) | 35.660 / 156.334 | 52.8 | 144.2 MB | 0.999996 | `bench/results/many-conns-prx-11ebe66-f1-c2000-r?.json` |
| many-conns (2000 conns, 1 KB) | nginx | 76,611 (76,468–76,891) | 12.883 / 236.777 | 25.7 | 48.3 MB | 1.0 | `bench/results/many-conns-nginx-11ebe66-f1-c2000-r?.json` |
| many-conns (2000 conns, 1 KB) | haproxy | 69,872 (69,831–70,825) | 17.361 / 220.579 | 28.3 | 72.7 MB | 1.0 | `bench/results/many-conns-haproxy-11ebe66-f1-c2000-r?.json` |
| many-conns (10000 conns, 1 KB) | prx (2 รอบ) | 36,530 (34,787–38,273) | 54.703 / 947.128 | 54.2 | 546.7 MB | 0.990567 | `bench/results/many-conns-prx-11ebe66-f1-c10000-r?.json` (r1, r3) |
| many-conns (10000 conns, 1 KB) | nginx | 42,705 (39,444–48,171) | 64.844 / 1768.856 | 34.4 | 97.9 MB | 0.991482 | `bench/results/many-conns-nginx-11ebe66-f1-c10000-r?.json` |
| many-conns (10000 conns, 1 KB) | haproxy | 38,137 (38,042–39,389) | 226.519 / 684.921 | 51.2 | 296.7 MB | 0.992735 | `bench/results/many-conns-haproxy-11ebe66-f1-c10000-r?.json` |

many-conns@10000: prx many-conns@10000 had 2 passing runs (0.990567, 0.996839) and 1 run rejected by the 0.99 gate (`success_rate 0.989606 < 0.99`); nginx 0.9929/0.9960/0.9915; haproxy 0.9927/0.9931/0.9944 — so every target sits near the gate at 10000 conns (load generator on the macOS host). median ของ prx ที่ 10000 จึงเป็นค่ากลางของ 2 รอบ

อัตราส่วน RPS (median prx / median คู่แข่ง):

| Scenario | prx / haproxy | prx / nginx | Source |
|---|---|---|---|
| h1-keepalive | 0.801 | 0.787 | `bench/results/h1-keepalive-*-11ebe66-f1-c64-r?.json` |
| large-body | 0.978 | 0.862 | `bench/results/large-body-*-11ebe66-f1-c64-r?.json` |
| many-conns@2000 | 0.540 | 0.492 | `bench/results/many-conns-*-11ebe66-f1-c2000-r?.json` |
| many-conns@10000 | 0.958 | 0.855 | `bench/results/many-conns-*-11ebe66-f1-c10000-r?.json` |

#### Before / after (prx เท่านั้น): P0 baseline `62109bc` vs F1 `11ebe66`

| Scenario | RPS 62109bc → 11ebe66 | CPU µs/req | peak RSS | Source |
|---|---|---|---|---|
| h1-keepalive | 39,071 → 51,808 (+32.6%) | 50.0 → 37.3 (−25.5%) | 32.7 → 33.5 MB (+2.2%) | `bench/results/baseline/h1-keepalive-prx-62109bc-r?.json`, `bench/results/h1-keepalive-prx-11ebe66-f1-c64-r?.json` |
| large-body | 25,528 → 31,122 (+21.9%) | 76.6 → 63.5 (−17.0%) | 33.1 → 34.9 MB (+5.7%) | `bench/results/baseline/large-body-prx-62109bc-r?.json`, `bench/results/large-body-prx-11ebe66-f1-c64-r?.json` |
| many-conns@2000 | 28,402 → 37,718 (+32.8%) | 68.5 → 52.8 (−22.9%) | 237.6 → 144.2 MB (−39.3%) | `bench/results/baseline/many-conns-prx-62109bc-r?.json`, `bench/results/many-conns-prx-11ebe66-f1-c2000-r?.json` |

#### h1-close (รันครั้งเดียวต่อ target, in-network เหมือน §3.2)

| Target | RPS | p50 / p99 (ms) | CPU µs/req | peak RSS | Source |
|---|---|---|---|---|---|
| prx | 18,060 | 1.441 / 19.032 | 43.0 | 48.1 MB | `bench/results/h1-close-prx-11ebe66-f1.json` |
| nginx | 21,357 | 0.958 / 19.928 | 29.6 | 34.1 MB | `bench/results/h1-close-nginx-11ebe66-f1.json` |
| haproxy | 50,543 | 0.543 / 13.598 | 25.4 | 26.1 MB | `bench/results/h1-close-haproxy-11ebe66-f1.json` |

รอบเดียวที่ `62109bc` (§3.2) ได้ prx 13,125 / nginx 21,480 / haproxy 39,168 — haproxy moved 39k -> 50.5k between two single runs ดังนั้นตัวเลข h1-close เป็น **indicative only** ไม่ใช้เป็นข้อสรุป (prx/haproxy 0.36, prx/nginx 0.85 ในรอบนี้)

#### Verdict

เป้าหมาย "prx เร็วกว่า haproxy/nginx" **ไม่สำเร็จในทุก scenario** — RPS ของ prx เทียบ haproxy: h1-keepalive 0.80×, large-body 0.98×, many-conns@2000 0.54×, many-conns@10000 0.96× (เทียบ nginx: 0.79× / 0.86× / 0.49× / 0.86×)
สิ่งที่ดีขึ้นจริงเทียบ `62109bc`: RPS +21.9% (large-body) ถึง +32.8% (many-conns@2000) ในทั้งสาม scenario, CPU µs/req ลดลง 17–26%, RSS@2000 ลดจาก 237.6 → 144.2 MB แต่ยังเป็น 2.0× ของ haproxy (72.7 MB) และ 3.0× ของ nginx (48.3 MB) — เป้า O2 (≤ 71.8 MB) ไม่สำเร็จ, context switch ต่อ request ลดจาก 0.19 → ~0.05 (จาก P6/O7 ใน `.team/PLAN-PERF.md`, ไม่ได้วัดใหม่ใน F1); เป้า CPU ของ O7 (h1-keepalive ≤ 35 µs/req) ไม่สำเร็จ (37.3)
สิ่งที่เหลือและสาเหตุ (measured, one heap dump per conn count, sampled estimates — P7/P5 ใน `.team/PLAN-PERF.md`): ราว 27 KiB/conn เป็น allocation ต่อ connection ภายใน pingora ที่ไม่มี public knob — `pingora-proxy lib.rs:1560` (process_new_http future, 12.0 KiB/conn), `pingora-core apps/mod.rs:265` (process_new future, 4.1), `pingora-core protocols/http/v1/server.rs:303` (INIT_HEADER_BUF_SIZE, 4.0), `pingora-core protocols/http/v1/client.rs:385` (upstream resp header reserve, 2.5); และ `getpeername` ทุกครั้งที่ reuse upstream connection จาก pool ที่ `pingora-core connectors/mod.rs:264` (check_fd_match) — ทั้งหมดต้องแก้ที่ upstream Pingora ไม่ใช่ใน prx
ตัวเลขเปอร์เซ็นต์ที่ Cloudflare เผยแพร่เกี่ยวกับ Pingora **ไม่ได้ถูกยืนยันหรือหักล้าง** โดยเอกสารนี้ — harness นี้ (cpuset 2 CPU, VM บน macOS, body 1 KB / 64 KB) ทดสอบสิ่งนั้นไม่ได้

## 4. ข้อจำกัดของวิธีวัดนี้

- วัดบนเครื่องเดียว ผ่าน loopback/bridge network — ไม่มี latency ของเครือข่ายจริง
  ผลจึงเป็น "ขอบบนของ software overhead" ไม่ใช่ตัวเลข production
- backend จำลองตอบจาก memory ทันที — งานจริงที่ backend ช้ากว่านี้มาก ส่วนแบ่งของ proxy จะเล็กลงตามไปด้วย
- ไม่ได้วัด TLS handshake (ต้องรอ [T111](tasks/T111-tls-perf-multicert.md))
- ปิด access log ทั้งหมดเพื่อความเป็นธรรม — เปิดแล้วทุกตัวจะช้าลงคนละแบบ (ดู [T403](tasks/T403-access-log-async-json.md))
- ยังไม่มีตัวเลขจากเครื่อง bare-metal
- เครื่องเดียว: Mac 12 core (Apple M4 Pro) ผ่าน OrbStack — proxy ทุกตัวถูก pin ไว้ที่ 2 CPU (`cpuset: "0,1"` ใน `bench/docker-compose.yml`), backend อีก 2 CPU
- รอบละ 20 วินาที ไม่ใช่ 60 วินาทีตามมาตรฐาน และมีแค่ 3 รอบต่อ scenario (h1-close และ many-conns@10000 รันครั้งเดียว)
- load generator ต่างที่กัน: §3.1 รัน oha บน macOS host แต่ h1-close (§3.2) รันใน docker network — เทียบข้ามกันไม่ได้
- ไม่มี h2 (ไม่ได้ติดตั้ง `h2load`)
- ไม่ได้วัด WAF (ดู §7)
- success gate = 0.99 (`scripts/bench.sh`) — รันที่ต่ำกว่านี้ถูกทิ้ง ไม่บันทึกผล
- jemalloc-sys ต้องมี C toolchain และ make ตอน build (ใช้ได้บน macOS Xcode CLT และ ubuntu CI, ช้ากว่าบน musl); Windows MSVC จะใช้ system allocator แทน

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

