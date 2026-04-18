//! Benchmarking setup for pallet-plim-oracle.
//!
//! These are skeleton benchmarks (one per extrinsic) sufficient to satisfy the
//! `runtime-benchmarks` feature gate. Real weight curves will be derived once
//! the runtime integrates this pallet and exercises it under load.

use super::*;
use crate::pallet::{Pallet as PlimOracle, Updaters, Quorum};
use crate::types::AssetPair;
use frame_benchmarking::v2::*;
use frame_support::BoundedBTreeSet;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn propose_rate() {
		let caller: T::AccountId = whitelisted_caller();
		let mut set: BoundedBTreeSet<T::AccountId, T::MaxUpdaters> = BoundedBTreeSet::new();
		let _ = set.try_insert(caller.clone());
		Updaters::<T>::put(set);
		Quorum::<T>::put(1);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), AssetPair::PlimEur, 1_000_000u64);
	}

	#[benchmark]
	fn add_updater() {
		let new_updater: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		_(RawOrigin::Root, new_updater);
	}

	#[benchmark]
	fn remove_updater() {
		let updater: T::AccountId = whitelisted_caller();
		let mut set: BoundedBTreeSet<T::AccountId, T::MaxUpdaters> = BoundedBTreeSet::new();
		let _ = set.try_insert(updater.clone());
		Updaters::<T>::put(set);
		#[extrinsic_call]
		_(RawOrigin::Root, updater);
	}

	#[benchmark]
	fn set_quorum() {
		let updater: T::AccountId = whitelisted_caller();
		let mut set: BoundedBTreeSet<T::AccountId, T::MaxUpdaters> = BoundedBTreeSet::new();
		let _ = set.try_insert(updater);
		Updaters::<T>::put(set);
		#[extrinsic_call]
		_(RawOrigin::Root, 1u32);
	}

	impl_benchmark_test_suite!(PlimOracle, crate::mock::new_test_ext(), crate::mock::Test);
}
