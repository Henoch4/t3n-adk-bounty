//! z-agent-paywall v0.1.0 — paywalled agent gateway.
//!
//! Enforces a per-session spend budget BEFORE an agent tool dispatch is
//! granted, then emits a Stripe PaymentIntent-style payment via the host
//! `http-with-placeholders` interface (card data only ever enters the host's
//! placeholder resolution — never WASM memory).
//!
//! Host capabilities required (manifest):
//! ```json
//! { "host_capabilities": ["kv_store", "logging", "tenant_context", "http", "http_with_placeholders"] }
//! ```
//!
//! State layout — KV map `z:<tid>:gate`:
//!   key `spend:<caller>`  → u64 cents spent this session (binary LE)
//!   key `meta:budget`     → JSON `{ session_budget_cents, per_call_cap_cents }`
#![warn(clippy::style)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

pub const CONTRACT_VERSION: &str = "0.1.0";

wit_bindgen::generate!({
    world: "agent-paywall",
    path: "wit",
    additional_derives: [
        serde::Deserialize,
        serde::Serialize,
    ],
    generate_all,
});

mod paywall;

struct Component;

#[cfg(target_arch = "wasm32")]
impl exports::z::agent_paywall::contracts::Guest for Component {
    fn check_gate(
        req: exports::z::agent_paywall::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("check-gate: missing input")?;
        paywall::check_gate(&input)
    }

    fn enter_gateway(
        req: exports::z::agent_paywall::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("enter-gateway: missing input")?;
        paywall::enter_gateway(&input)
    }

    fn pay_for_service(
        req: exports::z::agent_paywall::contracts::GenericInput,
    ) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        let input = req.input.ok_or("pay-for-service: missing input")?;
        paywall::pay_for_service(&input)
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