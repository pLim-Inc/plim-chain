//! Unit tests for pallet-plim-kyc.

use crate::{
	mock::{
		new_test_ext, PlimKyc, RuntimeEvent, RuntimeOrigin, System, Test, ATTESTOR1, ATTESTOR2,
		NON_ATTESTOR, SUBJECT_A, SUBJECT_B,
	},
	pallet::{Attestors, KycRecords, SanctionList},
	types::{KycLevel, KycRecord, SanctionReason},
	Error, Event, KycProvider,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, Hash};

/// Helper: build a sane default `KycRecord` attested by `ATTESTOR1`.
fn record(level: KycLevel, expires_at: u64) -> KycRecord<Test> {
	KycRecord::<Test> {
		level,
		attested_by: ATTESTOR1,
		attested_at: 1,
		expires_at,
		document_hash: H256::repeat_byte(0xAB),
		country_code: *b"CH",
	}
}

/// Helper: build a `KycRecord` attested by an arbitrary account.
fn record_by(attestor: u64, level: KycLevel, expires_at: u64) -> KycRecord<Test> {
	KycRecord::<Test> {
		level,
		attested_by: attestor,
		attested_at: 1,
		expires_at,
		document_hash: H256::repeat_byte(0xCD),
		country_code: *b"DE",
	}
}

/// Helper: bounded reason vec.
fn reason(s: &[u8]) -> BoundedVec<u8, frame_support::pallet_prelude::ConstU32<64>> {
	BoundedVec::try_from(s.to_vec()).expect("reason fits in 64 bytes")
}

// ---------------------------------------------------------------------------
// 1. set_kyc by non-attestor → NotAttestor
// ---------------------------------------------------------------------------
#[test]
fn set_kyc_by_non_attestor_fails() {
	new_test_ext().execute_with(|| {
		let rec = KycRecord::<Test> {
			level: KycLevel::Basic,
			attested_by: NON_ATTESTOR,
			attested_at: 1,
			expires_at: 1_000,
			document_hash: H256::zero(),
			country_code: *b"CH",
		};
		assert_noop!(
			PlimKyc::set_kyc(RuntimeOrigin::signed(NON_ATTESTOR), SUBJECT_A, rec),
			Error::<Test>::NotAttestor,
		);
	});
}

// ---------------------------------------------------------------------------
// 2. set_kyc overwrites existing record (verify level changes)
// ---------------------------------------------------------------------------
#[test]
fn set_kyc_overwrites_existing_record() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Basic, 1_000),
		));
		assert_eq!(KycRecords::<Test>::get(SUBJECT_A).unwrap().level, KycLevel::Basic);

		// Overwrite with Enhanced.
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Enhanced, 2_000),
		));
		let stored = KycRecords::<Test>::get(SUBJECT_A).unwrap();
		assert_eq!(stored.level, KycLevel::Enhanced);
		assert_eq!(stored.expires_at, 2_000);
	});
}

// ---------------------------------------------------------------------------
// 3. add_to_sanction_list auto-revokes existing KYC
// ---------------------------------------------------------------------------
#[test]
fn sanction_auto_revokes_existing_kyc() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Enhanced, 1_000),
		));
		assert!(KycRecords::<Test>::contains_key(SUBJECT_A));

		assert_ok!(PlimKyc::add_to_sanction_list(
			RuntimeOrigin::root(),
			SUBJECT_A,
			SanctionReason::OfacSdn,
		));

		// Record removed and account is sanctioned.
		assert!(!KycRecords::<Test>::contains_key(SUBJECT_A));
		assert_eq!(SanctionList::<Test>::get(SUBJECT_A), Some(SanctionReason::OfacSdn));

		// Both events emitted.
		let evs = System::events();
		let has_revoked = evs.iter().any(|e| matches!(
			&e.event,
			RuntimeEvent::PlimKyc(Event::KycRevoked { account, .. }) if *account == SUBJECT_A
		));
		let has_sanctioned = evs.iter().any(|e| matches!(
			&e.event,
			RuntimeEvent::PlimKyc(Event::AccountSanctioned { account, .. }) if *account == SUBJECT_A
		));
		assert!(has_revoked, "KycRevoked event should be emitted on auto-revoke");
		assert!(has_sanctioned, "AccountSanctioned event should be emitted");
	});
}

// ---------------------------------------------------------------------------
// 4. read after expiry: require_at_least → KycExpired AND is_expired = true
// ---------------------------------------------------------------------------
#[test]
fn read_after_expiry_returns_kyc_expired() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Enhanced, 100),
		));

		// Advance the chain past expiry.
		System::set_block_number(500);

		assert!(<PlimKyc as KycProvider<u64, u64>>::is_expired(&SUBJECT_A, 500));
		assert_noop!(
			<PlimKyc as KycProvider<u64, u64>>::require_at_least(
				&SUBJECT_A,
				KycLevel::Basic,
				500,
			),
			Error::<Test>::KycExpired,
		);
	});
}

