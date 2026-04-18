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

#[benchmarks]
mod benchmarks {
	use super::*;

	fn dummy_asset<T: Config>(manager: T::AccountId) -> RwaAsset<T>
	where
		BalanceOf<T>: From<u128>,
	{
		let symbol: BoundedVec<u8, frame_support::pallet_prelude::ConstU32<16>> =
			vec![bP, bL, bI, bM, b-, bX].try_into().unwrap();
		let name: BoundedVec<u8, frame_support::pallet_prelude::ConstU32<128>> =
			vec![bX].try_into().unwrap();
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
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(manager.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		#[extrinsic_call]
		_(RawOrigin::Signed(manager), 0u32.into(), to, amount, proof);
	}

	#[benchmark]
	fn burn_shares()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(who.clone()));
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), who.clone(), 1u128.into());
		TotalIssued::<T>::insert(<T as Config>::RwaAssetId::from(0u32), 1u128.into());
		#[extrinsic_call]
		_(RawOrigin::Signed(who), 0u32.into(), 1u128.into());
	}

	#[benchmark]
	fn transfer_shares()
	where
		BalanceOf<T>: From<u128>,
	{
		let from: T::AccountId = whitelisted_caller();
		let to: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(from.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), from.clone(), 10u128.into());
		#[extrinsic_call]
		_(RawOrigin::Signed(from), 0u32.into(), to, 1u128.into());
	}

	#[benchmark]
	fn distribute_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(manager.clone()));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Active);
		Shareholders::<T>::insert(<T as Config>::RwaAssetId::from(0u32), manager.clone(), 1u128.into());
		TotalIssued::<T>::insert(<T as Config>::RwaAssetId::from(0u32), 1u128.into());
		#[extrinsic_call]
		_(RawOrigin::Signed(manager), 0u32.into(), 1u128.into(), Currency::Plim, H256::zero());
	}

	#[benchmark]
	fn claim_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		_(RawOrigin::Signed(who), 0u32.into(), 0u64.into());
	}

	#[benchmark]
	fn claim_all_yield()
	where
		BalanceOf<T>: From<u128>,
	{
		let who: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(who.clone()));
		#[extrinsic_call]
		_(RawOrigin::Signed(who), 0u32.into());
	}

	#[benchmark]
	fn freeze()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(manager));
		#[extrinsic_call]
		_(RawOrigin::Root, 0u32.into());
	}

	#[benchmark]
	fn unfreeze()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(manager));
		AssetStatus::<T>::insert(<T as Config>::RwaAssetId::from(0u32), RwaStatus::Frozen);
		#[extrinsic_call]
		_(RawOrigin::Root, 0u32.into());
	}

	#[benchmark]
	fn wind_down()
	where
		BalanceOf<T>: From<u128>,
	{
		let manager: T::AccountId = whitelisted_caller();
		Assets::<T>::insert(0u32.into(), dummy_asset::<T>(manager));
		#[extrinsic_call]
		_(RawOrigin::Root, 0u32.into());
	}

	impl_benchmark_test_suite!(Rwa, crate::mock::new_test_ext(), crate::mock::Test);
}
