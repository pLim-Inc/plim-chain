use crate::{
	mock::*,
	pallet::{PendingRates, Quorum, Rates, Updaters},
	types::{AssetPair, OracleRate},
	Error, Event, RateProvider,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::DispatchError;

extern crate alloc;
use alloc::vec;

const ALICE: u64 = 1;
const BOB: u64 = 2;
const CAROL: u64 = 3;
const EVE: u64 = 99;

// 1. propose_rate by non-updater -> NotUpdater
#[test]
fn propose_rate_by_non_updater_fails() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			crate::Pallet::<Test>::propose_rate(
				RuntimeOrigin::signed(EVE),
				AssetPair::PlimEur,
				1_500_000,
			),
			Error::<Test>::NotUpdater
		);
	});
}

// 2. single propose with quorum=2 does NOT activate
#[test]
fn single_propose_below_quorum_does_not_activate() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(1);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			1_500_000,
		));
		assert!(Rates::<Test>::get(AssetPair::PlimEur).is_none());
		// Pending should still hold ALICE's proposal.
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_some());
	});
}

// 3. two distinct updaters same value -> activates and emits RateUpdated
#[test]
fn quorum_reached_activates_rate() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(10);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			1_500_000,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::PlimEur,
			1_500_000,
		));
		let r: OracleRate<Test> =
			Rates::<Test>::get(AssetPair::PlimEur).expect("rate should be set");
		assert_eq!(r.rate_micros, 1_500_000);
		assert_eq!(r.updated_at, 10);
		assert_eq!(r.quorum_attesters.len(), 2);

		// Pending entries for contributing attesters cleared.
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, BOB).is_none());

		// Verify event was emitted.
		let evts = System::events();
		let saw_updated = evts.iter().any(|r| {
			matches!(
				r.event,
				RuntimeEvent::PlimOracle(Event::RateUpdated {
					pair: AssetPair::PlimEur,
					rate_micros: 1_500_000,
					..
				})
			)
		});
		assert!(saw_updated, "RateUpdated event should be emitted");
	});
}

// 4. two updaters with different values -> no activation
#[test]
fn divergent_proposals_do_not_activate() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(1);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			1_500_000,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::PlimEur,
			1_600_000,
		));
		assert!(Rates::<Test>::get(AssetPair::PlimEur).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_some());
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, BOB).is_some());
	});
}

// 5. stale rate -> get_rate returns StaleRate
#[test]
fn get_rate_returns_stale_when_aged_out() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(5);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			2_000_000,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::PlimEur,
			2_000_000,
		));
		// StalenessWindow = 100; advance well past it.
		System::set_block_number(5 + 101);
		let res = <crate::Pallet<Test> as RateProvider<Test>>::get_rate(AssetPair::PlimEur);
		assert_eq!(res, Err(DispatchError::from(Error::<Test>::StaleRate)));
		assert!(!<crate::Pallet<Test> as RateProvider<Test>>::is_fresh(
			AssetPair::PlimEur,
			System::block_number()
		));
	});
}

// 6. remove_updater clears their pending proposals
#[test]
fn remove_updater_clears_pending_proposals() {
	new_test_ext_with(vec![ALICE, BOB, CAROL], 2).execute_with(|| {
		System::set_block_number(1);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			1_111,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::BtcEur,
			2_222,
		));
		// Sanity.
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_some());
		assert!(PendingRates::<Test>::get(AssetPair::BtcEur, ALICE).is_some());

		assert_ok!(crate::Pallet::<Test>::remove_updater(RuntimeOrigin::root(), ALICE));

		// Pending across ALL pairs should be empty for ALICE.
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::PeurEur, ALICE).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::BtcEur, ALICE).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::EthEur, ALICE).is_none());
		// And ALICE no longer in updater set.
		assert!(!Updaters::<Test>::get().contains(&ALICE));
	});
}

// 7. set_quorum greater than Updaters.len() -> BadQuorum
#[test]
fn set_quorum_above_updater_count_fails() {
	new_test_ext_with(vec![ALICE, BOB], 1).execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::set_quorum(RuntimeOrigin::root(), 5),
			Error::<Test>::BadQuorum
		);
		// Zero also rejected.
		assert_noop!(
			crate::Pallet::<Test>::set_quorum(RuntimeOrigin::root(), 0),
			Error::<Test>::BadQuorum
		);
		// Valid setting works.
		assert_ok!(crate::Pallet::<Test>::set_quorum(RuntimeOrigin::root(), 2));
		assert_eq!(Quorum::<Test>::get(), 2);
	});
}

// 8. add_updater by non-root -> BadOrigin
#[test]
fn add_updater_requires_root() {
	new_test_ext_with(vec![ALICE], 1).execute_with(|| {
		assert_noop!(
			crate::Pallet::<Test>::add_updater(RuntimeOrigin::signed(ALICE), BOB),
			DispatchError::BadOrigin
		);
		// Root works.
		assert_ok!(crate::Pallet::<Test>::add_updater(RuntimeOrigin::root(), BOB));
		assert!(Updaters::<Test>::get().contains(&BOB));
	});
}

// 9. proposes older than StalenessWindow are pruned and don't count
#[test]
fn stale_pending_proposals_are_pruned() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(1);
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			3_141_592,
		));
		// Advance beyond the staleness window so ALICE's pending is stale.
		System::set_block_number(1 + 101);
		// BOB now proposes the same value — quorum should NOT be reached because
		// ALICE's stale entry is pruned first and does not count.
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::PlimEur,
			3_141_592,
		));
		assert!(Rates::<Test>::get(AssetPair::PlimEur).is_none());
		// ALICE's stale entry is gone; BOB's fresh entry remains pending.
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, ALICE).is_none());
		assert!(PendingRates::<Test>::get(AssetPair::PlimEur, BOB).is_some());
	});
}

// 10. multi-asset independence
#[test]
fn rates_for_different_pairs_are_independent() {
	new_test_ext_with(vec![ALICE, BOB], 2).execute_with(|| {
		System::set_block_number(1);
		// Activate PlimEur.
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::PlimEur,
			1_500_000,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::PlimEur,
			1_500_000,
		));
		assert!(Rates::<Test>::get(AssetPair::PlimEur).is_some());
		// BtcEur untouched.
		assert!(Rates::<Test>::get(AssetPair::BtcEur).is_none());

		// Now activate BtcEur — PlimEur must not change.
		let plim = Rates::<Test>::get(AssetPair::PlimEur).unwrap().rate_micros;
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(ALICE),
			AssetPair::BtcEur,
			60_000_000_000,
		));
		assert_ok!(crate::Pallet::<Test>::propose_rate(
			RuntimeOrigin::signed(BOB),
			AssetPair::BtcEur,
			60_000_000_000,
		));
		let btc = Rates::<Test>::get(AssetPair::BtcEur).expect("btc rate set");
		assert_eq!(btc.rate_micros, 60_000_000_000);
		assert_eq!(Rates::<Test>::get(AssetPair::PlimEur).unwrap().rate_micros, plim);
	});
}
