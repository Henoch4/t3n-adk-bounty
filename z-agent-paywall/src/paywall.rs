//! Paywall implementation — see `lib.rs` for the contract overview.
//!
//! Session spend is stored per caller in KV map `z:<tid>:gate`.

use alloc::string::String as AString;
use alloc::vec::Vec;
use alloc::string::ToString as _;
use serde_json::{json, Value};

#[cfg(target_arch = "wasm32")]
use crate::host::{
    interfaces::{kv_store, logging},
    tenant::tenant_context,
};

#[cfg(target_arch = "wasm32")]
fn map_name() -> Result<AString, AString> {
    let tid = tenant_context::tenant_did();
    Ok(alloc::format!("z:{}:gate", hex::encode(tid)))
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
    caller: String,
    label: String,
    amount_cents: u64,
}

#[derive(serde::Deserialize)]
struct PayReq {
    caller: String,
    amount_cents: u64,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(serde::Serialize)]
struct GateResp {
    allowed: bool,
    label: String,
    session_spent_cents: u64,
    session_budget_cents: u64,
    session_remaining_cents: u64,
    reason: AString,
}

#[derive(serde::Serialize)]
struct EnterGatewayResp {
    granted: bool,
    label: String,
    spent_cents: u64,
    session_spent_cents: u64,
    session_remaining_cents: u64,
    reason: AString,
}

#[derive(serde::Serialize)]
struct PayForServiceResp {
    paid: bool,
    payment_intent: String,
    amount_cents: u64,
    currency: String,
    session_remaining_cents: u64,
}

pub fn check_gate(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: GateReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("check-gate: bad input: {e}"))?;

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (sb, pc) = budget()?;
        let spent = read_spend(&map, &req.caller)?;
        let remaining = sb.saturating_sub(spent);
        let allowed = req.amount_cents <= pc && req.amount_cents <= remaining && remaining > 0;

        let _ = logging::info(&alloc::format!(
            "paywall.check-gate caller={} label={} amount={} spent={spent} session={sb} cap={pc} allowed={allowed}",
            req.caller, req.label, req.amount_cents
        ));
        let reason = if !allowed {
            if req.amount_cents > pc {
                alloc::format!("amount {} exceeds per-call cap {}", req.amount_cents, pc)
            } else if remaining == 0 {
                "session budget exhausted".to_string()
            } else {
                alloc::format!(
                    "not enough session budget ({} spent of {} left: {})",
                    spent,
                    sb,
                    sb.saturating_sub(spent)
                )
            }
        } else {
            "under budget".to_string()
        };
        let resp = GateResp {
            allowed,
            label: req.label,
            session_spent_cents: spent,
            session_budget_cents: sb,
            session_remaining_cents: sb.saturating_sub(spent),
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

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (session, _) = budget()?;
        let spent = read_spend(&map, &req.caller)?;
        let proceeds = session.saturating_sub(spent);
        let granted = proceeds >= req.amount_cents;

        if granted {
            write_spend(&map, &req.caller, spent + req.amount_cents)?;
        }
        let spent_after = read_spend(&map, &req.caller)?;

        let _ = logging::info(&alloc::format!(
            "paywall.enter-gateway caller={} label={} amount={} granted={granted}",
            req.caller, req.label, req.amount_cents
        ));
        let resp = EnterGatewayResp {
            granted,
            label: req.label,
            spent_cents: if granted { req.amount_cents } else { 0 },
            session_spent_cents: spent_after,
            session_remaining_cents: session.saturating_sub(spent_after),
            reason: if granted {
                "toll paid".to_string()
            } else {
                "insufficient session budget".to_string()
            },
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

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (session, _) = budget()?;
        let spent = read_spend(&map, &req.caller)?;
        let proceeds = session.saturating_sub(spent);
        let currency = req.currency.unwrap_or_else(|| "usd".to_string());

        if proceeds < req.amount_cents {
            // Soft-declined intent: the caller sees the gate enforced.
            let resp = PayForServiceResp {
                paid: false,
                payment_intent: "pi_MOCK_DECLINED_INSUFFICIENT_BUDGET".to_string(),
                amount_cents: req.amount_cents,
                currency,
                session_remaining_cents: proceeds,
            };
            return serde_json::to_vec(&resp).map_err(|e| e.to_string());
        }

        // Mint a Stripe-style intent id locally (deterministic, no network in
        // the reference path). A real deployment swaps the KV spend for an
        // http-with-placeholders POST to api.stripe.com with the session's
        // payment method marker.
        let intent = alloc::format!(
            "pi_enc_{}_{}",
            req.caller.len(),
            spent + req.amount_cents
        );
        write_spend(&map, &req.caller, spent + req.amount_cents)?;
        let spent_after = read_spend(&map, &req.caller)?;

        let _ = logging::info(&alloc::format!(
            "paywall.pay-for-service caller={} amount={} currency={} intent={}",
            req.caller, req.amount_cents, currency, intent
        ));

        let resp = PayForServiceResp {
            paid: true,
            payment_intent: intent,
            amount_cents: req.amount_cents,
            currency,
            session_remaining_cents: session.saturating_sub(spent_after),
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
}