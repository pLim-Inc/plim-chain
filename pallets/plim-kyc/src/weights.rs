//! Placeholder weights for pallet-plim-kyc.
//!
//! These will be replaced by benchmarked values once `frame-benchmarking` runs
//! against the production runtime.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::weights::Weight;
use core::marker::PhantomData;

/// Weight functions needed for pallet-plim-kyc.
pub trait WeightInfo {
	fn set_kyc() -> Weight;
	fn revoke_kyc() -> Weight;
	fn add_attestor() -> Weight;
	fn remove_attestor() -> Weight;
	fn add_to_sanction_list() -> Weight;
	fn remove_from_sanction_list() -> Weight;
}

/// Placeholder weights for production use.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_kyc() -> Weight { Weight::from_parts(10_000, 0) }
	fn revoke_kyc() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_attestor() -> Weight { Weight::from_parts(10_000, 0) }
	fn remove_attestor() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_to_sanction_list() -> Weight { Weight::from_parts(10_000, 0) }
	fn remove_from_sanction_list() -> Weight { Weight::from_parts(10_000, 0) }
}

/// Default weight implementation for tests and backwards compatibility.
impl WeightInfo for () {
	fn set_kyc() -> Weight { Weight::from_parts(10_000, 0) }
	fn revoke_kyc() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_attestor() -> Weight { Weight::from_parts(10_000, 0) }
	fn remove_attestor() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_to_sanction_list() -> Weight { Weight::from_parts(10_000, 0) }
	fn remove_from_sanction_list() -> Weight { Weight::from_parts(10_000, 0) }
}
