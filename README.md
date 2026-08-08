# T3N ADK Bounty — Paywall Agent Gate + Quota Counter on TEE Contracts

Claimed **Agent ID + test tokens** and deployed a first **Rust→WASM TEE
contract** on the T3N testnet. Two custom contracts, both registered and invoked
live on-cluster:

| Contract | Tail | contract_id | What it proves |
|---|---|---|---|
| `z-agent-paywall` | `z:<tid>:agent-paywall` | 559 | Paywall gate — per-session budget + per-call cap enforced inside the enclave before an agent tool dispatch; identity-bound caller key |
| `z-quota-counter` | `z:<tid>:quota-counter` | 560 | KV-backed per-caller quota counters with hard-stop at `limit`, 24h reset window, ceiling-clamped first-touch |

Both were built from scratch on this machine (Rust 1.97 → `wasm32-wasip2`,
`wit-bindgen 0.49`), mirroring the reference `z-tenant-flight` crate, and
registered via the official `@terminal3/t3n-sdk`. The contract ids above are
freshly minted from the v0.2.0 / v0.3.0 re-registration (each register call
mints a new id — BUGS.md #2).

## Repo layout

```
my-t3n-app/        TypeScript SDK harness (tsx scripts)
  quickstart.ts    auth + balance (balance path is broken — see BUGS.md)
  walkthrough.ts   reference flight contract registration (contract_id 539)
  deploy-contracts.ts paywall + quota deploy, maps, budget seed, invocations
  demo-quota.ts    quota-counter re-register + per-caller demo
z-agent-paywall/   custom Gateway contract (WIT world + Rust src)
z-quota-counter/   custom Quota counter contract
z-tenant-flight/   reference contract (registered as travel-contracts)
verification/      wasm-tools `component wit` extracts (binary-level WIT proof)
                    + LIVE_OUTPUTS.md (verbatim re-verification on testnet)
BUGS.md            SDK/deliverable findings found while building
```

## How to run (Windows / bash adaptions inline)

```bash
# 1) keys (already in .env, git-ignored)
copy your T3N_API_KEY into .env  (DID + Stripe test key optional)

# 2) SDK scripts
cd my-t3n-app && npm install && npx tsx quickstart.ts
npx tsx walkthrough.ts
npx tsx deploy-contracts.ts   # paywall + quota live demo, with kv logs

# 3) Rust
cd z-agent-paywall && cargo build --target wasm32-wasip2 --release
cd z-quota-counter && cargo build --target wasm32-wasip2 --release
```

## Live outputs (verbatim from testnet — `verification/LIVE_OUTPUTS.md`)

### Paywall — per-session budget + per-call cap, identity-bound caller
```
[check-gate]     25   => allowed true,  caller_key did:bound:5db…, remaining 500
[check-gate]     70   => allowed false (per-call cap 50 exceeded)
[enter-gateway]       => granted true,  spent 25, remaining 475
[enter-gateway]       => granted true,  spent 50, remaining 450
[check-gate]    400   => allowed false (per-call cap 50 exceeded; also over session)
[pay-for-service]     => paid, intent pi_enc_50_60, 10c, remaining 440
```
The sequence above is the exact call order in `deploy-contracts.ts`. The
caller_key is host-bound (`did:bound:<tid hex>`) — the client-supplied `caller`
string is ignored on the session path, so identity can't be spoofed. The
per-call cap (50c) is enforced on `check-gate`, `enter-gateway`, AND
`pay-for-service`, so the 70c and 400c calls are denied.

### Quota counter — per-caller hard-stop + reset
```
consume(app1, limit=5, x3)  => used 2, 4, exceed@6 (stays 4)
check(app1)                 => used 4 / 5, reset_epoch_secs present
consume(app2, limit=3, x4)  => used 1, 2, 3 (at_limit), exceed@4 (stays 3)
check(app2)                 => used 3 / 3, remaining 0
reset(app2)                 => used 0 / 3   ← hard-stop + reset verified live
```
The hard-stop at `limit` is non-trivial: an exceeded consume refuses to write,
so the stored counter never overshoots. Keys are namespaced by the bound
caller DID (`u:<tid-hex>:<key>`); a first-touch caller's requested limit is
clamped by the tenant's ceiling (default 100_000).

Logs flushed from inside the enclave via `logging.info` read back through
`tenant.contracts.logs(tail)`.

## Key design facts

- Contracts read / write `z:<tid>:` KV maps (`gate`, `quotas`, `secrets`).
- Stripe key + session budget live in KV, never in WASM globals.
- The pay-flow mints a deterministic `pi_enc_...` ref locally (no outbound
  network in the reference path). A live Stripe `POST /v1/payment_intents`
  via `http-with-placeholders` is the documented next step — the shipped WIT
  deliberately prunes the `http` host interfaces so the byte budget stays
  under the 1 MiB `max_wasm_bytes` cap (BUGS.md #6).

## Bounty deliverables

- **Agent ID claimed** — DID `did:t3n:5db3681df85b9a698777a5aa60331...b5dc`
- **Free token proof** — SDK paths broken on this cluster; see BUGS.md #1 for
  the repro + CLI fallback.
- **Deployed TEE contract** — both contracts above, live on testnet.
- **Bonus** — both paywall (agent-paywall) + quota counter.
- **Findings** — `BUGS.md`.
- **Google Doc** — link in the bounty submission sheet.
- **Screenshots** — `screenshots/`.

## License

MIT (contract crates mirror the `z-tenant-flight` repo license).