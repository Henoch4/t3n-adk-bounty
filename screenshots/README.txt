Live T3N console captures for the bounty submission
====================================================

Each PNG is a hand-verified terminal capture of a real testnet run on the
tenant DID did:t3n:5db3681df85b9a698777a5aa603329da86cdb5dc. Verbatim RPC
transcripts (the full text, not just the visible frame) live in
../verification/LIVE_OUTPUTS.md.

Files
-----
- walkthrough success.png
    Script: walkthrough.ts (v0.1.1)
    Shows: Reference flight contract registered live — contract_id 567

- deploy-contracts success.png
    Script: deploy-contracts.ts (paywall 0.2.1 / quota-counter 0.3.1)
    Shows: Both custom contracts registered live — paywall 568 + quota-counter
           569; paywall gate/pay sequence runs live (identity-bound caller_key,
           per-call cap denials at 70c and 400c, pi_enc_... intent)

- demo-quota credit error.png
    Script: demo-quota.ts (0.3.1)
    Shows: The 20K test-credit grant exhausted; node returns
           InsufficientCredit (required=10000000000, available=0) for a
           read-only contracts.logs call — BUGS.md #8

- walkthrough error.png
    Shows: bumped-version register collision captured for transparency (the
           "version X not higher than current X" path from BUGS.md #2)

- deploy contracts error.png
    Shows: same — version-already-on-cluster error before the bump landed

Note on IDs
-----------
Contract IDs differ from earlier runs in LIVE_OUTPUTS.md because every fresh
contracts.register mints a new id (BUGS.md #2). The latest verified IDs are
567 (flight / travel-contracts), 568 (agent-paywall), 569 (quota-counter).
