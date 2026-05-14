//! Unit tests for pallet-plim-payments.

#![cfg(test)]

use crate::{
	mock::*, Error, MandateInfo, MandateInfoV1, MandateRef, Mandates, PaymentOriginTransportCode,
};
use frame_support::{assert_noop, assert_ok, traits::OnRuntimeUpgrade};

const REF_A: MandateRef = [0xAA; 32];

#[test]
fn create_mandate_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::create_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			2,
			1, // asset_id
			1_000,
			100, // expires_at
			REF_A,
		));
		assert!(Mandates::<Test>::contains_key(REF_A));
	});
}

#[test]
fn cannot_create_duplicate_mandate() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::create_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			2,
			1,
			1_000,
			100,
			REF_A,
		));
		assert_noop!(
			crate::Pallet::<Test>::create_mandate(
				frame_system::RawOrigin::Signed(1).into(),
				2,
				1,
				1_000,
				100,
				REF_A,
			),
			Error::<Test>::MandateAlreadyExists
		);
	});
}

#[test]
fn create_mandate_defaults_origin_transport_to_https() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::create_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			2,
			1,
			1_000,
			100,
			REF_A,
		));
		let info = Mandates::<Test>::get(REF_A).unwrap();
		assert_eq!(info.payment_origin_transport_code, PaymentOriginTransportCode::Https);
	});
}

#[test]
fn set_mandate_origin_transport_only_by_payer() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::create_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			2,
			1,
			1_000,
			100,
			REF_A,
		));
		// Non-payer cannot retag.
		assert_noop!(
			crate::Pallet::<Test>::set_mandate_origin_transport(
				frame_system::RawOrigin::Signed(2).into(),
				REF_A,
				PaymentOriginTransportCode::Mesh,
			),
			Error::<Test>::NotMandateOwner
		);
		// Payer can.
		assert_ok!(crate::Pallet::<Test>::set_mandate_origin_transport(
			frame_system::RawOrigin::Signed(1).into(),
			REF_A,
			PaymentOriginTransportCode::Mesh,
		));
		let info = Mandates::<Test>::get(REF_A).unwrap();
		assert_eq!(info.payment_origin_transport_code, PaymentOriginTransportCode::Mesh);
	});
}

#[test]
fn set_mandate_origin_transport_unknown_mandate_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::set_mandate_origin_transport(
				frame_system::RawOrigin::Signed(1).into(),
				[0xCC; 32],
				PaymentOriginTransportCode::Mesh,
			),
			Error::<Test>::MandateNotFound
		);
	});
}

#[test]
fn migration_v1_to_v2_backfills_existing_mandates_with_https() {
	new_test_ext().execute_with(|| {
		// Simulate a pre-L99 storage row by writing a V1-shaped struct
		// directly via unhashed::put — translate() will then re-decode it
		// as MandateInfoV1 and rewrite as MandateInfo (V2) with Https.
		let v1 = MandateInfoV1::<Test> {
			payer: 1,
			payee: 2,
			asset_id: 1,
			allowance: 9_999,
			expires_at: 200,
		};
		let key = frame_support::storage::storage_prefix(b"PlimPayments", b"Mandates");
		// Compose Blake2_128Concat hash of the mandate_ref key.
		use codec::Encode;
		use frame_support::Blake2_128Concat;
		use frame_support::StorageHasher;
		let mut full_key = key.to_vec();
		full_key.extend_from_slice(&Blake2_128Concat::hash(&REF_A.encode()));
		frame_support::storage::unhashed::put(&full_key, &v1);

		// Run the migration.
		let _w =
			crate::migrations::v1_to_v2_origin_transport::Migration::<Test>::on_runtime_upgrade();

		// Every row now decodes as V2 with Https.
		let info: MandateInfo<Test> = Mandates::<Test>::get(REF_A).unwrap();
		assert_eq!(info.payer, 1);
		assert_eq!(info.payee, 2);
		assert_eq!(info.allowance, 9_999);
		assert_eq!(info.payment_origin_transport_code, PaymentOriginTransportCode::Https);
	});
}

#[test]
fn revoke_mandate_only_by_payer() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::create_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			2,
			1,
			1_000,
			100,
			REF_A,
		));
		// Non-payer cannot revoke.
		assert_noop!(
			crate::Pallet::<Test>::revoke_mandate(
				frame_system::RawOrigin::Signed(2).into(),
				REF_A
			),
			Error::<Test>::NotMandateOwner
		);
		// Payer can.
		assert_ok!(crate::Pallet::<Test>::revoke_mandate(
			frame_system::RawOrigin::Signed(1).into(),
			REF_A
		));
		assert!(!Mandates::<Test>::contains_key(REF_A));
	});
}
