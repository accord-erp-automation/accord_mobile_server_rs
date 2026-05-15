# Session Store JSON vs LMDB Benchmark - 2026-05-15

## Scope

This benchmark compares login/session creation throughput for the JSON session
store and the optional LMDB session store.

The route under test is:

```text
POST /v1/mobile/auth/login
```

The request uses the built-in admin login path, so it does not require ERPNext
or direct database access. The benchmark isolates session creation and HTTP
handler overhead.

## Environment

- Backend repo: `accord_mobile_server_rs`
- Build: `cargo run --release --locked --bin accord_mobile_server_rs`
- Tool: ApacheBench (`ab`)
- Warmup: `ab -n 200 -c 50`
- Measured run: `ab -n 2000 -c 100`
- Request body:

```json
{"phone":"+998880000000","code":"19621978"}
```

Each backend used fresh temporary store paths.

## Results

| Backend | Requests | Concurrency | RPS | Median | p95 | p99 | Failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| JSON | 2000 | 100 | 329.91 | 300ms | 561ms | 609ms | 0 |
| LMDB | 2000 | 100 | 4683.20 | 19ms | 42ms | 62ms | 0 |

## Readout

- LMDB improved login/session creation throughput by about `14.2x`.
- LMDB reduced p95 latency from `561ms` to `42ms`.
- Both backends completed with zero failed requests.

The improvement comes from removing JSON write amplification. JSON rewrites the
session map on each login, while LMDB writes one token/value record per login.

## Baseline Note

Before the LMDB implementation, the same JSON-only code path was measured at:

| Backend | Requests | Concurrency | RPS | Median | p95 | Failed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| JSON baseline | 2000 | 100 | 338.30 | 276ms | 569ms | 0 |

The post-refactor JSON result stayed in the same range, while LMDB removed the
session-store bottleneck for this login-heavy workload.
