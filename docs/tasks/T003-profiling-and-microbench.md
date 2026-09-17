# T003 — Flamegraph + criterion micro-bench

**Phase:** 0 · Measurement
**Status:** todo
**Size:** M (~1d)
**Depends on:** T001
**Files:** `Cargo.toml`, `benches/` (ใหม่), `scripts/profile.sh`, `docs/PROFILING.md`

## เป้าหมาย

หาให้เจอว่า CPU หมดไปกับอะไรใน hot path และมี micro-bench ที่จับ regression ระดับ ns ได้

## ขอบเขตงาน

1. เพิ่ม `[dev-dependencies] criterion` + `[[bench]]` และ profile `[profile.bench]`/`[profile.release] debug = 1`
   เพื่อให้ symbol ออกใน flamegraph
2. `benches/routing.rs`:
   - `select_route` กับ config 10 / 100 / 1000 routes, host ตรง/ไม่ตรง/wildcard/default fallback
   - `normalize_host`, `hash_key`
3. `benches/config.rs`: parse + validate + `RuntimeConfig::from_config` (ใช้ตอนวัด reload cost)
4. `scripts/profile.sh`: รัน prx release + `cargo flamegraph` (หรือ `perf record`) ระหว่างยิง load จาก T001
   แล้ว output `bench/results/flamegraph-<sha>.svg`
5. `docs/PROFILING.md`: วิธีอ่าน flamegraph, วิธีเปิด `tokio-console` (feature flag `tokio-console` แบบ opt-in)

## Acceptance criteria

- [ ] `cargo bench` รันได้และมี baseline เก็บไว้
- [ ] มี flamegraph ของ scenario `h1-keepalive` commit ไว้ พร้อมสรุป top-5 hot frame
- [ ] `docs/PROFILING.md` ทำตามแล้วได้ flamegraph ใหม่ได้จริง

## วิธีทดสอบ

```bash
cargo bench -- routing
bash scripts/profile.sh h1-keepalive
```

## Out of scope

- การแก้ code ตามผล profile (ไปที่ T101/T102/T103)
