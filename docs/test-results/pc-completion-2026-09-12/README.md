# Local completion evidence — 2026-09-12

This directory records bounded Windows measurements from the local completion run rooted at `runtime/local-completion-20260912`. The copied JSON files are the small review set; the larger raw runtime files remain local and are referenced by the evidence manifest when available.

The environment was Windows 11 Pro build 26200 on a 24 logical processor i9-12900K machine. The network scope was loopback. These results do not measure a remote PC, Tailscale, WebRTC/ICE, browser paint, a physical mobile device, or a VM.

## Measured results

- The WS credit deadline previously expired while no bytes were in flight. After 21 seconds idle, the next output could close the socket before its ACK. The fixed path starts the deadline on new outstanding output and renews it only when ACKs release bytes. [credit-before.json](credit-before.json) records the failure; [credit-after3.json](credit-after3.json) verifies delayed ACK survival and closure after 20.02 seconds when duplicate ACKs make no progress.
- The native layout probe moved the target from `y=1067.21875` with no hit to `y=204` with a button hit. See [native-geometry/geometry-0.json](native-geometry/geometry-0.json) and [native-smoke-fixed3/geometry-0.json](native-smoke-fixed3/geometry-0.json).
- Native lifecycle evidence recorded 50 completed cycles with no reported errors and cleanup enabled. The 100-tab switching run was `MEASURED`, completed 100 samples, and reported p50 73.45 ms and p95 78.40 ms. See [native-lifecycle-50/result.json](native-lifecycle-50/result.json) and [native-tabs-100b/result.json](native-tabs-100b/result.json).
- Session lifecycle completed 500 of 500 requested sessions. Its latency array is retained in [session-lifecycle-500.json](session-lifecycle-500.json).
- The normal browser run measured 10,000 echo samples: p50 1.10 ms, p95 1.50 ms, p99 2.10 ms, with zero performance drops. The stress run measured another 10,000 samples while a 1 MiB/s workload ran for 10 minutes, then recorded 100 recovery samples; early, pre-recovery, and final drops were zero. The burst run measured 100 samples with a 5 MiB/s workload and 100 recovery samples; drops were zero. See [browser-normal-final.json](browser-normal-final.json), [browser-stress3-10k-10m.json](browser-stress3-10k-10m.json), and [browser-burst-final.json](browser-burst-final.json).
- The Ctrl+C result measures Agent receive to PTY write. It does not claim target-program termination. The recorded baseline and stress p95 values were 0.045 ms and 0.037 ms; status was `MEASURED`. See [ctrlc.json](ctrlc.json).

The combined local check joined 10,000 baseline echo samples, 10,000 stress samples, Node gateway/Vite HMR, two PTYs, and owned native capture/encode. It recorded 24 HMR updates, zero performance drops, hardware encoding, and a p95 change from 1.6 ms to 1.5 ms (-0.1 ms). The media report requested 1280x720 at 15 fps for 60 seconds and ran the encoder for 70 seconds with the resource callback. See [combined-final2/result.json](combined-final2/result.json) and [combined-final2/media/result.json](combined-final2/media/result.json).

The media report keeps source and encoded rates separate: requested/source profile was 15 fps, actual encoded fps was about 14.98, and the report also records unique pushed source frames and their rate. Those fields are not interchangeable.

## Resource measurements

The sampler reports whole-machine CPU percentage and private working set. The CPU values below are the weighted means for the explicitly tracked product group; memory values are maximum private working set. Private bytes are retained separately in the JSON and are not substituted for private working set.

| Scenario | Requested / elapsed | Product processes | CPU weighted mean | Private working set max |
| --- | ---: | ---: | ---: | ---: |
| Agent only, idle, 10 minutes | 600 s / 600.07 s | 1 | 0% | 10,403,840 bytes (9.92 MiB) |
| Native idle, 10 minutes | 600 s / 600.17 s | 8 | 0.0293% | 119,275,520 bytes |
| Native output, 10 minutes | 600 s / 600.16 s | 8 | 1.3068% | 215,343,104 bytes |
| 720p/15 hardware capture + encode | 600 s / 600.11 s | 2 | 0.0920% | 90,890,240 bytes |
| 1080p/30 hardware capture + encode | 600 s / 600.11 s | 2 | 0.1398% | 93,523,968 bytes |

The two media rows track Agent plus the native encoder benchmark with local UI
closed; their animated-window fixture is reported separately. Each has zero
invalid resource samples (534 and 533 samples respectively). Encoding ran for
610 seconds around each 600-second resource measurement and produced 14.9975
and 29.9951 encoded buffers/s. The fixture delivered only 12.6389 and 21.1080
unique pushed source frames/s, respectively: repeated-frame encoding is not proof
of a fresh 15/30-fps viewer. See [720p](media-720-10m/result.json),
[1080p](media-1080-10m/result.json), and
[identity-checked process exit](media-cleanup.json). These close bounded host CPU
and private-working-set observations, not WebRTC/decoder/viewer acceptance.

