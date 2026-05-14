//! Unit tests for `pallet-rwa`.

use crate::{
	mock::*,
	pallet::{Assets, AssetStatus, NextDistributionId, Shareholders, TotalIssued, UnclaimedYield, YieldDistributions},
	types::*,
	BalanceOf, Error, Event, KycLevel,
};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::H256;
use sp_runtime::traits::Zero;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bounded_symbol(s: &[u8]) -> BoundedVec<u8, frame_support::pallet_prelude::ConstU32<16>> {
	s.to_vec().try_into().expect("symbol fits in 16 bytes")
}

fn bounded_name(s: &[u8]) -> BoundedVec<u8, frame_support::pallet_prelude::ConstU32<128>> {
	s.to_vec().try_into().expect("name fits in 128 bytes")
}

fn make_asset(total_supply: Balance, kyc: KycLevel) -> RwaAsset<Test> {
	RwaAsset::<Test> {
		symbol: bounded_symbol(b"PLIM-RE1"),
		name: bounded_name(b"pLim Swiss RE Fund I"),
		description_hash: H256::repeat_byte(0xAB),
		total_supply,
		kyc_required: kyc,
		yield_currency: Currency::PEur,
		nav_currency: Currency::PEur,
		jurisdiction: *b"CH",
		created_at_block: 1u64,
		manager: ALICE,
	}
}

/// Insert an asset directly into storage (skips the deterministic-id derivation
/// that `register_asset` does, so tests can pick clean ids).
fn install_asset(id: u32, asset: RwaAsset<Test>) {
	Assets::<Test>::insert(id, asset);
	AssetStatus::<Test>::insert(id, RwaStatus::Active);
}

fn payment_proof(payer: AccountId, amount: Balance) -> PaymentProof<AccountId, Balance> {
	PaymentProof { payer, amount, proof_hash: H256::repeat_byte(0x01) }
}

// ---------------------------------------------------------------------------
// 1. register_asset by non-root → BadOrigin
// ---------------------------------------------------------------------------
#[test]
fn register_asset_non_root_fails() {
	new_test_ext().execute_with(|| {
		let asset = make_asset(1_000, KycLevel::Basic);
		assert_noop!(
			Rwa::register_asset(RuntimeOrigin::signed(ALICE), asset),
			sp_runtime::DispatchError::BadOrigin,
		);
	});
}

// ---------------------------------------------------------------------------
// 2. mint_shares over total_supply → TotalSupplyExceeded
// ---------------------------------------------------------------------------
#[test]
fn mint_shares_over_supply_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(100, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		assert_noop!(
			Rwa::mint_shares(
				RuntimeOrigin::signed(ALICE),
				1,
				BOB,
				101,
				payment_proof(ALICE, 101),
			),
			Error::<Test>::TotalSupplyExceeded,
		);
	});
}

// ---------------------------------------------------------------------------
// 3. mint_shares to KYC-ungated user → ReceiverKycFailed
// ---------------------------------------------------------------------------
#[test]
fn mint_shares_unkyced_receiver_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		// EVE has KycLevel::None by default.
		assert_noop!(
			Rwa::mint_shares(
				RuntimeOrigin::signed(ALICE),
				1,
				EVE,
				10,
				payment_proof(ALICE, 10),
			),
			Error::<Test>::ReceiverKycFailed,
		);
	});
}

// ---------------------------------------------------------------------------
// 4. transfer_shares sender fails KYC → SenderKycFailed
// ---------------------------------------------------------------------------
#[test]
fn transfer_shares_sender_kyc_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Enhanced));
		// Pre-fund BOB by minting (he is Institutional → satisfies Enhanced).
		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		// Now downgrade BOB so the transfer he initiates fails.
		MockKyc::set_level(BOB, KycLevel::None);
		MockKyc::set_level(CHARLIE, KycLevel::Enhanced);
		assert_noop!(
			Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, CHARLIE, 10),
			Error::<Test>::SenderKycFailed,
		);
	});
}

// ---------------------------------------------------------------------------
// 5. transfer_shares receiver fails KYC → ReceiverKycFailed
// ---------------------------------------------------------------------------
#[test]
fn transfer_shares_receiver_kyc_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Enhanced));
		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		// EVE is unKYCed.
		assert_noop!(
			Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, EVE, 10),
			Error::<Test>::ReceiverKycFailed,
		);
	});
}

// ---------------------------------------------------------------------------
// 6. transfer_shares when Frozen → AssetFrozen
// ---------------------------------------------------------------------------
#[test]
fn transfer_shares_when_frozen_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		MockKyc::set_level(CHARLIE, KycLevel::Basic);
		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		assert_ok!(Rwa::freeze(RuntimeOrigin::root(), 1));
		assert_noop!(
			Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, CHARLIE, 10),
			Error::<Test>::AssetFrozen,
		);
	});
}

