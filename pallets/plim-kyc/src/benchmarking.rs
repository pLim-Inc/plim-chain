//! Benchmarking setup for pallet-plim-kyc.
//!
//! Skeleton stubs only — these do not exercise full storage paths and exist
//! to satisfy the `runtime-benchmarks` feature gate. Real benchmarks should
//! be authored before mainnet activation.

use super::*;

#[allow(unused)]
use crate::Pallet as PlimKyc;
use crate::types::{KycRecord, KycLevel, SanctionReason};
use frame_benchmarking::v2::*;
use frame_support::BoundedVec;
use frame_support::pallet_prelude::ConstU32;
use frame_system::RawOrigin;
use sp_core::H256;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_kyc() {
		let attestor: T::AccountId = whitelisted_caller();
		// Insert into attestor set via root.
		PlimKyc::<T>::add_attestor(RawOrigin::Root.into(), attestor.clone())
			.expect("add_attestor succeeds");
		let subject: T::AccountId = account("subject", 0, 0);

		let now = <frame_system::Pallet<T>>::block_number();
		let expires_at = now + 1_000u32.into();

		let record = KycRecord::<T> {
			level: KycLevel::Basic,
			attested_by: attestor.clone(),
			attested_at: now,
			expires_at,
			document_hash: H256::repeat_byte(0xAB),
			country_code: *b"CH",
		};

		#[extrinsic_call]
		set_kyc(RawOrigin::Signed(attestor), subject, record);
	}

	#[benchmark]
	fn revoke_kyc() {
		let attestor: T::AccountId = whitelisted_caller();
		PlimKyc::<T>::add_attestor(RawOrigin::Root.into(), attestor.clone())
			.expect("add_attestor succeeds");
		let subject: T::AccountId = account("subject", 0, 0);

		let now = <frame_system::Pallet<T>>::block_number();
		let expires_at = now + 1_000u32.into();
		let record = KycRecord::<T> {
			level: KycLevel::Basic,
			attested_by: attestor.clone(),
			attested_at: now,
			expires_at,
			document_hash: H256::repeat_byte(0xAB),
			country_code: *b"CH",
		};
		PlimKyc::<T>::set_kyc(
			RawOrigin::Signed(attestor.clone()).into(),
			subject.clone(),
			record,
		)
		.expect("set_kyc succeeds");

		let reason: BoundedVec<u8, ConstU32<64>> =
			BoundedVec::try_from(b"benchmark".to_vec()).unwrap();

		#[extrinsic_call]
		revoke_kyc(RawOrigin::Signed(attestor), subject, reason);
	}

	#[benchmark]
	fn add_attestor() {
		let attestor: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		add_attestor(RawOrigin::Root, attestor);
	}

	#[benchmark]
	fn remove_attestor() {
		let attestor: T::AccountId = whitelisted_caller();
		PlimKyc::<T>::add_attestor(RawOrigin::Root.into(), attestor.clone())
			.expect("add_attestor succeeds");

		#[extrinsic_call]
		remove_attestor(RawOrigin::Root, attestor);
	}

	#[benchmark]
	fn add_to_sanction_list() {
		let subject: T::AccountId = account("subject", 0, 0);

		#[extrinsic_call]
		add_to_sanction_list(RawOrigin::Root, subject, SanctionReason::OfacSdn);
	}

	#[benchmark]
	fn remove_from_sanction_list() {
		let subject: T::AccountId = account("subject", 0, 0);
		PlimKyc::<T>::add_to_sanction_list(
			RawOrigin::Root.into(),
			subject.clone(),
			SanctionReason::OfacSdn,
		)
		.expect("add_to_sanction_list succeeds");

		#[extrinsic_call]
		remove_from_sanction_list(RawOrigin::Root, subject);
	}

	impl_benchmark_test_suite!(PlimKyc, crate::mock::new_test_ext(), crate::mock::Test);
}
