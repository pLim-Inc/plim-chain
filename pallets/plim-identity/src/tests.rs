//! Unit tests for `pallet-plim-identity`.
//!
//! Coverage:
//!   - Pre-existing identity registration / verify / revoke smoke tests.
//!   - L99 Workstream A (Task 8):
//!     * `set_device_attestation` rejects callers that don't own an identity.
//!     * `set_device_attestation` writes the hash on the owner's row.
//!     * `migration::v1_to_v2_device_attestation` defaults pre-L99
//!       identities to `device_attestation_hash = None`.

#![cfg(test)]

use crate::{
	mock::*, Error, Identities, IdentityInfo, IdentityInfoV1,
};
use frame_support::{assert_noop, assert_ok, traits::OnRuntimeUpgrade};

const OWNER: u64 = 1;
const STRANGER: u64 = 2;
const COUNTRY_USA: [u8; 3] = *b"USA";

#[test]
fn register_then_set_device_attestation_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(crate::Pallet::<Test>::register(
			frame_system::RawOrigin::Signed(OWNER).into(),
			b"Alice".to_vec(),
			COUNTRY_USA,
		));
		let hash = [0x9A; 32];
		assert_ok!(crate::Pallet::<Test>::set_device_attestation(
			frame_system::RawOrigin::Signed(OWNER).into(),
			hash,
		));
		let info = Identities::<Test>::get(OWNER).unwrap();
		assert_eq!(info.device_attestation_hash, Some(hash));
	});
}

#[test]
fn set_device_attestation_requires_existing_identity() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::set_device_attestation(
				frame_system::RawOrigin::Signed(STRANGER).into(),
				[0u8; 32],
			),
			Error::<Test>::NotRegistered
		);
	});
}

#[test]
fn migration_v1_to_v2_defaults_device_attestation_to_none() {
	new_test_ext().execute_with(|| {
		// Seed a V1-shaped identity row directly into storage (no
		// device_attestation_hash field yet) — translate() will rewrite as V2.
		use codec::Encode;
		use frame_support::storage::storage_prefix;
		use frame_support::Blake2_128Concat;
		use frame_support::StorageHasher;

		let v1 = IdentityInfoV1::<Test> {
			display_name: b"Bob".to_vec().try_into().unwrap(),
			country: *b"DEU",
			verified_at: None,
			verifier: None,
		};
		let pfx = storage_prefix(b"PlimIdentity", b"Identities");
		let mut full_key = pfx.to_vec();
		full_key.extend_from_slice(&Blake2_128Concat::hash(&OWNER.encode()));
		frame_support::storage::unhashed::put(&full_key, &v1);

		let _w =
			crate::migrations::v1_to_v2_device_attestation::Migration::<Test>::on_runtime_upgrade();

		let info: IdentityInfo<Test> = Identities::<Test>::get(OWNER).unwrap();
		assert_eq!(info.country, *b"DEU");
		assert_eq!(info.device_attestation_hash, None);
	});
}
