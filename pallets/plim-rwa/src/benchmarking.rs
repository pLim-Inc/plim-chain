//! Benchmarking skeleton for `pallet-rwa`.
//!
//! Stubs only — every benchmark performs the minimal call shape needed for
//! the macro to compile. Concrete benchmarked weights will replace the
//! placeholders in `weights.rs` once a production runtime exists.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Rwa;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_core::H256;

#[benchmarks(where BalanceOf<T>: From<u128>)]
mod benchmarks {
	use super::*;

	fn dummy_asset<T: Config>(manager: T::AccountId) -> RwaAsset<T>
	where
		BalanceOf<T>: From<u128>,
	{
		let symbol: BoundedVec<u8, frame_support::pallet_prelude::ConstU32<16>> =
			vec![b'P', b'L', b'I', b'M', b'-', b'X'].try_into().unwrap();
		let name: BoundedVec<u8, frame_support::pallet_prelude::ConstU32<128>> =
			vec![b'X'].try_into().unwrap();
		RwaAsset::<T> {
			symbol,
			name,
			description_hash: H256::zero(),
			total_supply: 1_000_000u128.into(),
			kyc_required: KycLevel::None,
			yield_currency: Currency::Plim,
			nav_currency: Currency::Plim,
			jurisdiction: *b"CH",
			created_at_block: 0u32.into(),
			manager,
		}
	}

	#[benchmark]
	fn register_asset()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		let asset = dummy_asset::<T>(manager);
		#[extrinsic_call]
		_(RawOrigin::Root, asset);
	}

	#[benchmark]
	fn mint_shares()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		let to: T::AccountId = whitelisted_caller();
		let amount: BalanceOf<T> = 1u128.into();
		let proof = PaymentProof { payer: manager.clone(), amount, proof_hash: H256::zero() };
		// Pre-install asset directly.
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(manager.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		#[extrinsic_call]
		_(RawOrigin::Signed(manager), <T as Config>::RwaAssetId::from(0u32), to, amount, proof);
	}

	#[benchmark]
	fn burn_shares()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		let one: BalanceOf<T> = 1u128.into();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(who.clone()));
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), who.clone(), one);
		TotalIssued::<T>::insert(<T as Config>::RwaAssetId::from(0u32), one);
		#[extrinsic_call]
		_(RawOrigin::Signed(who), <T as Config>::RwaAssetId::from(0u32), one);
	}

	#[benchmark]
	fn transfer_shares()
	where
		BalanceOf<T>: From<u128>,
	{
		let from: T::AccountId = whitelisted_caller();
		let to: T::AccountId = whitelisted_caller();
		let ten: BalanceOf<T> = 10u128.into();
		let one: BalanceOf<T> = 1u128.into();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(from.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), from.clone(), ten);
		#[extrinsic_call]
		_(RawOrigin::Signed(from), <T as Config>::RwaAssetId::from(0u32), to, one);
	}

	#[benchmark]
	fn distribute_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		let one: BalanceOf<T> = 1u128.into();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(manager.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), manager.clone(), one);
		TotalIssued::<T>::insert(<T as Config>::RwaAssetId::from(0u32), one);
		#[extrinsic_call]
		_(RawOrigin::Signed(manager), <T as Config>::RwaAssetId::from(0u32), one, Currency::Plim, H256::zero());
	}

	#[benchmark]
	fn claim_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		_(RawOrigin::Signed(who), <T as Config>::RwaAssetId::from(0u32), <T as Config>::DistributionId::default());
	}

	#[benchmark]
	fn claim_all_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(who.clone()));
		#[extrinsic_call]
		_(RawOrigin::Signed(who), <T as Config>::RwaAssetId::from(0u32));
	}

	#[benchmark]
	fn freeze()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(manager));
		#[extrinsic_call]
		_(RawOrigin::Root, <T as Config>::RwaAssetId::from(0u32));
	}

	#[benchmark]
	fn unfreeze()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(manager));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Frozen);
		#[extrinsic_call]
		_(RawOrigin::Root, <T as Config>::RwaAssetId::from(0u32));
	}

	#[benchmark]
	fn wind_down()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(<T as Config>::RwaAssetId::from(0u32), dummy_asset::<T>(manager));
		#[extrinsic_call]
		_(RawOrigin::Root, <T as Config>::RwaAssetId::from(0u32));
	}

	impl_benchmark_test_suite!(Rwa, crate::mock::new_test_ext(), crate::mock::Test);
}
