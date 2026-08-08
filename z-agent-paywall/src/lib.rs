//! z-agent-paywall v0.2.0 — paywalled agent gateway.
//!
//! Enforces a per-session spend budget AND a per-call cap BEFORE an agent
//! tool dispatch is granted. On approval it mints a Stripe PaymentIntent-style
//! intent locally (deterministic, no network in the reference path); a real
//! deployment swaps the KV spend for an http-with-placeholders POST to
//! api.stripe.com (card data only ever enters the host's placeholder
//! resolution — never WASM memory).
//!
//! Host capabilities required (manifest) — exactly the imports in `wit/`:
//! ```json
//! { "host_capabilities": ["kv_store", "logging", "tenant_context"] }
//! ```
//!
//! State layout — KV map `z:<tid>:gate`:
//!   key `spend:<caller>`  → u64 cents spent this session (binary LE)
//!   key `meta:budget`     → JSON `{ session_budget_cents, per_call_cap_cents }`
#![warn(clippy::style)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

pub const CONTRACT_VERSION: &str = "0.2.0";

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