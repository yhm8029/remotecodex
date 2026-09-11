# Device quota fix review

Result: **APPROVED: no P1 finding.**

The pairing quota now counts only devices whose credentials remain active. Revoked device records still remain in memory and SQLite, their cancellation tokens remain cancelled, and all authentication entry points continue to reject them. The change therefore releases capacity after revocation without restoring revoked credentials or weakening the 64-active-device limit.

The three regressions cover the observed lifetime-tombstone failure, persistence across `Auth` reconstruction, continued rejection of an old revoked device, preservation of 64 revoked records plus one active record, and rejection of a 65th simultaneously active device with `RateLimited`. The reported red run reproduced the old failure, and the corrected suite passed all 49 Agent tests.

No store or schema change is needed for this defect. Revoked-record retention remains unchanged and should not be folded into this fix without a separate retention and audit policy.