The Agent-only idle result contains 553 valid samples over 600.071721 seconds. CPU was zero at the sampler's resolution and private working set was constant at 10,403,840 bytes (9.92 MiB). The separate idle2 and idle8 reports are also retained. All three mark `identity_metadata_legacy=false`; they represent different tracked workloads and should not be read as a legacy/current build comparison. The idle2 and idle8 product CPU weighted means were approximately 0.000109% and 0%, with product private working set p95 values of 2,514,944 and 4,329,472 bytes. See [idle0-final-summary.json](idle0-final-summary.json), [idle2-summary.json](idle2-summary.json), [idle8-summary.json](idle8-summary.json), [native-idle-10m/summary.json](native-idle-10m/summary.json), and [native-output-10m/summary.json](native-output-10m/summary.json).

The device quota now counts active credentials rather than retained revoked records. Regression coverage preserves revoked-record authentication rejection and the 64-active-device ceiling; all 49 Agent tests passed. A direct post-fix run then reused the database containing 64 revoked records without resetting it, authenticated two new pairings, completed the terminal/reconnect/resource smoke, and revoked both fixture credentials during cleanup. The database ended with 66 revoked and zero active records. See [review-device-quota.md](review-device-quota.md), [device-quota-before.json](device-quota-before.json), [device-quota-after.json](device-quota-after.json), and [soak-resource-smoke/result.json](soak-resource-smoke/result.json). The resource smoke covered 20.05 seconds, six probes, and three resource samples; it is not soak evidence.

The current unsigned standard and offline archives pass extraction and integrity validation without executing either installer. Each archive contains the expected 61-file inventory plus 1,032 license/SBOM files. The packaged 605-component SBOM passes the CycloneDX schema check. The packaged native desktop completed two close/reopen cycles while preserving the Agent epoch and PTY session identity; cleanup completed. See [package-validation.json](package-validation.json) and [package-native-smoke/result.json](package-native-smoke/result.json). These checks do not replace clean-VM install, upgrade, or uninstall acceptance.

The output fixture achieved 102,399.94 bytes/s for 610 seconds around the 600-second measurement. Its Node generator was measured separately for an overlapping 300 seconds: mean CPU 1.60%, max private working set 23,662,592 bytes. That shorter workload sample is not added to a full ten-minute product total. The native output window was visible and not minimized at the recorded observation; foreground was false.

Per-process CIM GPU dedicated-memory counters were contradictory on this PC: one encoder reported 9.04 GB while the same adapter reported only 845 MB total. These raw fields are **unreliable for GPU allocation/leak acceptance**. CPU/private memory measurements remain separate. [gpu-counter-validation.json](gpu-counter-validation.json) records the identity-checked comparison. Microsoft describes a similar counter problem in [KB 4490156](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/gpu-process-memory-counters-report-wrong-value); this is supporting context, not proof that this Windows 11 run has the identical Windows 10 defect.

## Trace and raw data boundary

The capped trace summary recorded 8,356,916 samples, zero drops, and a truncated raw event section at the configured 50 MiB trace cap. This cap bounds one captured artifact. It is not an aggregate seven-day logging policy, retention guarantee, or capacity claim. See [capped-trace-summary.json](capped-trace-summary.json).

Normal tracing writes to stdout and creates no product-owned diagnostic-file directory. Audit metadata has seven-day/10,000-row pruning. `RC_PERF_TRACE` is an explicit, one-shot export with a <=50 MiB cap; retention of caller-selected exports is not implemented as automatic directory cleanup.

## Pending or still running

The remaining local long-duration item is in progress and must not be represented as a pass until its completed result exists:

- **IN_PROGRESS** — the requested 24-hour soak started at 2026-09-11 21:32:05 UTC (2026-09-12 06:32:05 KST), and its first probe was observed. Its earliest valid finish is 2026-09-12 21:32:05 UTC (2026-09-13 06:32:05 KST). See [soak-24h-launch.json](soak-24h-launch.json).

No pending item is represented as a pass by the results above.

## Evidence and limits

[evidence-files.json](evidence-files.json) lists the copied files and hashes. Matching JSON files should remain siblings of this README, with the native subdirectories preserved. The source raw files under `runtime/local-completion-20260912` remain local for later manifest reconciliation.

These are bounded local completion measurements. They support the scopes stated in each report, including the native capture/encode smoke and the Node gateway/Vite HMR check. They do not establish remote WebRTC behavior, browser paint latency, physical mobile behavior, VM behavior, or seven-day aggregate logging behavior.
