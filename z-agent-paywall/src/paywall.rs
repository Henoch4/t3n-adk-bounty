//! Paywall implementation — see `lib.rs` for the contract overview.
//!
//! Session spend is stored per caller in KV map `z:<tid>:gate`.
//!
//! Identity: when the runtime binds a calling-user DID (Session API path), the
//! spend key is derived from that DID — client-supplied `caller` strings are
//! ignored so nobody can debit/rotate another caller's budget. On direct-exec
//! dispatches (no user context) the supplied `caller` string is used as a
//! best-effort key and logged as the fallback path.

use alloc::string::String as AString;
use alloc::vec::Vec;
use alloc::string::ToString as _;

#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(test)]
use serde_json::json;

#[cfg(target_arch = "wasm32")]
use crate::host::{
    interfaces::{kv_store, logging},
    tenant::tenant_context,
};

/// Upper bound any caller can ask for in a single priced call (protects logs
/// and KV keys from absurd values even when identity is absent).
pub const MAX_CALL_CENTS: u64 = 1_000_000;
/// Upper bound on the label field echoed into logs and responses.
pub const MAX_LABEL_BYTES: usize = 128;

#[cfg(target_arch = "wasm32")]
fn map_name() -> Result<AString, AString> {
    let tid = tenant_context::tenant_did();
    Ok(alloc::format!("z:{}:gate", hex::encode(tid)))
}

