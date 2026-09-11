# Local performance harness

These commands target an owned Release Agent listening at `127.0.0.1:3847`.
Use a fixture data directory and a new output directory for every run. Never point
lifecycle fixtures at user sessions. Pairing secrets stay in memory; result files
contain identities and measurements, not credentials.

Set `RC_PLAYWRIGHT_MODULE` to the file URL of an installed Playwright `index.mjs`.
The browser harness uses installed Chrome; native checks use Tauri Release and
WebView2. The media example needs the locally staged GStreamer SDK.

```powershell
node --test tests/perf/license-coverage.test.mjs tests/perf/soak-resources.test.mjs
.\tests\windows\resource-sample.ps1
node tests/perf/combined.mjs <agentExe> <newDirectory> <agentManifest>
node tests/perf/soak.mjs <agentExe> <newDirectory> 20 5 <agentManifest>
node tests/perf/soak.mjs <agentExe> <newDirectory> 86400 300 <agentManifest>
.\tests\perf\media-smoke.ps1 -Directory <newDirectory> -Width 1280 -Height 720 -Fps 15 -Seconds 600 -AgentManifest <agentManifest>
node scripts/summarize-resources.mjs <raw.ndjson> <newSummary.json>
```

Angle-bracket arguments above are placeholders. A process manifest contains an
array of `{Id, StartTicks, Group, Label}` objects. `Id` is an integer; `StartTicks`
is `StartTime.ToUniversalTime().Ticks.ToString()`, preserved as a string. Agent
roots use group `product`; the helper captures owned CMD children separately.
Do not infer creation time from a PID or convert ticks through a JavaScript number.

The 24-hour run requires an Agent manifest. It writes synchronous probe records,
10-second resource samples, hourly checkpoints and final summaries. Stabilization
is fixed at one hour; the post-cutoff private-byte least-squares slope is expressed
in bytes/hour, with 10 MiB/hour triggering failure/investigation. Sustained handle
or thread growth is also flagged. Missing samples, PID reuse, observation gaps,
failed echo/reconnect or cleanup prevent success. Short runs say `SMOKE_PASS`.

Only the Agent belongs to the soak product group. CMD shells are workload; the
Node echo child and Chrome automation are not included in this resource manifest.
The output-Native scenario separately measures product Agent/Tauri/WebView2 and
records the output generator's scope. Untracked processes are not zero.

The GPU CIM counters are raw provider values. On the measured PC, per-process
DedicatedUsage exceeded the same-adapter total and cannot prove allocation or a
leak. Preserve this limitation when interpreting media measurements.

`RC_PERF_TRACE` enables a one-shot metadata-only output trace at a caller-selected
absolute file path. Raw events stop at 49 MiB; bounded histograms continue and the
footer keeps the total below 50 MiB. Nonzero dropped samples invalidate latency
statistics. Trace quantiles are bucket upper bounds. Client timing ends at xterm
parse/apply; capture/encode benchmarking uses a fakesink. Neither is viewer paint.

For current results, see
[the checked-in evidence](../../docs/test-results/pc-completion-2026-09-12/README.md).
