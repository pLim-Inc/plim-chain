//! Placeholder weights for pallet-plim-licenses.
//!
//! These will be replaced by benchmarked values once `frame-benchmarking` runs
//! against the production runtime.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet-plim-licenses.
pub trait WeightInfo {
	fn create_license_template() -> Weight;
	fn mint_license() -> Weight;
	fn update_license_attrs() -> Weight;
	fn revoke_license() -> Weight;
	fn claim_custody_license() -> Weight;
	fn burn_license() -> Weight;
	fn set_creator_config() -> Weight;
	fn sweep_expired(n: u32) -> Weight;
}

/// Placeholder weights for production use.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn create_license_template() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn mint_license() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	fn update_license_attrs() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn revoke_license() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn claim_custody_license() -> Weight {
		Weight::from_parts(18_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}

	fn burn_license() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn set_creator_config() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn sweep_expired(n: u32) -> Weight {
		Weight::from_parts(10_000_000_u64.saturating_add(1_000_000_u64.saturating_mul(n as u64)), 0)
			.saturating_add(T::DbWeight::get().reads(n as u64))
			.saturating_add(T::DbWeight::get().writes(n as u64))
	}
}

/// Default weight implementation for tests and backwards compatibility.
impl WeightInfo for () {
	fn create_license_template() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn mint_license() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn update_license_attrs() -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn revoke_license() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn claim_custody_license() -> Weight {
		Weight::from_parts(18_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}

	fn burn_license() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn set_creator_config() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}

	fn sweep_expired(n: u32) -> Weight {
		Weight::from_parts(10_000_000_u64.saturating_add(1_000_000_u64.saturating_mul(n as u64)), 0)
			.saturating_add(RocksDbWeight::get().reads(n as u64))
			.saturating_add(RocksDbWeight::get().writes(n as u64))
	}
}
