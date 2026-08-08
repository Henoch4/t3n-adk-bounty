//! Quota counter implementation — see `lib.rs` for the contract overview.
//!
//! All counters live in KV map `z:<tid>:quotas` where `<tid>` is the raw
//! 20-byte tenant DID hex-encoded (same convention as `z:<tid>:secrets`).

use alloc::string::String as AString;
use alloc::vec::Vec;
use alloc::string::ToString as _;
use serde_json::{json, Value};

#[cfg(target_arch = "wasm32")]
use crate::host::{interfaces::{kv_store, logging}, tenant::tenant_context};

#[cfg(target_arch = "wasm32")]
fn map_name() -> Result<AString, AString> {
    let tid = tenant_context::tenant_did();
    Ok(alloc::format!("z:{}:quotas", hex::encode(tid)))
}

#[derive(serde::Deserialize)]
struct CheckReq {
    key: String,
}

#[derive(serde::Deserialize)]
struct ConsumeReq {
    key: String,
    limit: u32,
    #[serde(default)]
    amount: Option<u32>,
}

#[derive(serde::Deserialize)]
struct ResetReq {
    key: String,
}

#[derive(serde::Serialize)]
struct CounterState {
    key: String,
    quota_map: String,
    used: u64,
    limit: u64,
    remaining: i64,
    resets_at: &'static str,
}

#[derive(serde::Serialize)]
struct ConsumeResp {
    key: String,
    used: u64,
    limit: u64,
    remaining: i64,
    exceeded: bool,
    at_limit: bool,
}

#[derive(serde::Serialize)]
struct ResetResp {
    key: String,
    used: u64,
    limit: u64,
}

pub fn check(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: CheckReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("check: bad input: {e}"))?;

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (used, limit) = read_counter(&map, &req.key)?;
        let remaining = limit.saturating_sub(used) as i64;
        let _ = logging::info(&alloc::format!(
            "quota.check key={} used={used} limit={limit}",
            req.key
        ));
        let resp = CounterState {
            key: req.key,
            quota_map: map,
            used,
            limit,
            remaining,
            resets_at: "1970-01-01T00:00:00Z",
        };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("check is only implemented on the wasm32 target".to_string())
    }
}

pub fn consume(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: ConsumeReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("consume: bad input: {e}"))?;

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (mut used, stored_limit) = read_counter(&map, &req.key)?;
        // First touch: no stored row yet — adopt the requested limit.
        let limit = if stored_limit > 0 { stored_limit } else { req.limit as u64 };
        let amount = req.amount.unwrap_or(1).max(1) as u64;
        let exceeded = used.saturating_add(amount) > limit;
        if !exceeded {
            used += amount;
            write_counter(&map, &req.key, used, limit)?;
        }
        let remaining = limit.saturating_sub(used) as i64;
        let _ = logging::info(&alloc::format!(
            "quota.consume key={} used={used} limit={limit} amount={amount} exceeded={exceeded}",
            req.key
        ));
        let resp = ConsumeResp {
            key: req.key,
            used,
            limit,
            remaining,
            exceeded,
            at_limit: used >= limit,
        };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("consume is only implemented on the wasm32 target".to_string())
    }
}

pub fn reset(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: ResetReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("reset: bad input: {e}"))?;

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let (_, limit) = read_counter(&map, &req.key)?;
        write_counter(&map, &req.key, 0, limit)?;
        let _ = logging::info(&alloc::format!("quota.reset key={}", req.key));
        let resp = ResetResp { key: req.key, used: 0, limit };
        serde_json::to_vec(&resp).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("reset is only implemented on the wasm32 target".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn read_counter(map: &str, key: &str) -> Result<(u64, u64), AString> {
    let ckey = alloc::format!("counter:{key}");
    let bytes = kv_store::get(map, ckey.as_bytes())
        .map_err(|e| alloc::format!("kv read: {e}"))?;
    match bytes {
        None => Ok((0, 0)),
        Some(bytes) => {
            let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let used = v["used"].as_u64().unwrap_or(0);
            let limit = v["limit"].as_u64().unwrap_or(0);
            Ok((used, limit))
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn write_counter(map: &str, key: &str, used: u64, limit: u64) -> Result<(), AString> {
    let ckey = alloc::format!("counter:{key}");
    let value_json = serde_json::to_vec(&json!({ "used": used, "limit": limit }))
        .map_err(|e| e.to_string())?;
    kv_store::put(map, ckey.as_bytes(), &value_json)
        .map_err(|e| alloc::format!("kv write: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_bad_input_returns_err() {
        let result = check(b"not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bad input"));
    }

    #[test]
    fn consume_non_wasm_returns_err() {
        let input = serde_json::to_vec(&json!({"key": "did:test", "limit": 10})).unwrap();
        let result = consume(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("wasm32"));
    }
}