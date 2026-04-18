//! Unit tests for pallet-plim-royalties.

#![cfg(test)]

use crate::{
	mock::*,
	types::RoyaltyCurrency,
	AccumulatedRoyalties, Error, OnRoyaltyPayment, PlatformFeeBp, PlatformTreasury,
	RoyaltyHistory, PaymentCount, TotalRoyaltiesPaid,
};
use frame_support::{assert_noop, assert_ok};

// ---------------------------------------------------------------------------
// OnRoyaltyPayment trait tests
// ---------------------------------------------------------------------------

#[test]
fn on_royalty_paid_accumulates_correctly() {
	new_test_ext().execute_with(|| {
		// Two royalty payments for creator 1 in PLIM.
		<crate::Pallet<Test> as OnRoyaltyPayment<AccountId, u32, Balance>>::on_royalty_paid(
			&1, &42, 500, RoyaltyCurrency::PLIM,
		);
		<crate::Pallet<Test> as OnRoyaltyPayment<AccountId, u32, Balance>>::on_royalty_paid(
			&1, &43, 300, RoyaltyCurrency::PLIM,
		);

		assert_eq!(AccumulatedRoyalties::<Test>::get(&1, &RoyaltyCurrency::PLIM), 800);

		// A payment in a different currency should be tracked separately.
		<crate::Pallet<Test> as OnRoyaltyPayment<AccountId, u32, Balance>>::on_royalty_paid(
			&1, &44, 100, RoyaltyCurrency::PEUR,
		);
		assert_eq!(AccumulatedRoyalties::<Test>::get(&1, &RoyaltyCurrency::PEUR), 100);
		// PLIM untouched.
		assert_eq!(AccumulatedRoyalties::<Test>::get(&1, &RoyaltyCurrency::PLIM), 800);
	});
}

#[test]
fn royalty_history_recorded() {
	new_test_ext().execute_with(|| {
		System::set_block_number(10);

		<crate::Pallet<Test> as OnRoyaltyPayment<AccountId, u32, Balance>>::on_royalty_paid(
			&2, &99, 1_000, RoyaltyCurrency::EURFiat,
		);

		// At least one entry should exist in history.
		assert_eq!(PaymentCount::<Test>::get(), 1);
		assert_eq!(TotalRoyaltiesPaid::<Test>::get(), 1_000);

		// Verify the history entry contents via iteration (we know there's exactly 1).
		let mut found = false;
		RoyaltyHistory::<Test>::iter().for_each(|(_key, payment)| {
			assert_eq!(payment.creator, 2);
			assert_eq!(payment.item_id, 99);
			assert_eq!(payment.amount, 1_000);
			assert_eq!(payment.currency, RoyaltyCurrency::EURFiat);
			assert_eq!(payment.block_number, 10);
			assert!(!payment.claimed);
			found = true;
		});
		assert!(found, "royalty history entry not found");
	});
}

// ---------------------------------------------------------------------------
// claim_accumulated_royalties tests
// ---------------------------------------------------------------------------

#[test]
fn claim_royalties_resets_balance() {
	new_test_ext().execute_with(|| {
		// Accumulate 500 PEUR for creator 1.
		<crate::Pallet<Test> as OnRoyaltyPayment<AccountId, u32, Balance>>::on_royalty_paid(
			&1, &10, 500, RoyaltyCurrency::PEUR,
		);
		assert_eq!(AccumulatedRoyalties::<Test>::get(&1, &RoyaltyCurrency::PEUR), 500);

		// Claim (PEUR = off-chain settlement, event only).
		assert_ok!(crate::Pallet::<Test>::claim_accumulated_royalties(
			frame_system::RawOrigin::Signed(1).into(),
			RoyaltyCurrency::PEUR,
		));

		// Accumulated balance should be zero after claim.
		assert_eq!(AccumulatedRoyalties::<Test>::get(&1, &RoyaltyCurrency::PEUR), 0);
	});
}

#[test]
fn claim_royalties_fails_if_zero() {
	new_test_ext().execute_with(|| {
		// No royalties accumulated for creator 1.
		assert_noop!(
			crate::Pallet::<Test>::claim_accumulated_royalties(
				frame_system::RawOrigin::Signed(1).into(),
				RoyaltyCurrency::PLIM,
			),
			Error::<Test>::NoAccumulatedRoyalties
		);
	});
}

// ---------------------------------------------------------------------------
// Platform fee / treasury admin tests
// ---------------------------------------------------------------------------

#[test]
fn platform_fee_max_30_pct() {
	new_test_ext().execute_with(|| {
		// 30% (3000 bp) should succeed.
		assert_ok!(crate::Pallet::<Test>::update_platform_fee(
			frame_system::RawOrigin::Root.into(),
			3000,
		));
		assert_eq!(PlatformFeeBp::<Test>::get(), 3000);

		// 30.01% (3001 bp) should fail.
		assert_noop!(
			crate::Pallet::<Test>::update_platform_fee(
				frame_system::RawOrigin::Root.into(),
				3001,
			),
			Error::<Test>::InvalidFee
		);
	});
}

#[test]
fn set_treasury_admin_only() {
	new_test_ext().execute_with(|| {
		// Non-admin (signed origin) should fail.
		assert_noop!(
			crate::Pallet::<Test>::set_platform_treasury(
				frame_system::RawOrigin::Signed(1).into(),
				99,
			),
			sp_runtime::DispatchError::BadOrigin
		);

		// Admin (root) should succeed.
		assert_ok!(crate::Pallet::<Test>::set_platform_treasury(
			frame_system::RawOrigin::Root.into(),
			99,
		));
		assert_eq!(PlatformTreasury::<Test>::get(), Some(99));
	});
}

#[test]
fn update_platform_fee_admin_only() {
	new_test_ext().execute_with(|| {
		// Non-admin should fail.
		assert_noop!(
			crate::Pallet::<Test>::update_platform_fee(
				frame_system::RawOrigin::Signed(1).into(),
				100,
			),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}