// ---------------------------------------------------------------------------
// 7. distribute_yield: 50/30/20 of 100 issued, 1000 yield → 500/300/200
// ---------------------------------------------------------------------------
#[test]
fn distribute_yield_pro_rata() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));

		// Prime three holders directly via storage (faster than mint_shares loop).
		Shareholders::<Test>::insert(1, BOB, 50u128);
		Shareholders::<Test>::insert(1, CHARLIE, 30u128);
		Shareholders::<Test>::insert(1, DAVE, 20u128);
		TotalIssued::<Test>::insert(1, 100u128);

		assert_ok!(Rwa::distribute_yield(
			RuntimeOrigin::signed(ALICE),
			1,
			1_000u128,
			Currency::PEur,
			H256::repeat_byte(0xCD),
		));

		let dist_id = 0u64;
		let bob = UnclaimedYield::<Test>::get((1u32, dist_id), &BOB);
		let charlie = UnclaimedYield::<Test>::get((1u32, dist_id), &CHARLIE);
		let dave = UnclaimedYield::<Test>::get((1u32, dist_id), &DAVE);
		assert_eq!(bob, 500);
		assert_eq!(charlie, 300);
		assert_eq!(dave, 200);
		assert_eq!(bob + charlie + dave, 1_000);

		assert_eq!(NextDistributionId::<Test>::get(1u32), 1u64);
	});
}

// ---------------------------------------------------------------------------
// 8. claim_yield: first call pays; second call → NothingToClaim
// ---------------------------------------------------------------------------
#[test]
fn claim_yield_double_claim_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		Shareholders::<Test>::insert(1, BOB, 100u128);
		TotalIssued::<Test>::insert(1, 100u128);

		assert_ok!(Rwa::distribute_yield(
			RuntimeOrigin::signed(ALICE),
			1,
			500u128,
			Currency::PEur,
			H256::zero(),
		));

		let bal_before = Balances::free_balance(BOB);
		assert_ok!(Rwa::claim_yield(RuntimeOrigin::signed(BOB), 1, 0));
		let bal_after = Balances::free_balance(BOB);
		assert_eq!(bal_after - bal_before, 500);

		assert_noop!(
			Rwa::claim_yield(RuntimeOrigin::signed(BOB), 1, 0),
			Error::<Test>::NothingToClaim,
		);
	});
}

// ---------------------------------------------------------------------------
// 9. claim_all_yield bounded: 100 distributions, MaxDistributionsPerClaim=50
// ---------------------------------------------------------------------------
#[test]
fn claim_all_yield_bounded_to_max() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000_000, KycLevel::Basic));
		Shareholders::<Test>::insert(1, BOB, 100u128);
		TotalIssued::<Test>::insert(1, 100u128);

		// 100 distributions of 100 PLIM each (BOB owns 100% so gets the lot).
		for _ in 0..100u32 {
			assert_ok!(Rwa::distribute_yield(
				RuntimeOrigin::signed(ALICE),
				1,
				100u128,
				Currency::Plim,
				H256::zero(),
			));
		}

		// Verify we have 100 unclaimed rows for BOB.
		let count_before: u32 = (0u64..100)
			.filter(|d| !UnclaimedYield::<Test>::get((1u32, *d), &BOB).is_zero())
			.count() as u32;
		assert_eq!(count_before, 100);

		assert_ok!(Rwa::claim_all_yield(RuntimeOrigin::signed(BOB), 1));

		let count_after: u32 = (0u64..100)
			.filter(|d| !UnclaimedYield::<Test>::get((1u32, *d), &BOB).is_zero())
			.count() as u32;
		// Exactly 50 should remain (cap = 50).
		assert_eq!(count_before - count_after, 50);
	});
}

// ---------------------------------------------------------------------------
// 10. freeze blocks mint + transfer, allows claim
// ---------------------------------------------------------------------------
#[test]
fn freeze_blocks_mint_and_transfer_allows_claim() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		MockKyc::set_level(CHARLIE, KycLevel::Basic);

		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		// Distribute first (manager-only; Active state still).
		assert_ok!(Rwa::distribute_yield(
			RuntimeOrigin::signed(ALICE),
			1,
			500u128,
			Currency::Plim,
			H256::zero(),
		));

		assert_ok!(Rwa::freeze(RuntimeOrigin::root(), 1));

		// Mint blocked.
		assert_noop!(
			Rwa::mint_shares(
				RuntimeOrigin::signed(ALICE),
				1,
				BOB,
				1,
				payment_proof(ALICE, 1),
			),
			Error::<Test>::AssetFrozen,
		);
		// Transfer blocked.
		assert_noop!(
			Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, CHARLIE, 1),
			Error::<Test>::AssetFrozen,
		);
		// Claim still works.
		assert_ok!(Rwa::claim_yield(RuntimeOrigin::signed(BOB), 1, 0));
	});
}

