# PC/mobile invitation connection — 2026-09-14

The approved design replaces separate host address and ticket entry with a local QR or copied HTTPS invitation. The receiving device still acknowledges the experimental build and explicitly registers. Successful host metadata stores only origin and label; device keys continue using the existing authentication storage.

## Implemented boundaries

- Strict versioned URL-fragment payload, bounded UTF-8/base64url, canonical HTTPS `.ts.net` origin, no query/credentials/path alternatives. Browser import binds to the current origin; the fragment is scrubbed synchronously before parsing.
- Host invitation requires the connected local owner API, fresh owned/HTTPS-ready Serve with no restart pending, and the matching running Agent origin. Ticket scopes explicitly exclude GUI permissions. QR generation is local. No invite persistence or external QR service.
- One-use/300-second expiry enforcement remains the existing Agent API; the UI clears QR/link on expiry and disposal, and does not renew automatically.
- Address changes, failed imports and local-host transitions clear pending invitations. Local owner ticket issuance is restricted to the exact loopback target and disabled for remote targets. Astra reviewed and confirmed both integration fixes.

## Validation

- `npm run check:web`: 0 errors, 0 warnings.
- `node --experimental-strip-types --test tests/web/*.test.mjs`: 9 passed, including malformed/canonical origin handling, Unicode and metadata-only storage. Missing modules produced the initial RED failures before implementation.
- `tests/e2e/invitations.mjs`: 3 groups passed: readiness/scopes, local QR/clipboard/virtual 5-minute expiry without renewal, paste with no automatic API.
- Existing `tests/e2e/tailscale-setup.mjs`: 11 groups passed.
- Actual App browser harness: 4 groups passed, including explicit QR-fragment registration at 390x844 with no horizontal overflow, address/invalid-import ticket clearing, native host transition constrained to loopback, and foreign-origin rejection/scrubbing without API.
- Packaging and installed-runtime evidence will be recorded after execution.

The E2E harness uses Playwright with `RC_PLAYWRIGHT_MODULE` pointing to an installed Playwright ESM module and `PLAYWRIGHT_BROWSERS_PATH` pointing to its matching browser assets. Calls use isolated in-memory mocks and fabricated tickets; these tests do not establish authenticated connectivity from a second physical PC or phone.

## M3 delegation and corrected instructions

See m3-calls.json for direct response token counts and accepted/rejected units. M3 wrote small storage, UI, App and test functions; the strict invitation codec used the approved Luna fallback after a truncated response and a failed validation retry. Parent integration, review and fixture plumbing are not represented as M3-only implementation.

Failures changed subsequent prompts: provide actual API signatures, state fields and DOM selectors; give test functions the existing fixture rather than ask for a new harness; explicitly distinguish browser globals from Node execution. The parent initially instructed clock installation after navigation, which misses the already-created interval. Moving clock installation before navigation fixed that harness error; it is not counted as M3 coding inability. These lessons are now in AGENTS.md.
