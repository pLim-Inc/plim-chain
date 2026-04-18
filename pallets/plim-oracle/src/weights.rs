//! Placeholder weights for pallet-plim-oracle.
//!
//! These will be replaced by benchmarked values once `frame-benchmarking` runs
//! against the production runtime.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::weights::Weight;
use core::marker::PhantomData;

/// Weight functions needed for pallet-plim-oracle.
pub trait WeightInfo {
	fn propose_rate() -> Weight;
	fn add_updater() -> Weight;
	fn remove_updater() -> Weight;
	fn set_quorum() -> Weight;
}

/// Placeholder weights for production use.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn propose_rate() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_updater() -> Weight  { Weight::from_parts(10_000, 0) }
	fn remove_updater() -> Weight { Weight::from_parts(10_000, 0) }
	fn set_quorum() -> Weight   { Weight::from_parts(10_000, 0) }
}

/// Stub impl for unit tests / mock runtimes.
impl WeightInfo for () {
	fn propose_rate() -> Weight { Weight::from_parts(10_000, 0) }
	fn add_updater() -> Weight  { Weight::from_parts(10_000, 0) }
	fn remove_updater() -> Weight { Weight::from_parts(10_000, 0) }
	fn set_quorum() -> Weight   { Weight::from_parts(10_000, 0) }
}
