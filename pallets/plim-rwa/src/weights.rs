//! Placeholder weights for `pallet-rwa`.
//!
//! All stubs return `Weight::from_parts(10_000, 0)`. These will be replaced by
//! benchmarked values once `frame-benchmarking` runs against the production
//! runtime.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::weights::Weight;
use core::marker::PhantomData;

/// Weight functions needed for `pallet-rwa`.
pub trait WeightInfo {
	fn register_asset() -> Weight;
	fn mint_shares() -> Weight;
	fn burn_shares() -> Weight;
	fn transfer_shares() -> Weight;
	fn distribute_yield() -> Weight;
	fn claim_yield() -> Weight;
	fn claim_all_yield() -> Weight;
	fn freeze() -> Weight;
	fn unfreeze() -> Weight;
	fn wind_down() -> Weight;
}

/// Placeholder implementation for production use.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn register_asset() -> Weight { Weight::from_parts(10_000, 0) }
	fn mint_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn burn_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn transfer_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn distribute_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn claim_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn claim_all_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn freeze() -> Weight { Weight::from_parts(10_000, 0) }
	fn unfreeze() -> Weight { Weight::from_parts(10_000, 0) }
	fn wind_down() -> Weight { Weight::from_parts(10_000, 0) }
}

/// Default `()` implementation for tests and backwards compatibility.
impl WeightInfo for () {
	fn register_asset() -> Weight { Weight::from_parts(10_000, 0) }
	fn mint_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn burn_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn transfer_shares() -> Weight { Weight::from_parts(10_000, 0) }
	fn distribute_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn claim_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn claim_all_yield() -> Weight { Weight::from_parts(10_000, 0) }
	fn freeze() -> Weight { Weight::from_parts(10_000, 0) }
	fn unfreeze() -> Weight { Weight::from_parts(10_000, 0) }
	fn wind_down() -> Weight { Weight::from_parts(10_000, 0) }
}
