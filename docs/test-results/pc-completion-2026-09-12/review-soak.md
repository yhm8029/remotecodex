# Soak resource-gate review

Reviewed `tests/perf/soak.mjs` after the resource-gate fix.

## Finding and resolution

The earlier implementation could return `PASS_BOUNDED` for a 24-hour run without an Agent process manifest. That skipped resource sampling, PID creation-time validation, and the memory-growth gate.

The full-duration invocation now rejects a missing manifest before it creates the run directory or starts fixtures. A 24-hour `PASS_BOUNDED` additionally requires all of the following:

- elapsed loop time and observed resource duration of at least 86,400 seconds;
- at least two post-stabilization resource samples;
- a finite private-byte OLS slope; and
- `leak_flag === false`.

Otherwise the result is `FAIL` with incomplete resource evidence. Sampler errors, identity mismatches, missing or duplicate process rows, resource observation gaps, and fixture cleanup errors still propagate as failures.

## Evidence and scope

`tests/perf/soak-resources.test.mjs` passed its seven focused checks: exact 10 MiB/hour threshold, declining/stable case, insufficient trend span, PID creation-time reuse, duplicate/missing rows, and non-monotonic samples. The full 24-hour command without a manifest was also verified to reject before fixtures.

The planned 20-second sampler-integrated smoke has not run yet, and no 24-hour soak has passed. This review only confirms that a future full-duration pass requires the resource evidence above.

Integration follow-up: the 20-second sampler-integrated run completed with SMOKE_PASS, six probes, three resource samples spanning20.1324386seconds, no errors. See `soak-resource-smoke/result.json`. This does not satisfy the24-hour gate.
