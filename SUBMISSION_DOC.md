# T3N ADK Bounty — Submission

**Submitting tenant:** `did:t3n:5db3681df85b9a698777a5aa603329da86cdb5dc`
**Environment:** testnet · `@terminal3/t3n-sdk@4.30.0` · Node v24.18.0 · Rust 1.97.1 · `wasm32-wasip2` · wil-bindgen 0.49
**Date:** Aug 2026

---

## 1. Public GitHub repo

**URL (public):** `https://github.com/Henoch4/t3n-adk-bounty`

Contains:
- `my-t3n-app/` — TypeScript SDK harness (tsx scripts): auth, walkthrough, deploy-contracts, per-caller quota demo
- `z-agent-paywall/` — custom paywall agent-gate contract (WIT + Rust)
- `z-quota-counter/` — custom quota-counter contract (WIT + Rust)
- `z-tenant-flight/` — reference flight contract
- `verification/` — WIT proof extracts + verbatim live transcripts
- `screenshots/` — terminal captures of the live runs
- `BUGS.md` — full findings write-up (#1–#8)

---

## 2. What was built

Two custom **Rust→WASM TEE contracts**, built from scratch on this machine and deployed + executed **live on the T3N testnet** through the official SDK:

| Contract | Tail | contract_id | What it proves |
|---|---|---|---|
| `z-agent-paywall` | `agent-paywall` | **568** (v0.2.1) | Per-session budget + per-call cap enforced inside the enclave before an agent tool dispatch; identity-bound caller key |
| `z-quota-counter` | `quota-counter` | **569** (v0.3.1) | KV-backed per-caller quota counters with hard-stop at `limit`, 24h reset window, ceiling-clamped first-touch |

Reference flight contract also re-registered live: `travel-contracts` → contract_id **567**.

Both contracts: ~160 KB WASM (well under the 1 MiB limit), WIT-world-verified by `wasm-tools component wit`, with enclave logs flushed via `logging.info` and read back via `tenant.contracts.logs`.

---

## 3. Screenshots (live, verified on the real testnet)

> Screen captures below are committed in `screenshots/` in the repo.

**[INSERT IMAGE: walkthrough success.png — reference flight contract registered live, contract_id 567]**

**[INSERT IMAGE: deploy-contracts success.png — both custom contracts registered (paywall 568, quota-counter 569) + live gate/pay + quota runs]**

**[INSERT IMAGE: demo-quota credit error.png — credit grant exhausted; InsufficientCredit error, documented as BUGS.md #8]**

**Paywall gate — live output (transcript):**
```
[check-gate]  25   => allowed true,  caller_key did:bound:5db…, remaining 500
[check-gate]  70   => allowed false (per-call cap 50 exceeded)
[enter-gateway]     => granted true,  spent 25, remaining 475
[enter-gateway]     => granted true,  spent 50, remaining 450
[check-gate] 400    => allowed false (per-call cap 50 exceeded)
[pay-for-service]   => paid, intent pi_enc_50_60, 10c, remaining 440
```

**Quota counter — hard-stop + reset (live run):**
```
consume(app2, limit=3, x4) => used 1, 2, 3 (at_limit), exceed@4 (stays 3)
check(app2)                => used 3 / 3, remaining 0
reset(app2)                => used 0 / 3
```

Key behaviors proven live:
- Caller keys are host-bound (`deterministic DID`, not client-supplied `caller`) — no spoofing.
- Per-call cap enforced on all three gate functions.
- Quota hard-stop refuses to over-write; first-touch limits clamped to the tenant ceiling.
- 24h reset window rolls over automatically from the cluster clock.

---

## 4. Bugs / issues faced (BUGS.md, all reproduced)

1. **Free-token balance is not provable via the SDK** on this cluster — `getBalance()` throws `Invalid character` (atob); `getUsage()` throws `expected struct GetUsageParams`. Blocks the "claim free tokens" walkthrough step.
2. **Re-registration needs a strictly-higher semver and mints a fresh `contract_id`** — every hotfix is version bump + map-ACL re-grant + new id; the old id keeps an orphaned grant.
3. **`maps` scope "only: [contract_id]" is silently lost on upgrade** — no way to keep the tenant owner writable while a single contract reads.
4. **`fuel_per_minute` cap trips after ~10 KV-heavy calls** in a single demo script (reproduced live).
5. **`tenant.tenant.me()` returns quota/status fields** (`max_wasm_bytes` family) not present in the SDK docs.
6. **1 MiB WASM cap shared across interfaces** — pruned `http` imports keep the customs ~160 KB; the reference with `http` climbs over 200 KB.
7. Env: `wasm-tools` compile needed disk freed first (transient).
8. **`InsufficientCredit` error is unit-confusing** — a read-only `contracts.logs` call demands **10,000,000,000 units** while the grant is 20,000 **tokens** (1 token = 1,000,000 units); the error never prints the unit. Reads as a catastrophic mismatch until a top-up; confirmed working end-to-end after a 40K-token top-up.

Bonus observations recorded in BUGS.md: `http-with-placeholder` & host interfaces reachable; `tenant.contracts.register`, `maps.create/update/entrySet`, `logging` and `kv-store` all work well.

---

## 5. Deliverables checklist

- ✔️ **Public GitHub repo** (link in section 1)
- ✔️ **Agent ID / DID claimed + connected** (`did:t3n:5db3681d…b5dc`)
- ✔️ **TEE contract deployed + executed live** — two custom ones (paywall 568, quota 569) + reference (567)
- ✔️ **Screenshots** — committed + inserted above
- ✔️ **Bugs faced** — `BUGS.md` #1–#8, inline error quotes, reproduced
- ⚠️ **Free-token balance proof** — SDK balance helpers throw on this cluster (BUGS.md #1); the 20K grant is unit-denominated (1 token = 1e6 units) and `InsufficientCredit` errors never say the unit (#8). A 40K-token top-up cleared the block and the full demo ran again live (Pass C).

---

## 6. How to reproduce

```bash
# clone the public repo
git clone https://github.com/Henoch4/t3n-adk-bounty.git && cd t3n-adk-bounty

# SDK scripts
cd my-t3n-app && npm install
npx tsx quickstart.ts        # auth (balance paths throw — BUGS.md #1)
npx tsx deploy-contracts.ts # paywall + quota live demo (register + gates + logs)

# Rust builds
cd ../z-agent-paywall && cargo build --target wasm32-wasip2 --release
cd ../z-quota-counter && cargo build --target wasm32-wasip2 --release
```

_(Assumes your own `.env` with a testnet `T3N_API_KEY`; testnet is limited by the per-minute fuel cap and 20K credit grant, see BUGS.md #4 and #8.)_