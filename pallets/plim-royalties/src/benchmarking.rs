//! Benchmarking setup for pallet-plim-royalties.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_platform_treasury() {
		let treasury: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Root, treasury.clone());

		assert_eq!(PlatformTreasury::<T>::get(), Some(treasury));
	}

	#[benchmark]
	fn update_platform_fee() {
		#[extrinsic_call]
		_(RawOrigin::Root, 500u16);

		assert_eq!(PlatformFeeBp::<T>::get(), 500u16);
	}

	#[benchmark]
	fn claim_accumulated_royalties() {
		let caller: T::AccountId = whitelisted_caller();
		AccumulatedRoyalties::<T>::insert(&caller, &RoyaltyCurrency::PLIM, BalanceOf::<T>::from(0u32));

		// Insert a small amount so the claim has work to do but will emit event-only
		// (PEUR off-chain settlement path, avoiding NativeCurrency deposit complexity).
		AccumulatedRoyalties::<T>::insert(
			&caller,
			&RoyaltyCurrency::PEUR,
			BalanceOf::<T>::from(100u32),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), RoyaltyCurrency::PEUR);

		assert_eq!(
			AccumulatedRoyalties::<T>::get(&caller, &RoyaltyCurrency::PEUR),
			BalanceOf::<T>::from(0u32),
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
