# แผนแยก — Plugin architecture + WAF (Phase 5)

**สถานะ:** draft · ยังไม่เริ่มทำ
**ขอบเขต:** `src/plugin/`, `src/waf/`, `src/proxy.rs`, `src/runtime.rs`, `src/config.rs`, `webui/`
**ไม่ทับกับ roadmap เดิม:** Phase 0–4 ใน [`tasks/README.md`](tasks/README.md) เดินต่อได้อิสระ
งานในแผนนี้แตะ hot path จึงมีกติกาของตัวเองอยู่ท้ายเอกสาร

---

## 1. ทำไมต้องมี plugin ก่อน ค่อยมี WAF

ตอนนี้ prx มี "plugin" อยู่แล้ว 5 ตัว เพียงแต่ฝังตายอยู่ใน `request_filter` / `response_filter`:
header rules, rate limit, concurrency limit, micro-cache, compression ทั้งหมดเขียนซ้ำรูปแบบเดียวกัน —
อ่าน config ที่คอมไพล์ไว้ใน `RouteRuntime`, ตัดสินใจ, แล้วอาจตอบกลับเองหรือปล่อยผ่าน

ถ้าเอา WAF ยัดเข้าไปอีกตัวโดยไม่ทำ abstraction ก่อน `request_filter` จะกลายเป็นฟังก์ชัน 400 บรรทัด
ที่ไม่มีใครกล้าแก้ และลำดับการทำงาน (WAF ต้องมาก่อนหรือหลัง rate limit?) จะถูกกำหนดโดย
"ใครเขียน if ไว้ก่อน" แทนที่จะเป็น config

**ลำดับจึงเป็น: ทำ plugin core → ย้ายของเดิมมาอยู่บนมัน (พิสูจน์ว่า API ใช้ได้จริงและไม่ช้าลง) → แล้วค่อยทำ WAF เป็น plugin ตัวที่หก**

การย้ายของเดิม (T502) ไม่ใช่ refactor เพื่อความสวย — มันคือการทดสอบว่า API รองรับทุกรูปแบบที่มีอยู่จริง
(ตอบกลับเอง, แก้ header, ถือ resource ที่ต้องคืนตอนจบ, เขียน response body) **ก่อน** ที่จะมีคนนอกมาเขียนพึ่งมัน

---

## 2. การตัดสินใจเรื่อง WAF engine

คำถามคือ "ใช้ ModSecurity ได้ไหม" — ได้สามทาง และทางที่ตรงที่สุดคือทางที่แย่ที่สุดสำหรับโปรเจกต์นี้

### A. FFI ไป libmodsecurity3

ของมีจริง: [`modsecurity`](https://crates.io/crates/modsecurity) 1.0.0 + `modsecurity-sys` (Apache-2.0,
repo `rkrishn7/rust-modsecurity`) เป็น high-level wrapper ของ libmodsecurity3

เหตุผลที่**ไม่**เอาเป็นตัวหลัก:

1. **API เป็น synchronous ทั้งชุด** — `msc_process_request_headers` และเพื่อน ๆ เป็น blocking call
   ใน worker ของ pingora ที่เป็น async แปลว่าต้องเลือกระหว่างบล็อกเวิร์กเกอร์ (ฆ่า p99 ของทุก request
   ที่แชร์เธรดนั้น) หรือโยนไป `spawn_blocking` ทุก request (จ่ายคอนเท็กซ์สวิตช์ + ย้าย buffer ข้ามเธรด)
   ทั้งสองทางขัดกับ T101–T103 ที่อุตส่าห์ตัด allocation ต่อ request ออกไป
2. **PCRE backtracking** — CRS มี regex หลายร้อยตัว การันตี worst case ไม่ได้ WAF ที่ถูก DoS
   ด้วย input ที่ทำให้ regex ระเบิดคือ WAF ที่กลายเป็นช่องโหว่เสียเอง
3. **เอา C++ ก้อนใหญ่มาวางบน data path ของตัวที่ทำหน้าที่ security** — memory-safety ของ Rust
   หายไปตรงจุดที่ต้องการมันที่สุด
4. libmodsecurity v3 อยู่ใต้ OWASP หลัง Trustwave ส่งมอบ ทรัพยากรที่ใส่เข้าไปน้อยกว่าเดิม
   — **ต้องเช็คสถานะ upstream ปัจจุบันอีกครั้งใน T504** ไม่ใช่เชื่อบรรทัดนี้