// ---------------------------------------------------------------------------
// 5. remove_attestor prevents future submissions
// ---------------------------------------------------------------------------
#[test]
fn removed_attestor_cannot_submit() {
	new_test_ext().execute_with(|| {
		// Sanity: ATTESTOR1 can submit before removal.
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Basic, 1_000),
		));

		// Remove ATTESTOR1.
		assert_ok!(PlimKyc::remove_attestor(RuntimeOrigin::root(), ATTESTOR1));
		assert!(!Attestors::<Test>::get().contains(&ATTESTOR1));

		// Future submissions from ATTESTOR1 must fail.
		assert_noop!(
			PlimKyc::set_kyc(
				RuntimeOrigin::signed(ATTESTOR1),
				SUBJECT_B,
				record(KycLevel::Basic, 1_000),
			),
			Error::<Test>::NotAttestor,
		);
	});
}

// ---------------------------------------------------------------------------
// 6. SanctionList check blocks set_kyc → AccountSanctioned
// ---------------------------------------------------------------------------
#[test]
fn sanctioned_account_blocks_set_kyc() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::add_to_sanction_list(
			RuntimeOrigin::root(),
			SUBJECT_A,
			SanctionReason::EuSanctions,
		));

		assert_noop!(
			PlimKyc::set_kyc(
				RuntimeOrigin::signed(ATTESTOR1),
				SUBJECT_A,
				record(KycLevel::Basic, 1_000),
			),
			Error::<Test>::AccountSanctioned,
		);
	});
}

// ---------------------------------------------------------------------------
// 7. Level ordering test
// ---------------------------------------------------------------------------
#[test]
fn kyc_level_ordering_holds() {
	assert!(KycLevel::None < KycLevel::Basic);
	assert!(KycLevel::Basic < KycLevel::Enhanced);
	assert!(KycLevel::Enhanced < KycLevel::Institutional);
}

// ---------------------------------------------------------------------------
// 8. require_at_least: Basic when Enhanced required → KycBelowRequiredLevel
// ---------------------------------------------------------------------------
#[test]
fn require_at_least_below_required_level() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Basic, 10_000),
		));

		assert_noop!(
			<PlimKyc as KycProvider<u64, u64>>::require_at_least(
				&SUBJECT_A,
				KycLevel::Enhanced,
				50,
			),
			Error::<Test>::KycBelowRequiredLevel,
		);

		// And the satisfied case works.
		assert_ok!(<PlimKyc as KycProvider<u64, u64>>::require_at_least(
			&SUBJECT_A,
			KycLevel::Basic,
			50,
		));
	});
}

// ---------------------------------------------------------------------------
// 9. revoke_kyc emits event with reason_hash == blake2_256(reason)
// ---------------------------------------------------------------------------
#[test]
fn revoke_kyc_emits_correct_reason_hash() {
	new_test_ext().execute_with(|| {
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			record(KycLevel::Basic, 1_000),
		));

		let raw_reason = b"document_fraud_detected";
		let reason_bv = reason(raw_reason);
		let expected_hash: H256 = BlakeTwo256::hash(raw_reason);

		assert_ok!(PlimKyc::revoke_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			reason_bv,
		));

		System::assert_last_event(
			Event::KycRevoked { account: SUBJECT_A, reason_hash: expected_hash }.into(),
		);
		assert!(!KycRecords::<Test>::contains_key(SUBJECT_A));
	});
}

// ---------------------------------------------------------------------------
// 10. Attestor A can revoke a record set by attestor B
// ---------------------------------------------------------------------------
#[test]
fn any_attestor_can_revoke_any_record() {
	new_test_ext().execute_with(|| {
		// ATTESTOR2 sets the record.
		assert_ok!(PlimKyc::set_kyc(
			RuntimeOrigin::signed(ATTESTOR2),
			SUBJECT_A,
			record_by(ATTESTOR2, KycLevel::Enhanced, 1_000),
		));
		assert!(KycRecords::<Test>::contains_key(SUBJECT_A));

		// ATTESTOR1 revokes it.
		assert_ok!(PlimKyc::revoke_kyc(
			RuntimeOrigin::signed(ATTESTOR1),
			SUBJECT_A,
			reason(b"cross-attestor revocation"),
		));
		assert!(!KycRecords::<Test>::contains_key(SUBJECT_A));
	});
}
