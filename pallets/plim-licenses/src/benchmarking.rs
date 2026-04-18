//! Benchmarking setup for pallet-plim-licenses

use super::*;

#[allow(unused)]
use crate::Pallet as PlimLicenses;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use alloc::vec;
use alloc::vec::Vec;
use types::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_license_template() {
		let caller: T::AccountId = whitelisted_caller();
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![1, 2, 3];

		#[extrinsic_call]
		create_license_template(
			RawOrigin::Signed(caller),
			LicenseType::Personal,
			1u16,
			2999u32,
			100_000u128,
			true,
			false,
			false,
			false,
			true,
			false,
			false,
			500u16,
			None,
			Some(10u32),
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			proof,
		);
	}

	#[benchmark]
	fn mint_license() {
		let owner: T::AccountId = whitelisted_caller();
		let creator: T::AccountId = whitelisted_caller();
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![1, 2, 3];

		#[extrinsic_call]
		_(
			RawOrigin::Root,
			owner,
			LicenseType::Personal,
			1u16,
			creator,
			2999u32,
			100_000u128,
			true,
			false,
			false,
			false,
			true,
			false,
			false,
			500u16,
			None,
			Some(10u32),
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			proof,
			None,
		);

		assert_eq!(pallet::NextItemId::<T>::get(), 1);
	}

	#[benchmark]
	fn update_license_attrs() {
		let caller: T::AccountId = whitelisted_caller();
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![];

		// Create a template first.
		PlimLicenses::<T>::create_license_template(
			RawOrigin::Signed(caller.clone()).into(),
			LicenseType::Commercial,
			1u16,
			0u32,
			0u128,
			true,
			true,
			true,
			false,
			false,
			false,
			false,
			500u16,
			None,
			None,
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			proof,
		)
		.expect("template creation should succeed");

		// Find the template hash (there's exactly one in storage).
		let hash = pallet::LicenseTemplates::<T>::iter_keys().next().expect("template exists");

		#[extrinsic_call]
		update_license_attrs(
			RawOrigin::Signed(caller),
			hash,
			Some(1000u16),
			None,
			None,
			None,
			None,
			None,
		);
	}

	#[benchmark]
	fn revoke_license() {
		let owner: T::AccountId = whitelisted_caller();
		let creator: T::AccountId = whitelisted_caller();
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![];

		PlimLicenses::<T>::mint_license(
			RawOrigin::Root.into(),
			owner,
			LicenseType::Personal,
			1u16,
			creator,
			0u32,
			0u128,
			false,
			false,
			false,
			false,
			false,
			false,
			false,
			0u16,
			None,
			None,
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			proof,
			None,
		)
		.expect("mint should succeed");

		#[extrinsic_call]
		revoke_license(RawOrigin::Root, 0u32);

		assert!(pallet::Licenses::<T>::get(0u32).is_none());
	}

	#[benchmark]
	fn claim_custody_license() {
		let custodian: T::AccountId = whitelisted_caller();
		let creator: T::AccountId = whitelisted_caller();
		let email_hash = [0xABu8; 32];
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![];

		PlimLicenses::<T>::mint_license(
			RawOrigin::Root.into(),
			custodian.clone(),
			LicenseType::Personal,
			1u16,
			creator,
			0u32,
			0u128,
			true,
			false,
			false,
			false,
			false,
			false,
			false,
			500u16,
			None,
			None,
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::Custody,
			proof,
			Some(email_hash),
		)
		.expect("mint should succeed");

		let claimer: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		claim_custody_license(RawOrigin::Signed(claimer), 0u32, 0u64);
	}

	#[benchmark]
	fn burn_license() {
		let owner: T::AccountId = whitelisted_caller();
		let creator: T::AccountId = whitelisted_caller();
		let geo: Vec<[u8; 2]> = vec![];
		let platforms: Vec<Vec<u8>> = vec![];
		let proof: Vec<u8> = vec![];

		PlimLicenses::<T>::mint_license(
			RawOrigin::Root.into(),
			owner.clone(),
			LicenseType::Personal,
			1u16,
			creator,
			0u32,
			0u128,
			false,
			false,
			false,
			false,
			false,
			false,
			false,
			0u16,
			None,
			None,
			None,
			geo,
			platforms,
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			proof,
			None,
		)
		.expect("mint should succeed");

		#[extrinsic_call]
		burn_license(RawOrigin::Signed(owner), 0u32);

		assert!(pallet::Licenses::<T>::get(0u32).is_none());
	}

	#[benchmark]
	fn set_creator_config() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		set_creator_config(
			RawOrigin::Signed(caller.clone()),
			1000u16,
			caller.clone(),
			RoyaltyAsset::Native,
		);
	}

	impl_benchmark_test_suite!(PlimLicenses, crate::mock::new_test_ext(), crate::mock::Test);
}
