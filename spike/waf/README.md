# T504 spike — วัดว่า CRS เข้ากับ regex ของ Rust ได้แค่ไหน

โค้ดในโฟลเดอร์นี้ไม่ได้อยู่บน build ของ prx และไม่ได้ตั้งใจให้อยู่ถาวร
ผลลัพธ์คือตัวเลขกับ [`docs/decisions/0001-waf-engine.md`](../../docs/decisions/0001-waf-engine.md)
เก็บไว้เพราะการตัดสินใจที่ reproduce ไม่ได้คือความเห็น ไม่ใช่การวัด

## ทำซ้ำ

```bash
# 1. เอา CRS รุ่นที่ปล่อยแล้วมา (ไม่ใช่ main)
git clone --depth 1 https://github.com/coreruleset/coreruleset /tmp/crs
git -C /tmp/crs fetch --depth 1 origin tag v4.21.0 && git -C /tmp/crs checkout v4.21.0

# 2. แกะทุก SecRule ออกมา
python3 extract.py /tmp/crs > /tmp/crs.json

# 3. คอมไพล์ทุก @rx ด้วย regex ของ Rust — ดิบ ๆ แล้วผ่านชั้นแปลง
cd rxprobe
cargo run --release < /tmp/crs.json               # 305/312
cargo run --release -- --translate < /tmp/crs.json # 312/312

# 4. ชั้นแปลงเปลี่ยนความหมายไหม (ต้องใช้ corpus — ดูใน extract ของ decision record)
cargo run --release --bin diff < /tmp/crs.json

# 5. naive vs RegexSet vs prefilter
cargo run --release --bin matchbench < /tmp/crs.json

# 6. crate ที่มีอยู่แล้วโหลด CRS ได้แค่ไหน
cd ../engines && cargo run --release
```

## ไฟล์

| | |
|---|---|
| `extract.py` | แกะ `SecRule` จากไฟล์ `.conf` — ต่อบรรทัด, แยก field, อ่าน action |
| `rxprobe/src/main.rs` | คอมไพล์ทุก `@rx` แล้วจำแนกสาเหตุที่ล้ม |
| `rxprobe/src/translate.rs` | ชั้นแปลง PCRE→Rust ~120 บรรทัด |
| `rxprobe/src/bin/diff.rs` | differential test: แปลงแล้วยังแมตช์เหมือนเดิมไหม |
| `rxprobe/src/bin/matchbench.rs` | เทียบสามวิธีการรัน ruleset |
| `engines/src/main.rs` | โหลด CRS ด้วย `zentinel-modsec` และ `barbacane-waf` |

## ข้อควรระวังถ้าจะเอาโค้ดนี้ไปใช้ต่อ

`translate.rs` แปลง pattern ทุกอันแล้วค่อยพิสูจน์ว่าไม่เปลี่ยนความหมาย
**T505 ไม่ควรทำแบบนี้** — ควรพยายามคอมไพล์ตรง ๆ ก่อน แล้วซ่อมเฉพาะอันที่ล้ม
(แนวทางของ `barbacane-waf`) เพราะการเขียน pattern ที่ engine รับอยู่แล้วใหม่
คือการเปลี่ยน security control โดยไม่มีใครตรวจ
