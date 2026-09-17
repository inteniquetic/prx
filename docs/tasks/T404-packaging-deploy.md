# T404 — Packaging: Docker slim, systemd, Helm

**Phase:** 4 · Observability & Ops
**Status:** todo
**Size:** M (~1d)
**Depends on:** T310
**Files:** `Dockerfile`, `ops/systemd/`, `ops/helm/`, `Makefile`

## เป้าหมาย

ติดตั้งง่ายพอๆ กับ nginx ไม่งั้นต่อให้เร็วกว่าก็ไม่มีใครย้ายมา

## สถานะปัจจุบัน

มี `Dockerfile` และ `make build_image` อยู่แล้ว ยังไม่มี systemd unit, ไม่มี Helm chart,
ไม่มี multi-arch build และยังไม่ได้ build `webui` ในขั้น builder

## ขอบเขตงาน

1. Dockerfile: multi-stage (build webui → build rust → runtime distroless/alpine),
   non-root user, healthcheck ที่ใช้ `health_path`, multi-arch (amd64/arm64), รายงานขนาด image ใน CI
2. systemd unit ใน `ops/systemd/prx.service`: `Restart=always`, `LimitNOFILE`,
   hardening (`NoNewPrivileges`, `ProtectSystem=strict`, `AmbientCapabilities=CAP_NET_BIND_SERVICE` สำหรับพอร์ต 80/443)
   + วิธี reload (`systemctl reload` → SIGHUP)
3. Helm chart ใน `ops/helm/prx`: Deployment + Service + ConfigMap (Prx.toml) + ServiceMonitor +
   PodDisruptionBudget + readiness/liveness ผูกกับ `/healthz` `/readyz` + ตัวอย่างค่า resource
4. GitHub Action release: tag → build multi-arch image + binary artifact (Linux amd64/arm64) + checksum
5. `docs/DEPLOY.md`: docker run, docker compose, systemd, k8s — อย่างละตัวอย่างเดียวที่ก๊อปแล้วใช้ได้เลย

## Acceptance criteria

- [ ] image ขนาดรายงานใน CI และไม่โตขึ้นเงียบๆ (มี budget)
- [ ] container รันเป็น non-root และ serve ทั้ง proxy + UI ได้
- [ ] `helm install` บน kind cluster แล้ว pod ready และ proxy ทำงาน (เทสต์ใน CI แบบ smoke)
- [ ] systemd unit ผ่าน `systemd-analyze security` ในระดับที่ยอมรับได้ และ reload ไม่ drop connection

## Out of scope

- Operator / CRD