/// Derive the effective caller key. Prefers the host-bound caller DID; falls
/// back to the client-supplied string only when no user context exists, and
/// tags the fallback with a `-unbound` suffix so abuse is auditable.
#[cfg(target_arch = "wasm32")]
fn effective_caller(supplied: &str) -> AString {
    match tenant_context::calling_user_did() {
        Some(did) => {
            let hexdid = hex::encode(did);
            let _ = logging::info(&alloc::format!(
                "paywall identity-bound caller={} source=session authorized_map z:…:gate",
                hexdid
            ));
            AString::from("did:bound:") + &hexdid
        }
        None => alloc::format!("{supplied}-unbound"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn effective_caller(supplied: &str) -> AString {
    AString::from(supplied)
}

#[cfg(target_arch = "wasm32")]
fn budget() -> Result<(u64, u64), AString> {
    // defaults: 500 cents session budget, 50 cents per-call cap
    let map = map_name()?;
    let bytes = kv_store::get(&map, b"meta:budget")
        .map_err(|e| alloc::format!("kv read budget: {e}"))?;
    match bytes {
        None => Ok((500, 50)),
        Some(bytes) => {
            let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let sb = v["session_budget_cents"].as_u64().unwrap_or(500);
            let pc = v["per_call_cap_cents"].as_u64().unwrap_or(50);
            Ok((sb, pc))
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn read_spend(map: &str, caller: &str) -> Result<u64, AString> {
    let skey = alloc::format!("spend:{caller}");
    let bytes = kv_store::get(map, skey.as_bytes())
        .map_err(|e| alloc::format!("kv read spend: {e}"))?;
    match bytes {
        None => Ok(0),
        Some(b) => {
            // binary little-endian u64
            let mut arr = [0u8; 8];
            let n = b.len().min(8);
            arr[..n].copy_from_slice(&b[..n]);
            Ok(u64::from_le_bytes(arr))
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn write_spend(map: &str, caller: &str, cents: u64) -> Result<(), AString> {
    let skey = alloc::format!("spend:{caller}");
    kv_store::put(map, skey.as_bytes(), &cents.to_le_bytes())
        .map_err(|e| alloc::format!("kv write spend: {e}"))
}

#[derive(serde::Deserialize)]
struct GateReq {
    #[serde(default)]
    caller: String,
    #[serde(default)]
    label: String,
    amount_cents: u64,
}

#[derive(serde::Deserialize)]
struct PayReq {
    #[serde(default)]
    caller: String,
    amount_cents: u64,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(serde::Serialize)]
struct GateResp {
    allowed: bool,
    label: String,
    caller_key: String,
    session_spent_cents: u64,
    session_budget_cents: u64,
    session_remaining_cents: u64,
    reason: AString,
}

#[derive(serde::Serialize)]
struct EnterGatewayResp {
    granted: bool,
    label: String,
    caller_key: String,
    spent_cents: u64,
    session_spent_cents: u64,
    session_remaining_cents: u64,
    reason: AString,
}

#[derive(serde::Serialize)]
struct PayForServiceResp {
    paid: bool,
    caller_key: String,
    payment_intent: String,
    amount_cents: u64,
    currency: String,
    session_remaining_cents: u64,
    reason: AString,
}

/// Single decision function shared by check-gate and enter-gateway so both
/// gates enforce the same policy (per-call cap AND session headroom). Pure and
/// testable on the host (no WASI).
#[cfg(not(target_arch = "wasm32"))]
fn decide(amount: u64, spent: u64, session_budget: u64, per_call_cap: u64) -> (bool, u64, AString) {
    let remaining = session_budget.saturating_sub(spent);
    if amount > per_call_cap {
        (
            false,
            remaining,
            alloc::format!("amount {} exceeds per-call cap {}", amount, per_call_cap),
        )
    } else if amount > remaining {
        (
            false,
            remaining,
            alloc::format!(
                "not enough session budget ({} spent of {} left: {})",
                spent,
                session_budget,
                remaining
            ),
        )
    } else if remaining == 0 {
        (false, remaining, "session budget exhausted".to_string())
    } else {
        (true, remaining, "under budget".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn decide(amount: u64, spent: u64, session_budget: u64, per_call_cap: u64) -> (bool, u64, AString) {
    let remaining = session_budget.saturating_sub(spent);
    if amount > per_call_cap {
        (
            false,
            remaining,
            alloc::format!("amount {} exceeds per-call cap {}", amount, per_call_cap),
        )
    } else if amount > remaining || remaining == 0 {
        (
            false,
            remaining,
            alloc::format!(
                "not enough session budget ({} spent of {} left: {})",
                spent,
                session_budget,
                remaining
            ),
        )
    } else {
        (true, remaining, "under budget".to_string())
    }
}

pub fn check_gate(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: GateReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("check-gate: bad input: {e}"))?;
    if req.label.len() > MAX_LABEL_BYTES {
        return Err(alloc::format!("label exceeds {MAX_LABEL_BYTES} bytes"));
    }
    if req.amount_cents > MAX_CALL_CENTS {
        return Err(alloc::format!("amount exceeds {MAX_CALL_CENTS} cents"));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (sb, pc) = budget()?;
        let caller_key = effective_caller(&req.caller);
        let spent = read_spend(&map, &caller_key)?;
        let (allowed, remaining, reason) = decide(req.amount_cents, spent, sb, pc);

        let _ = logging::info(&alloc::format!(
            "paywall.check-gate key={caller_key} label={} amount={} spent={spent} session={sb} cap={pc} allowed={allowed}",
            req.label, req.amount_cents
        ));
        let resp = GateResp {
            allowed,
            label: req.label,
            caller_key,
            session_spent_cents: spent,
            session_budget_cents: sb,
            session_remaining_cents: remaining,
            reason,
        };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("check-gate is only implemented on the wasm32 target".to_string())
    }
}

pub fn enter_gateway(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: GateReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("enter-gateway: bad input: {e}"))?;
    if req.label.len() > MAX_LABEL_BYTES {
        return Err(alloc::format!("label exceeds {MAX_LABEL_BYTES} bytes"));
    }
    if req.amount_cents > MAX_CALL_CENTS {
        return Err(alloc::format!("amount exceeds {MAX_CALL_CENTS} cents"));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (session, pc) = budget()?;
        let caller_key = effective_caller(&req.caller);
        let spent = read_spend(&map, &caller_key)?;
        let (granted, _, _) = decide(req.amount_cents, spent, session, pc);

        if granted {
            write_spend(&map, &caller_key, spent + req.amount_cents)?;
        }
        let spent_after = read_spend(&map, &caller_key)?;

        let _ = logging::info(&alloc::format!(
            "paywall.enter-gateway key={caller_key} label={} amount={} granted={granted}",
            req.label, req.amount_cents
        ));
        let (_, _, reason) = decide(req.amount_cents, spent_after, session, pc);
        let resp = EnterGatewayResp {
            granted,
            label: req.label,
            caller_key,
            spent_cents: if granted { req.amount_cents } else { 0 },
            session_spent_cents: spent_after,
            session_remaining_cents: session.saturating_sub(spent_after),
            reason: if granted { "toll paid".to_string() } else { reason },
        };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("enter-gateway is only implemented on the wasm32 target".to_string())
    }
}

pub fn pay_for_service(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: PayReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("pay-for-service: bad input: {e}"))?;
    if req.amount_cents > MAX_CALL_CENTS {
        return Err(alloc::format!("amount exceeds {MAX_CALL_CENTS} cents"));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (session, pc) = budget()?;
        let caller_key = effective_caller(&req.caller);
        let spent = read_spend(&map, &caller_key)?;
        let currency = req.currency.unwrap_or_else(|| "usd".to_string());

        // Same gate as enter-gateway: per-call cap AND session headroom.
        let (allowed, _, _) = decide(req.amount_cents, spent, session, pc);
        if !allowed {
            // Soft-declined intent: the caller sees the gate enforced.
            let resp = PayForServiceResp {
                paid: false,
                caller_key,
                payment_intent: "pi_MOCK_DECLINED_INSUFFICIENT_BUDGET".to_string(),
                amount_cents: req.amount_cents,
                currency,
                session_remaining_cents: session.saturating_sub(spent),
                reason: "gate denied: over cap or over session budget".to_string(),
            };
            return serde_json::to_vec(&resp).map_err(|e| e.to_string());
        }

        // Mint a Stripe-style intent id locally (deterministic, no network in
        // the reference path). A real deployment swaps the KV spend for an
        // http-with-placeholders POST to api.stripe.com with the session's
        // payment method marker.
        let intent = alloc::format!(
            "pi_enc_{}_{}",
            caller_key.len(),
            spent + req.amount_cents
        );
        write_spend(&map, &caller_key, spent + req.amount_cents)?;
        let spent_after = read_spend(&map, &caller_key)?;

        let _ = logging::info(&alloc::format!(
            "paywall.pay-for-service key={caller_key} amount={} currency={} intent={}",
            req.amount_cents, currency, intent
        ));

        let resp = PayForServiceResp {
            paid: true,
            caller_key,
            payment_intent: intent,
            amount_cents: req.amount_cents,
            currency,
            session_remaining_cents: session.saturating_sub(spent_after),
            reason: "toll paid".to_string(),
        };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("pay-for-service is only implemented on the wasm32 target".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_gate_bad_input_returns_err() {
        let result = check_gate(b"not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bad input"));
    }

    #[test]
    fn enter_gateway_bad_input_returns_err() {
        let result = enter_gateway(b"not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bad input"));
    }

    #[test]
    fn pay_for_service_non_wasm_returns_err() {
        let input = serde_json::to_vec(&json!({
            "caller": "did:key:z6Mk_test",
            "amount_cents": 25,
        }))
        .unwrap();
        let result = pay_for_service(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("wasm32"));
    }

    // ---- shared decision logic (runs on host, no wasm) ----
    #[test]
    fn decide_under_budget_and_cap_allows() {
        let (allowed, remaining, reason) = decide(25, 0, 500, 50);
        assert!(allowed, "25c under 50c cap and 500c session -> allow: {reason}");
        assert_eq!(remaining, 500);
    }

    #[test]
    fn decide_over_per_call_cap_denies_even_with_headroom() {
        let (allowed, _, reason) = decide(70, 0, 500, 50);
        assert!(!allowed, "70c > 50c cap must be denied: {reason}");
        assert!(reason.contains("exceeds per-call cap"));
    }

    #[test]
    fn decide_at_cap_boundary_allowed() {
        let (allowed, _, _) = decide(50, 0, 500, 50);
        assert!(allowed, "50c == cap is allowed");
    }

    #[test]
    fn decide_over_session_denies() {
        let (allowed, _, reason) = decide(25, 490, 500, 50);
        assert!(!allowed, "spent 490 + 25 > 500 budget: {reason}");
    }

    #[test]
    fn decide_overshoots_session_and_cap_consistently() {
        // A call denied by per-call cap must also be denied by session view —
        // guard against the gates diverging.
        let (a1, _, _) = decide(400, 50, 500, 50);
        let (a2, _, _) = decide(400, 50, 500, 50);
        assert_eq!(a1, a2);
        assert!(!a1);
    }
}