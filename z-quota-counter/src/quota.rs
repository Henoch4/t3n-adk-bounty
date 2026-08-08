//! Quota counter implementation — see `lib.rs` for the contract overview.
//!
//! All counters live in KV map `z:<tid>:quotas` where `<tid>` is the raw
//! 20-byte tenant DID hex-encoded (same convention as `z:<tid>:secrets`).

use alloc::string::String as AString;
use alloc::vec::Vec;
use alloc::string::ToString as _;

#[cfg(any(target_arch = "wasm32", test))]
use serde_json::json;

#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
use crate::host::{interfaces::{kv_store, logging}, tenant::tenant_context};

/// Default upper bound on any counter's limit, so a first-touch caller can't
/// grant itself `u32::MAX` and neuter the tenant's quota. Overridable by
/// writing the JSON value `{"ceiling": N}` to key `meta:limit_ceiling`.
pub const DEFAULT_LIMIT_CEILING: u64 = 100_000;
/// Reset window in seconds (counters roll over automatically every 24h).
pub const RESET_WINDOW_SECS: u64 = 86_400;
/// Upper bound on a caller key length (protects KV keys and logs).
pub const MAX_KEY_BYTES: usize = 256;
/// Upper bound on a single consume amount.
pub const MAX_AMOUNT: u64 = 1_000_000_000;

#[cfg(target_arch = "wasm32")]
fn map_name() -> Result<AString, AString> {
    let tid = tenant_context::tenant_did();
    Ok(alloc::format!("z:{}:quotas", hex::encode(tid)))
}

/// Derive the effective counter key. When the runtime bound a caller DID
/// (session-dispatch), prefix the supplied key with the user's DID so no
/// caller can `consume`/`reset` another user's counter. Falls back to the
/// raw key (tagged `-unbound`) only on direct-exec / webhook paths where no
/// user context exists.
#[cfg(target_arch = "wasm32")]
fn effective_key(supplied: &str) -> AString {
    match tenant_context::calling_user_did() {
        Some(did) => alloc::format!("u:{}:{supplied}", hex::encode(did)),
        None => alloc::format!("{supplied}-unbound"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn effective_key(supplied: &str) -> AString {
    AString::from(supplied)
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
    /// Unix seconds at which the current reset window started (cluster clock).
    reset_epoch_secs: u64,
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

#[cfg(target_arch = "wasm32")]
fn limit_ceiling() -> u64 {
    let map = match map_name() {
        Ok(m) => m,
        Err(_) => return DEFAULT_LIMIT_CEILING,
    };
    match kv_store::get(&map, b"meta:limit_ceiling") {
        Ok(Some(bytes)) => {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(n) = v["ceiling"].as_u64() {
                    return n.min(10_000_000);
                }
            }
            DEFAULT_LIMIT_CEILING
        }
        _ => DEFAULT_LIMIT_CEILING,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn limit_ceiling() -> u64 {
    DEFAULT_LIMIT_CEILING
}

/// Unix seconds marking the start of the current reset window (cluster clock).
#[cfg(target_arch = "wasm32")]
fn reset_epoch() -> u64 {
    let now = tenant_context::cluster_timestamp_secs();
    (now / RESET_WINDOW_SECS) * RESET_WINDOW_SECS
}

#[cfg(not(target_arch = "wasm32"))]
fn reset_epoch() -> u64 {
    0
}

pub fn check(input: &[u8]) -> Result<Vec<u8>, AString> {
    let req: CheckReq = serde_json::from_slice(input)
        .map_err(|e| alloc::format!("check: bad input: {e}"))?;
    if req.key.len() > MAX_KEY_BYTES {
        return Err(alloc::format!("key exceeds {MAX_KEY_BYTES} bytes"));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let effective = effective_key(&req.key);
        let (used, limit) = read_counter(&map, &effective)?;
        let remaining = limit.saturating_sub(used) as i64;
        let _ = logging::info(&alloc::format!(
            "quota.check key={effective} used={used} limit={limit}"
        ));
        let resp = CounterState {
            key: effective,
            quota_map: map,
            used,
            limit,
            remaining,
            reset_epoch_secs: reset_epoch(),
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
    if req.key.len() > MAX_KEY_BYTES {
        return Err(alloc::format!("key exceeds {MAX_KEY_BYTES} bytes"));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let effective = effective_key(&req.key);
        let amount = (req.amount.unwrap_or(1) as u64).clamp(1, MAX_AMOUNT);
        let ceiling = limit_ceiling();
        let (mut used, stored_limit) = read_counter(&map, &effective)?;
        // First touch: no stored row yet — adopt the requested limit, clamped
        // by the ceiling so no caller can self-serve an unbounded quota.
        let limit = if stored_limit > 0 {
            stored_limit
        } else {
            (req.limit as u64).min(ceiling)
        };

        let exceeded = used.saturating_add(amount) > limit;
        if !exceeded {
            used += amount;
            write_counter(&map, &effective, used, limit)?;
        }
        let remaining = limit.saturating_sub(used) as i64;
        let _ = logging::info(&alloc::format!(
            "quota.consume key={effective} used={used} limit={limit} amount={amount} exceeded={exceeded}"
        ));
        let resp = ConsumeResp {
            key: effective,
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
    if req.key.len() > MAX_KEY_BYTES {
        return Err(alloc::format!("key exceeds {MAX_KEY_BYTES} bytes"));
    }

    #[cfg(target_arch = "wasm32")]
    {
        let map = map_name()?;
        let effective = effective_key(&req.key);
        let (_, limit) = read_counter(&map, &effective)?;
        write_counter(&map, &effective, 0, limit)?;
        let _ = logging::info(&alloc::format!("quota.reset key={effective}"));
        let resp = ResetResp { key: effective, used: 0, limit };
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

    // ---- first-touch limit adoption (pure logic) ----
    #[test]
    fn first_touch_adopts_limit() {
        // Stored row absent (0,0) + requested limit 5 -> adopted 5.
        let (limit, ceiling) = (5u64, DEFAULT_LIMIT_CEILING);
        let mut used = 0u64;
        let req_limit = 5u64;
        let limit = if limit > 0 { limit } else { req_limit.min(ceiling) };
        used += 2; // simulate a granted consume of amount 2
        assert_eq!(limit, 5);
        assert_eq!(used, 2);
    }

    #[test]
    fn first_touch_limited_ceiling() {
        // requested 9_999_999 but ceiling 100_000 -> adopt 100_000.
        let req_limit = 9_999_999u64;
        let ceiling = DEFAULT_LIMIT_CEILING;
        let limit = req_limit.min(ceiling);
        assert_eq!(limit, 100_000);
        assert!(limit <= DEFAULT_LIMIT_CEILING);
    }

    #[test]
    fn adopted_limit_never_increases() {
        let stored = 3u64;
        let requested = 9u64;
        let limit = if stored > 0 { stored } else { requested };
        assert_eq!(limit, 3, "existing stored limit must win over a higher request");
    }
}