// ---------------------------------------------------------------------------
// 11. unfreeze restores all ops
// ---------------------------------------------------------------------------
#[test]
fn unfreeze_restores_ops() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		MockKyc::set_level(CHARLIE, KycLevel::Basic);

		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		assert_ok!(Rwa::freeze(RuntimeOrigin::root(), 1));
		assert_ok!(Rwa::unfreeze(RuntimeOrigin::root(), 1));

		// Mint and transfer both succeed again.
		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			10,
			payment_proof(ALICE, 10),
		));
		assert_ok!(Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, CHARLIE, 5));
	});
}

// ---------------------------------------------------------------------------
// 12. wind_down sets WoundDown; subsequent mint → AssetWoundDown
// ---------------------------------------------------------------------------
#[test]
fn wind_down_blocks_mint() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		assert_ok!(Rwa::wind_down(RuntimeOrigin::root(), 1));
		assert_eq!(AssetStatus::<Test>::get(1u32), RwaStatus::WoundDown);
		assert_noop!(
			Rwa::mint_shares(
				RuntimeOrigin::signed(ALICE),
				1,
				BOB,
				10,
				payment_proof(ALICE, 10),
			),
			Error::<Test>::AssetWoundDown,
		);
	});
}

// ---------------------------------------------------------------------------
// 13. snapshot atomicity — transfer after distribution does NOT entitle the
//     recipient to that earlier distribution.
// ---------------------------------------------------------------------------
#[test]
fn snapshot_is_atomic() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		MockKyc::set_level(CHARLIE, KycLevel::Basic);

		Shareholders::<Test>::insert(1, BOB, 100u128);
		TotalIssued::<Test>::insert(1, 100u128);

		// Distribute at block 1.
		assert_ok!(Rwa::distribute_yield(
			RuntimeOrigin::signed(ALICE),
			1,
			500u128,
			Currency::Plim,
			H256::zero(),
		));
		// At this point only BOB has an unclaimed entry.
		assert_eq!(UnclaimedYield::<Test>::get((1u32, 0u64), &BOB), 500);
		assert_eq!(UnclaimedYield::<Test>::get((1u32, 0u64), &CHARLIE), 0);

		// Move to next block and transfer 50 shares from BOB to CHARLIE.
		System::set_block_number(2);
		assert_ok!(Rwa::transfer_shares(RuntimeOrigin::signed(BOB), 1, CHARLIE, 50));

		// CHARLIE's entry for the earlier distribution remains zero.
		assert_eq!(UnclaimedYield::<Test>::get((1u32, 0u64), &CHARLIE), 0);
		// BOB still owes/owns the original 500 of unclaimed.
		assert_eq!(UnclaimedYield::<Test>::get((1u32, 0u64), &BOB), 500);
	});
}

// ---------------------------------------------------------------------------
// 14. burn_shares emits BurnRequested
// ---------------------------------------------------------------------------
#[test]
fn burn_shares_emits_burn_requested() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		MockKyc::set_level(BOB, KycLevel::Basic);
		assert_ok!(Rwa::mint_shares(
			RuntimeOrigin::signed(ALICE),
			1,
			BOB,
			100,
			payment_proof(ALICE, 100),
		));
		assert_ok!(Rwa::burn_shares(RuntimeOrigin::signed(BOB), 1, 25));
		// Tail event must be BurnRequested (BurnRequested is emitted second).
		System::assert_last_event(
			Event::BurnRequested { asset_id: 1, account: BOB, amount: 25 }.into(),
		);
		assert_eq!(Shareholders::<Test>::get(1u32, &BOB), 75u128);
	});
}

// ---------------------------------------------------------------------------
// 15. distribute_yield by non-manager → ManagerOnly
// ---------------------------------------------------------------------------
#[test]
fn distribute_yield_non_manager_fails() {
	new_test_ext().execute_with(|| {
		install_asset(1, make_asset(1_000, KycLevel::Basic));
		Shareholders::<Test>::insert(1, BOB, 100u128);
		TotalIssued::<Test>::insert(1, 100u128);
		assert_noop!(
			Rwa::distribute_yield(
				RuntimeOrigin::signed(BOB),
				1,
				500u128,
				Currency::Plim,
				H256::zero(),
			),
			Error::<Test>::ManagerOnly,
		);
	});
}

// Suppress dead-code warning on the unused `BalanceOf` re-export.
#[allow(dead_code)]
type _Unused = BalanceOf<Test>;