### B. pure-Rust SecLang engine ที่มีคนทำไว้แล้ว

- [`zentinel-modsec`](https://crates.io/crates/zentinel-modsec) 0.4.0 — อ้าง "full OWASP CRS compatibility",
  มี feature `libmodsec-compare` (แปลว่าเขา differential-test กับตัวจริง ซึ่งเป็นสัญญาณที่ดี)
- [`barbacane-waf`](https://crates.io/crates/barbacane-waf) 0.1.1 — "SecLang rule engine in pure Rust"

ทั้งคู่ยัง 0.x และทั้งคู่เป็นส่วนหนึ่งของ **proxy คู่แข่ง** (zentinelproxy.io / parapet)
การพึ่งมันแปลว่าฝากคอขวดด้านความเร็วและ roadmap ไว้กับคนอื่น
แต่ก็**ไม่ควรตัดทิ้งโดยไม่วัด** — ถ้าตัวใดตัวหนึ่งดีจริง การใช้มันย่อมชนะการเขียนเอง T504 จึงต้องรันของจริงเทียบ

### C. เขียน SecLang engine เองใน tree ← **ตัวหลักที่แผนนี้เลือก**

ชิ้นส่วนที่ไม่ต้องเขียนเอง:
- `@detectSQLi` / `@detectXSS` → [`barbacane-libinjection`](https://crates.io/crates/barbacane-libinjection)
  หรือ [`libinjectionrs`](https://crates.io/crates/libinjectionrs) (BSD-3, port ของ libinjection
  ที่ differential-test กับ C แล้ว)
- regex → `regex` / `regex-automata` ที่มีอยู่แล้วในระบบนิเวศ
- prefilter → `aho-corasick`

ข้อได้เปรียบที่สำคัญที่สุดไม่ใช่ความเร็ว แต่คือ **`regex` ของ Rust รับประกัน linear time**
WAF ที่ไม่มีทาง ReDoS ได้เลยโดยโครงสร้าง เป็นคุณสมบัติที่ libmodsecurity ให้ไม่ได้

**ข้อเสียที่ต้องยอมรับตรง ๆ:** งานใหญ่ที่สุดในสามทาง และคำว่า "compatible" เป็นสเปกตรัม ไม่ใช่ yes/no

### สิ่งที่ตัดสินไปแล้ว และสิ่งที่ยังไม่ตัดสิน

- **ตัดสินแล้ว:** rule format คือ **SecLang / SecRules** เพื่อให้ CRS ใช้ได้ (ผู้ใช้เลือก)
- **ตัดสินแล้ว:** ทาง A จะมีอยู่ในฐานะ **plugin ที่เปิดด้วย feature flag** (`--features waf-libmodsecurity`)
  สำหรับคนที่ต้องการ compat 100% วันนี้และยอมจ่ายค่าของมัน — นี่คือประโยชน์ของการมี plugin architecture
  มันทำให้เรื่องนี้เป็น **ตัวเลือกใน config ไม่ใช่การ fork โปรเจกต์**
- **ยังไม่ตัดสิน และ T504 คือคนตัดสิน:** เขียนเอง (C) หรือใช้ของที่มี (B)

---

## 3. ตัวเลขเดียวที่ตัดสินทุกอย่าง

> **CRS กี่เปอร์เซ็นต์ที่คอมไพล์ผ่าน `regex` ของ Rust ได้?**

`regex` ไม่รองรับ backreference และ lookaround เพราะนั่นคือราคาของการรับประกัน linear time
CRS ส่วนใหญ่เลี่ยงสองอย่างนี้อยู่แล้ว (เพราะ PCRE มันช้า) แต่ "ส่วนใหญ่" ไม่ใช่ตัวเลข

**T504 ต้องได้ตัวเลขนี้มาก่อนที่จะมีใครเขียน parser สักบรรทัด** และ kill criteria คือ:

| ผลที่วัดได้ | ทำอะไรต่อ |
|---|---|
| ≥ 95% ของ rule คอมไพล์ผ่าน | เขียน engine เองตามแผน T505–T508 · rule ที่เหลือ log ว่า skip และบอกใน UI ตรง ๆ |
| 80–95% | เขียนเอง แต่ต้องมี fallback engine สำหรับ rule ที่เหลือ (ขอบเขตเพิ่ม ~1 task) |
| < 80% | **พลิกแผน** → ทำ proxy-wasm tier (T510) ขึ้นมาก่อน แล้วรัน `coraza-proxy-wasm` ซึ่ง compat อยู่แล้ว |

ตัวเลขนี้ประเมินจากความทรงจำไม่ได้ ต้องดาวน์โหลด CRS จริงแล้วรัน

---

## 4. สถาปัตยกรรม

### 4.1 Hook phase แมปกับ pingora ที่มีอยู่

| prx plugin phase | pingora hook | มีอยู่แล้ว? | ทำอะไรได้ |
|---|---|---|---|
| `on_request_head` | `request_filter` (หลัง route match) | ✅ | อ่าน/แก้ req header, ตอบกลับเอง, ปฏิเสธ |
| `on_request_body` | `request_body_filter` | ❌ **ต้องเพิ่ม (T503)** | ดู body ทีละ chunk แบบมีเพดาน |
| `on_upstream_request` | `upstream_request_filter` | ✅ | แก้ header ที่จะส่งไป upstream |
| `on_response_head` | `response_filter` | ✅ | แก้ status/header, บล็อกขากลับ |
| `on_response_body` | `response_body_filter` | ✅ | ตรวจข้อมูลรั่ว, rewrite body |
| `on_log` | `logging` | ✅ | audit, คืน resource |

WAF ต้องการครบทุก phase — และ `on_request_body` คือช่องที่ยังไม่มี จึงเป็น task แยก (T503)
ไม่ใช่รายละเอียดปลีกย่อยของ T501

### 4.2 Plugin trait — ราคาต้องเป็นศูนย์เมื่อไม่ได้เปิด

```rust
pub enum PluginDecision {
    Continue,
    /// ตอบกลับเองแล้วจบ request (เช่น WAF บล็อก 403)
    Respond { status: u16, body: Bytes },
}

#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn name(&self) -> &str;
    /// phase ที่ plugin นี้สนใจ — ใช้สร้าง bitmask ตอน compile chain
    fn phases(&self) -> PhaseMask;

    async fn on_request_head(&self, _req: &mut RequestHead<'_>, _st: &mut PluginState)
        -> Result<PluginDecision> { Ok(PluginDecision::Continue) }
    // ... phase อื่นมี default เหมือนกัน
}
```

สามข้อที่ทำให้มันไม่ทำให้ช้าลง:

1. **`PhaseMask` ต่อ chain** — `RouteRuntime` เก็บ `phase_mask: u8` ที่ OR รวมจากทุก plugin ในเชน
   `request_filter` เช็ค 1 บิตก่อน ถ้าไม่มีใครสนใจ phase นี้ก็ไม่มีทั้งลูป ทั้ง vtable dispatch
   **route ที่ไม่มี plugin จ่าย 1 bit test ต่อ request** ไม่ใช่การวนลูปบน `Vec` ว่าง
2. **คอมไพล์เชนตอน reload ไม่ใช่ตอน request** — `Box<[Arc<dyn Plugin>]>` อยู่ใน `RouteRuntime`
   สร้างครั้งเดียวใน `RuntimeConfig::from_config` เหมือนที่ `CompiledHeaderRules` ทำอยู่แล้ว
3. **State ต่อ request จัดสรรเมื่อจำเป็นเท่านั้น** — `PluginState` เป็น slab ใน `RequestCtx`
   ที่ `None` จนกว่าจะมี plugin ที่ประกาศว่าต้องใช้ ถ้า route ไม่มี plugin `RequestCtx` ไม่โตขึ้นเลย

**ข้อนี้ต้องพิสูจน์ด้วยตัวเลข ไม่ใช่ด้วยเหตุผล** — T501 ต้องแนบ before/after จาก `benches/routing.rs`
และเกณฑ์ผ่านคือ route ที่ไม่มี plugin ต้องไม่ช้าลงเกิน noise ของ harness

### 4.3 Config shape

plugin ประกาศครั้งเดียวระดับ global แล้ว route อ้างด้วยชื่อ — แบบเดียวกับที่ route อ้าง `service` อยู่แล้ว

```toml
[[plugin]]
name = "waf-main"
kind = "waf"
enabled = true

[plugin.config]
rules = ["/etc/prx/crs/crs-setup.conf", "/etc/prx/crs/rules/*.conf"]
paranoia_level = 1
anomaly_threshold = 5
mode = "blocking"        # blocking | detection
request_body_limit = 131072
audit_log = true

[[route]]
name = "api"
service = "api"
path_prefix = "/"
plugins = ["waf-main"]   # ลำดับใน array = ลำดับการทำงาน
```

ผลที่ตามมาซึ่งต้องทำให้ครบตาม Definition of Done ของ repo:
`Prx.toml`, `docs/CONFIG-WIKI.md`, type ฝั่ง WebUI, `config_edit.rs` path syntax, และ `validate.rs`

---

## 5. WAF จะเร็วได้อย่างไร

CRS มีกฎระดับ 900+ ข้อ ถ้ารัน regex ทุกข้อกับทุกตัวแปรของทุก request จบเห่ตั้งแต่ก่อนเริ่ม
สี่อย่างนี้คือที่มาของความเร็ว และต้องอยู่ในดีไซน์ตั้งแต่แรก ไม่ใช่ไปปรับทีหลัง:

1. **Literal prefilter ตัวเดียวสำหรับทั้ง ruleset** — ดึง literal substring ออกจาก regex ของทุกกฎ
   สร้าง `aho-corasick` automaton **หนึ่งตัว** กฎไหนที่ literal ไม่โดนก็ไม่ต้องรัน regex เลย
   (หลักการเดียวกับที่ `regex` ใช้ภายใน และที่ Hyperscan ทำ) — นี่คือตัวที่เปลี่ยนจาก O(rules) เป็น O(1) โดยประมาณ
2. **`RegexSet` ต่อ target** — กฎที่ยิงใส่ตัวแปรเดียวกัน (เช่น `ARGS`) รวมเป็น set เดียว
   สแกน input รอบเดียวได้คำตอบว่ากฎไหนโดนบ้าง แทนที่จะวน N รอบ
3. **Cache ผลของ transformation ต่อตัวแปร ไม่ใช่ต่อกฎ** — `t:lowercase,t:urlDecodeUni,t:removeWhitespace`
   ในกฎ CRS ซ้ำกันมหาศาล ทำครั้งเดียวต่อ (ตัวแปร, ชุด transformation) ต่อ request แล้ว memoize
   ถ้าไม่ทำข้อนี้ เวลาจะหมดไปกับ urldecode ซ้ำ ๆ มากกว่ากับการ match
4. **ดึงตัวแปรแบบ lazy** — อย่า parse body เป็น `ARGS_POST` ถ้าไม่มีกฎที่เปิดอยู่เล็งไปที่ `ARGS_POST`
   ruleset ที่คอมไพล์แล้วรู้ล่วงหน้าว่าต้องการตัวแปรอะไรบ้าง

### Budget ที่ต้องถือ

| กรณี | เพดาน | ใครบังคับ |
|---|---|---|
| route ที่ไม่มี plugin | 0 (ไม่เกิน noise ของ bench) | T501 |
| route ที่มี plugin แต่ไม่ใช่ WAF | < 100 ns ต่อ phase ที่เปิด | T502 |
| WAF เปิด, CRS PL1, request ปกติไม่มี body | p99 เพิ่ม < 1 ms | T508 |
| WAF เปิด + ตรวจ body 128 KB | p99 เพิ่ม < 5 ms | T508 |
| RSS ที่ CRS โหลดครบ | < 50 MB เหนือ baseline | T508 |

ตัวเลขพวกนี้เป็น**สมมติฐานตั้งต้นที่ตั้งไว้ให้วัดแล้วเถียงได้** ไม่ใช่ข้อสรุป
T508 มีสิทธิ์แก้มันพร้อมเหตุผลและตัวเลขประกอบ

---

## 6. ลำดับงาน

```
T501 plugin core ─┬─ T502 ย้ายของเดิมมาเป็น plugin
                  └─ T503 body inspection pipeline
                            │
                  T504 spike: วัด CRS compat + เทียบ engine ← ประตูตัดสิน
                            │
        ┌───────────────────┼────────────────────┐
     (≥80%)                                   (<80%)
        │                                        │
  T505 SecLang parser                      T510 proxy-wasm tier
  T506 operators + transformations          + coraza-proxy-wasm
  T507 CRS + anomaly scoring
  T508 WAF perf gate
        └──────────────┬─────────────────────────┘
                 T509 WAF ใน Web UI + audit
```

| ID | งาน | Size | ขึ้นกับ |
|---|---|---|---|
| [T501](tasks/T501-plugin-core.md) | Plugin trait, registry, chain ที่คอมไพล์ตอน reload | L | — |
| [T502](tasks/T502-builtins-as-plugins.md) | ย้าย header/rate limit/concurrency/cache/compression มาเป็น plugin | M | T501 |
| [T503](tasks/T503-body-inspection.md) | `request_body_filter` + buffering แบบมีเพดาน | L | T501 (เฉพาะส่วนต่อ plugin) |
| [T504](tasks/T504-waf-engine-spike.md) | **วัด CRS compat, เทียบ 4 ทางเลือก, เขียน decision record** | M | — |
| [T505](tasks/T505-seclang-parser.md) | Parser ของ SecLang → rule model ที่คอมไพล์แล้ว | L | T504 |
| [T506](tasks/T506-waf-operators.md) | Operators, transformations, libinjection | L | T505 |
| [T507](tasks/T507-crs-anomaly-scoring.md) | CRS: anomaly scoring, paranoia level, exclusion | L | T506 |
| [T508](tasks/T508-waf-performance.md) | Prefilter, RegexSet, perf gate พร้อมตัวเลข | M | T507 |
| [T509](tasks/T509-waf-webui-audit.md) | หน้า WAF ใน Web UI + audit log + ปรับ false positive | M | T507 |
| [T510](tasks/T510-proxy-wasm-tier.md) | proxy-wasm ABI (แผนสำรองของ WAF และทางให้คนนอกเขียน plugin) | L | T501 |

### Milestone

- **W1 — "prx ขยายได้"** : T501 + T502 → ของเดิมทั้งหมดวิ่งบน plugin API โดยตัวเลข perf ไม่ถอย
- **W2 — "ตัดสินใจบนตัวเลข"** : T503 + T504 → รู้แล้วว่า CRS เข้ากับ Rust regex แค่ไหน และจะเดินทางไหน
- **W3 — "WAF ใช้ได้จริง"** : T505–T508 → CRS PL1 รันได้ บล็อกของจริงได้ อยู่ใน budget
- **W4 — "คนอื่นใช้เป็น"** : T509 (+T510) → ปรับจูน false positive ได้จาก UI โดยไม่ต้อง ssh

---

### ทำ plugin ทีหลังได้ไหม

ได้ — และ dependency จริงผูกกันน้อยกว่าลำดับข้างบนมาก มีแค่ **2 ใบจาก 9 ที่ต้องมี T501 จริง ๆ**
คือ T502 (ซึ่งคือการย้ายเอง) กับ T510 (WASM) ส่วน T504 ไม่แตะโค้ดเบสเลย และ T505–T509
เป็นเรื่องภายใน WAF ล้วน

ลำดับแบบ **WAF ก่อน plugin** จึงเป็นไปได้:

```
T504 (spike)  →  T503 ครึ่งแรก (request_body_filter เปล่า ๆ)
              →  T505–T508 (WAF เป็น builtin ตัวที่หก เหมือนอีก 5 ตัวที่มีอยู่)
              →  T509 (UI)
              →  T501 + T502 ทีหลัง โดย WAF กลายเป็นตัวที่หกที่ถูกย้าย
```

**ราคาที่จ่าย** มีสามอย่าง และไม่มีอันไหนที่กู้ไม่ได้:

1. `request_filter` ยาวขึ้นอีกก่อนจะสั้นลง — คืนทุนตอนทำ T502
2. ลำดับของ WAF เทียบกับ rate limit/cache ถูกกำหนดโดยตำแหน่งของ `if` แทนที่จะเป็น config
   จนกว่าจะทำ T501
3. **ข้อที่ต้องระวังที่สุด: config shape** — ถ้า WAF ออกมาเป็น `[waf]` แล้ววันหนึ่งกลายเป็น
   `[[plugin]] kind = "waf"` นั่นคือ breaking change ที่ผลักต้นทุนไปให้ผู้ใช้

   แต่ข้อนี้**ไม่บังคับให้ต้องตัดสินใจตอนนี้** เพราะ repo มีกติกาอยู่แล้วว่าของเดิมต้องใช้ได้ต่อ
   โดยไม่ต้องแก้ไฟล์ (ดู acceptance criteria ของ T502) — `[waf]` ก็ได้การปฏิบัติแบบเดียวกัน
   คือถูกเก็บไว้เป็น sugar ที่คอมไพล์เป็น waf plugin ตอน `from_config`

**ข้อที่เสียถ้าทำ WAF ก่อนจริง ๆ คือการพิสูจน์ API** — T502 มีไว้เพื่อบังคับให้ของเดิม 5 ตัว
วิ่งบน plugin API ก่อนประกาศว่านิ่ง ถ้า WAF (ซึ่งเป็นตัวที่เรียกร้องจาก API มากที่สุด — ต้องการ
ทุก phase, ต้องการ body, ต้องการ state ข้าม phase) ถูกเขียนก่อนที่ API จะมีอยู่ API ที่ออกแบบทีหลัง
ก็จะถูกดัดให้เข้ากับสิ่งที่ WAF บังเอิญทำไปแล้ว แทนที่จะออกแบบจากความต้องการของทั้ง 6 ตัวพร้อมกัน

**ไม่ว่าจะเลือกลำดับไหน T504 ควรเป็นใบแรก** เพราะมันให้ข้อมูลมากที่สุดต่อแรงที่ลงไป
และอาจเปลี่ยนแผนทั้งเฟส

## 7. ความเสี่ยงที่รู้ตัวแล้ว

| ความเสี่ยง | ทำไมมันจริง | ทางรับมือ |
|---|---|---|
| CRS compat ต่ำกว่าที่คิด | `regex` ไม่มี backreference/lookaround | T504 วัดก่อนเขียน, มี kill criteria ชัด |
| Body buffering กิน RAM | WAF ต้องเห็น body ทั้งก้อนถึงจะตรวจ `ARGS_POST` ได้ | เพดานต่อ request + ต่อ instance, เกินแล้วเลือกได้ว่า block หรือ pass พร้อม log |
| False positive ทำของพัง | CRS PL1 ยัง block ของที่ถูกต้องได้ | `mode = "detection"` เป็นค่าเริ่มต้น, ต้องตั้งใจเปลี่ยนเป็น blocking เอง + T509 ทำ workflow ปรับจูน |
| WAF ทำให้ p99 พัง | CRS คือ regex หลายร้อยตัว | budget ในข้อ 5 + perf gate ใน CI (T508) ที่แดงได้ |
| Plugin API ผิดตั้งแต่แรก | ออกแบบจากจินตนาการว่าคนอื่นต้องการอะไร | T502 บังคับให้ของเดิม **5 ตัว** วิ่งบนมันก่อนจะประกาศว่า API นิ่ง |
| ผูกกับ crate ของ proxy คู่แข่ง | `zentinel-modsec` / `barbacane-waf` เป็นของ product อื่น | ถ้า T504 เลือกทางนั้น ต้องประเมินเรื่อง vendoring และ license ด้วย ไม่ใช่แค่ benchmark |
| เอา C++ มาไว้ใน data path | ทาง A | ไม่ใช่ default, อยู่หลัง feature flag, และบอกใน UI ว่ากำลังรันโหมดไหน |

---

## 8. กติกาของ Phase นี้ (เพิ่มจาก Definition of Done ของ repo)

1. **ทุก task ที่แตะ hot path ต้องแนบ before/after** จาก `scripts/bench.sh` หรือ `benches/` — กติกาเหล็กของ repo ข้อเดิม
2. **"route ที่ไม่ได้เปิด plugin ต้องไม่ช้าลง"** เป็น acceptance criterion ของทุก task ใน Phase นี้ ไม่ใช่แค่ T501
3. **WAF ต้องมี corpus ทดสอบสองฝั่ง** — payload ที่ต้องบล็อก (จาก CRS regression suite) และ
   traffic ปกติที่ต้องไม่บล็อก การวัดแค่ฝั่งเดียวคือวิธีสร้าง WAF ที่บล็อกทุกอย่างแล้วอ้างว่าปลอดภัย
4. **`mode = "detection"` เป็นค่าเริ่มต้นเสมอ** — WAF ที่เปิดมาแล้วบล็อกทันทีคือ WAF ที่ถูกปิดถาวรในสัปดาห์แรก
5. **กฎที่คอมไพล์ไม่ได้ต้องดังกว่าเงียบ** — นับ, log, แสดงใน UI ห้าม skip เงียบ ๆ
   เพราะ WAF ที่คิดว่าตัวเองรัน 900 กฎแต่จริง ๆ รัน 600 คือความปลอดภัยปลอม

---

## 9. สิ่งที่อยู่นอกแผนนี้

- WAF แบบ machine learning / anomaly detection ที่ไม่ใช่ rule-based
- Managed rule feed แบบเสียเงิน (Cloudflare/AWS ruleset)
- Bot management, CAPTCHA, JS challenge
- DDoS mitigation ระดับ L3/L4 — คนละปัญหากับ WAF
- ภาษา scripting สำหรับ plugin (Lua/JS) — ถ้าจะมี ให้มาทีหลัง T510 และใช้ทางเดียวกับ WASM
