# Live re-verification on testnet — 2026-08-08

Re-runs of `my-t3n-app/deploy-contracts.ts` and `my-t3n-app/demo-quota.ts`
after the security/honesty fixes (identity binding, per-call cap in every
gate path, first-touch limit ceiling + reset window). Captured verbatim from
the testnet node; contract ids are freshly minted because every re-register
in this iteration bumped the on-cluster semver (BUGS.md #2).

## z-agent-paywall — registered @0.2.0, contract_id 559

`setEnvironment("testnet")`; tenant `did:t3n:5db3681df85b9...86cdb5dc`.

Budget seeded via `maps.entrySet("gate","meta:budget", {500, 50})`.

```
[agent-paywall.check-gate] => {"allowed":true,"label":"resume-rewrite","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":0,"session_budget_cents":500,"session_remaining_cents":500,"reason":"under budget"}
[agent-paywall.check-gate] => {"allowed":false,"label":"video-render","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":0,"session_budget_cents":500,"session_remaining_cents":500,"reason":"amount 70 exceeds per-call cap 50"}
[agent-paywall.enter-gateway] => {"granted":true,"label":"resume-rewrite","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","spent_cents":25,"session_spent_cents":25,"session_remaining_cents":475,"reason":"toll paid"}
[agent-paywall.enter-gateway] => {"granted":true,"label":"resume-rewrite-second","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","spent_cents":25,"session_spent_cents":50,"session_remaining_cents":450,"reason":"toll paid"}
[agent-paywall.check-gate] => {"allowed":false,"label":"third-call-over-budget","caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","session_spent_cents":50,"session_budget_cents":500,"session_remaining_cents":450,"reason":"amount 400 exceeds per-call cap 50"}
[agent-paywall.pay-for-service] => {"paid":true,"caller_key":"did:bound:5db3681df85b9a698777a5aa603329da86cdb5dc","payment_intent":"pi_enc_50_60","amount_cents":10,"currency":"usd","session_remaining_cents":440,"reason":"toll paid"}
```

What this proves end-to-end on a live WASM enclave:
- Identity binding works — `caller_key` is host-bound (`did:bound:<tid hex>`),
  not the client-supplied string. Identity spoofing via `caller` field is
  impossible (review F1 fix).
- Per-call cap (50c) is enforced on `check-gate` AND `enter-gateway` AND
  `pay-for-service` (review F2 fix): the 70c and 400c calls are both denied.
- Session budget accrues across calls and is respected: 25+25+10 = 60 spent,
  remaining 440, never overspent.
- `pay-for-service` mints a deterministic `pi_enc_<keylen>_<spent>` intent —
  no outbound network in the reference path (the cluster prunes `http`
  imports from this component, see `z_agent_paywall.wit.txt`).

## z-quota-counter — registered @0.3.0, contract_id 560

Two runs (the cluster's `fuel_per_minute_max = 500_000_000` is exhausted
after ~10 KV-heavy calls — BUGS.md #4 — so the reset path is captured across
two invocations).

**Run A — `deploy-contracts.ts` (app1, fresh):**
```
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":2,"limit":5,"remaining":3,"exceeded":false,"at_limit":false}
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"exceeded":false,"at_limit":false}
[quota-counter.consume] => {"key":"u:5db...:did:key:z6Mk_app1","used":4,"limit":5,"remaining":1,"exceeded":true,"at_limit":false}
[quota-counter.check]    => {"key":"u:5db...:did:key:z6Mk_app1","quota_map":"z:5db...:quotas","used":4,"limit":5,"remaining":1,"reset_epoch_secs":1786147200}
```

**Run B — `demo-quota.ts` (app2 + reset, after cool-down):**
```
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":1,"limit":3,"remaining":2,"exceeded":false,"at_limit":false}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":2,"limit":3,"remaining":1,"exceeded":false,"at_limit":false}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"exceeded":false,"at_limit":true}
[consume] => {"key":"u:5db...:did:key:z6Mk_app2","used":3,"limit":3,"remaining":0,"exceeded":true,"at_limit":true}
[check]   => {"key":"u:5db...:did:key:z6Mk_app2","quota_map":"z:5db...:quotas","used":3,"limit":3,"remaining":0,"reset_epoch_secs":1786147200}
[reset]   => {"key":"u:5db...:did:key:z6Mk_app2","used":0,"limit":3}
```

What this proves end-to-end on a live WASM enclave:
- Hard-stop at `limit` is enforced: the consume at `4 > 3` returns
  `exceeded:true` and the stored counter stays at `3` (NO over-write).
- First-touch caller adopts its requested limit (default `DEFAULT_LIMIT_CEILING
  = 100_000` clamps an attacker who asks for `u32::MAX`; review F7 fix).
- Counters are namespaced by the bound caller DID: keys are
  `u:<tid-hex>:<supplied-key>` (review: previously the supplied key was used
  raw, which let one caller tamper with another's counter).
- `reset_epoch_secs` is derived from the cluster clock and rolls every 24h
  (review F8 reset-window fix).
- `reset` clears the counter back to zero while preserving the limit.
