//! z-quota-counter v0.1.0 — per-caller usage quota counter.
//!
//! Proves that a Rust→WASM TEE contract can enforce rate/quota policy using
//! the host `kv-store` interface: counters live in the tenant's KV map and
//! are read-modify-written inside the enclave on every invocation, so no
//! plaintext counter ever crosses the WASM boundary.
//!
//! Host capabilities required (manifest):
//! ```json
//! { "host_capabilities": ["kv_store", "logging", "tenant_context"] }
//! ```
//!
//! State layout — KV map `z:<tid>:quotas`:
//!   key `counter:<key>`     → JSON `{ used, limit }`
//!   key `meta:resets_at`    → ISO-8601 string (set once)
#![warn(clippy::style)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

pub const CONTRACT_VERSION: &str = "0.1.0";

wit_bindgen::generate!({
    world: "quota-counter",
    path: "wit",
    additional_derives: [
        serde::Deserialize,
        serde::Serialize,
    ],
    generate_all,
});

mod quota;

struct Component;

#[cfg(target_arch = "wasm32")]
impl exports::z::quota_counter::contracts::Guest for Component {
    fn check(
        req: exports::z::quota_counter::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("check: missing input")?;
        quota::check(&input)
    }

    fn consume(
        req: exports::z::quota_counter::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("consume: missing input")?;
        quota::consume(&input)
    }

    fn reset(
        req: exports::z::quota_counter::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("reset: missing input")?;
        quota::reset(&input)
    }
}

#[cfg(target_arch = "wasm32")]
export!(Component);

#[cfg(test)]
mod tests {
    use super::CONTRACT_VERSION;

    #[test]
    fn contract_version_is_semver() {
        let parts: Vec<&str> = CONTRACT_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for part in parts {
            assert!(part.parse::<u32>().is_ok());
        }
    }
}