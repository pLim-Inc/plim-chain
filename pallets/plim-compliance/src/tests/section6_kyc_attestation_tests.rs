//! §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23.
//!
//! Tests for the `register_kyc_attestation` extrinsic.
//!
//! TODO §6: needs `mock.rs` — this pallet currently ships no mock
//! runtime. These tests are stubbed against `crate::mock::*` and will
//! light up once the mock module is added in the §6 runtime PR.

#![cfg(test)]

// TODO §6: needs mock.rs — uncomment once available:
// use crate::mock::*;
// use crate::{Error, Event, KYCAttestations, KYCLevel, KYCStatus};
// use frame_support::{assert_noop, assert_ok, BoundedVec};
// use frame_system::RawOrigin;
// use sp_core::H256;

/// (a) Non-`ComplianceOrigin` callers must be rejected with BadOrigin.
#[test]
#[ignore = "TODO §6: needs mock.rs"]
fn register_kyc_attestation_requires_compliance_origin() {
	// new_test_ext().execute_with(|| {
	//     let subject: u64 = 42;
	//     let jurisdiction: BoundedVec<u8, _> =
	//         b"CH".to_vec().try_into().unwrap();
	//     assert_noop!(
	//         crate::Pallet::<Test>::register_kyc_attestation(
	//             RawOrigin::Signed(1).into(),
	//             subject,
	//             H256::repeat_byte(0xAB),
	//             jurisdiction,
	//             1_000u32.into(),
	//             KYCLevel::Basic,
	//             false,
	//         ),
	//         sp_runtime::DispatchError::BadOrigin
	//     );
	// });
}

/// (b) Successful call writes to both KYCStatus + KYCAttestations.
#[test]
#[ignore = "TODO §6: needs mock.rs"]
fn register_kyc_attestation_inserts_storage() {
	// new_test_ext().execute_with(|| {
	//     let subject: u64 = 42;
	//     let jurisdiction: BoundedVec<u8, _> =
	//         b"CH".to_vec().try_into().unwrap();
	//     let hash = H256::repeat_byte(0xAB);
	//
	//     assert_ok!(crate::Pallet::<Test>::register_kyc_attestation(
	//         RawOrigin::Root.into(),
	//         subject,
	//         hash,
	//         jurisdiction.clone(),
	//         1_000u32.into(),
	//         KYCLevel::Enhanced,
	//         true,
	//     ));
	//
	//     assert_eq!(KYCStatus::<Test>::get(&subject), KYCLevel::Enhanced);
	//     let att = KYCAttestations::<Test>::get(&subject).unwrap();
	//     assert_eq!(att.hash, hash);
	//     assert_eq!(att.jurisdiction, jurisdiction);
	//     assert_eq!(att.level, KYCLevel::Enhanced);
	//     assert!(att.eidas_present);
	// });
}

/// (c) Event payload matches what we deposited.
#[test]
#[ignore = "TODO §6: needs mock.rs"]
fn register_kyc_attestation_emits_event() {
	// new_test_ext().execute_with(|| {
	//     System::set_block_number(1);
	//
	//     let subject: u64 = 7;
	//     let jurisdiction: BoundedVec<u8, _> =
	//         b"ES".to_vec().try_into().unwrap();
	//     let hash = H256::repeat_byte(0xCD);
	//
	//     assert_ok!(crate::Pallet::<Test>::register_kyc_attestation(
	//         RawOrigin::Root.into(),
	//         subject,
	//         hash,
	//         jurisdiction,
	//         1_000u32.into(),
	//         KYCLevel::BusinessVerified,
	//         false,
	//     ));
	//
	//     System::assert_last_event(
	//         Event::KYCAttestationRegistered {
	//             subject,
	//             kyc_level: KYCLevel::BusinessVerified,
	//             attestation_hash: hash,
	//             expires_at: 1_000u32.into(),
	//             eidas_present: false,
	//         }
	//         .into(),
	//     );
	// });
}
