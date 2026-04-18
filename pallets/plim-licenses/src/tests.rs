use crate::{
	mock::*,
	pallet::{CustodyQueue, Licenses, NextItemId},
	types::*,
	Error, Event,
};
use frame_support::{assert_noop, assert_ok};

/// Helper: mint a default Personal license owned by `owner` via root origin.
fn mint_default_license(owner: u64, creator: u64) -> u32 {
	let item_id = NextItemId::<Test>::get();
	assert_ok!(PlimLicenses::mint_license(
		RuntimeOrigin::root(),
		owner,
		LicenseType::Personal,
		1,               // version
		creator,          // original_creator
		2999,             // original_price_eur_cents
		100_000,          // original_price_plim
		true,             // transferable
		false,            // print_commercial
		false,            // derivative_allowed
		false,            // derivative_share_alike
		true,             // attribution_required
		false,            // watermark_required
		false,            // exclusive_burns_original
		500,              // royalty_pct_bp (5%)
		None,             // expires_at
		Some(10),         // max_prints
		None,             // max_copies
		vec![],           // geo_restrictions
		vec![],           // platform_restrictions
		Jurisdiction::Global,
		PaymentMethod::PLIM,
		vec![1, 2, 3],   // payment_proof
		None,             // custody_buyer_email_hash
	));
	item_id
}

#[test]
fn mint_license_happy_path() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let item_id = mint_default_license(42, 10);

		// License stored
		let license = Licenses::<Test>::get(item_id).expect("license should exist");
		assert_eq!(license.current_owner, 42);
		assert_eq!(license.original_creator, 10);
		assert_eq!(license.royalty_pct_bp, 500);
		assert_eq!(license.max_prints, Some(10));
		assert_eq!(license.license_type, LicenseType::Personal);

		// NextItemId incremented
		assert_eq!(NextItemId::<Test>::get(), item_id + 1);

		// Event emitted
		System::assert_last_event(
			Event::LicenseMinted {
				item_id,
				owner: 42,
				license_type: LicenseType::Personal,
			}
			.into(),
		);
	});
}

#[test]
fn mint_license_validates_attrs() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// derivative_share_alike without derivative_allowed => error
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::Personal,
				1,
				10,
				0,
				0,
				true,
				false,
				false,             // derivative_allowed = false
				true,              // derivative_share_alike = true => invalid
				false,
				false,
				false,
				0,
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidLicenseAttrs
		);

		// exclusive_burns_original on non-Exclusive type => error
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::Personal, // not Exclusive
				1,
				10,
				0,
				0,
				true,
				false,
				false,
				false,
				false,
				false,
				true, // exclusive_burns_original = true => invalid
				0,
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidLicenseAttrs
		);

		// TimeLimited without expires_at => error
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::TimeLimited,
				1,
				10,
				0,
				0,
				true,
				false,
				false,
				false,
				false,
				false,
				false,
				0,
				None, // no expires_at => invalid for TimeLimited
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidLicenseAttrs
		);
	});
}

#[test]
fn mint_license_requires_marketplace_origin() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Signed origin (not root) should fail since MarketplaceOrigin = EnsureRoot
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::signed(99),
				42,
				LicenseType::Personal,
				1,
				10,
				0,
				0,
				false,
				false,
				false,
				false,
				false,
				false,
				false,
				0,
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn custody_claim_prevents_double_claim() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let email_hash = [0xABu8; 32];
		let item_id = NextItemId::<Test>::get();

		// Mint with custody
		assert_ok!(PlimLicenses::mint_license(
			RuntimeOrigin::root(),
			1, // custodian
			LicenseType::Personal,
			1,
			10,
			0,
			0,
			true,
			false,
			false,
			false,
			false,
			false,
			false,
			500,
			None,
			None,
			None,
			vec![],
			vec![],
			Jurisdiction::Global,
			PaymentMethod::Custody,
			vec![],
			Some(email_hash),
		));

		// Custody record exists
		assert!(CustodyQueue::<Test>::get(item_id).is_some());

		// First claim succeeds
		assert_ok!(PlimLicenses::claim_custody_license(
			RuntimeOrigin::signed(42),
			item_id,
			0, // nonce
		));

		// License owner updated
		let license = Licenses::<Test>::get(item_id).expect("license exists");
		assert_eq!(license.current_owner, 42);

		// Second claim fails
		assert_noop!(
			PlimLicenses::claim_custody_license(RuntimeOrigin::signed(99), item_id, 0),
			Error::<Test>::AlreadyClaimed
		);
	});
}

#[test]
fn royalty_pct_bounded_to_25_pct() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// 2501 bp = 25.01% => exceeds cap
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::Commercial,
				1,
				10,
				0,
				0,
				true,
				true,
				false,
				false,
				false,
				false,
				false,
				2501, // exceeds 2500 cap
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidRoyaltyPct
		);

		// 2500 bp = exactly 25% => allowed
		assert_ok!(PlimLicenses::mint_license(
			RuntimeOrigin::root(),
			42,
			LicenseType::Commercial,
			1,
			10,
			0,
			0,
			true,
			true,
			false,
			false,
			false,
			false,
			false,
			2500, // exactly at cap
			None,
			None,
			None,
			vec![],
			vec![],
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			vec![],
			None,
		));
	});
}

#[test]
fn exclusive_requires_exclusive_type() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// exclusive_burns_original on Commercial => error
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::Commercial,
				1,
				10,
				0,
				0,
				true,
				true,
				false,
				false,
				false,
				false,
				true, // exclusive_burns_original
				0,
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidLicenseAttrs
		);

		// exclusive_burns_original on Exclusive => OK
		assert_ok!(PlimLicenses::mint_license(
			RuntimeOrigin::root(),
			42,
			LicenseType::Exclusive,
			1,
			10,
			0,
			0,
			true,
			true,
			false,
			false,
			false,
			false,
			true, // exclusive_burns_original
			0,
			None,
			None,
			None,
			vec![],
			vec![],
			Jurisdiction::Global,
			PaymentMethod::PLIM,
			vec![],
			None,
		));
	});
}

#[test]
fn burn_license_by_owner() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let item_id = mint_default_license(42, 10);

		// Non-owner cannot burn
		assert_noop!(
			PlimLicenses::burn_license(RuntimeOrigin::signed(99), item_id),
			Error::<Test>::Unauthorized
		);

		// Owner can burn
		assert_ok!(PlimLicenses::burn_license(RuntimeOrigin::signed(42), item_id));

		// License is gone
		assert!(Licenses::<Test>::get(item_id).is_none());

		// Event emitted
		System::assert_last_event(
			Event::LicenseBurned { item_id, owner: 42 }.into(),
		);

		// Burning again fails
		assert_noop!(
			PlimLicenses::burn_license(RuntimeOrigin::signed(42), item_id),
			Error::<Test>::LicenseNotFound
		);
	});
}

#[test]
fn royalty_requires_transferable() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Royalty > 0 but transferable = false => error
		assert_noop!(
			PlimLicenses::mint_license(
				RuntimeOrigin::root(),
				42,
				LicenseType::Personal,
				1,
				10,
				0,
				0,
				false, // not transferable
				false,
				false,
				false,
				false,
				false,
				false,
				100, // royalty > 0
				None,
				None,
				None,
				vec![],
				vec![],
				Jurisdiction::Global,
				PaymentMethod::PLIM,
				vec![],
				None,
			),
			Error::<Test>::InvalidLicenseAttrs
		);
	});
}

#[test]
fn set_creator_config_works() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		assert_ok!(PlimLicenses::set_creator_config(
			RuntimeOrigin::signed(10),
			1000, // 10%
			10,   // payout to self
			RoyaltyAsset::Native,
		));

		System::assert_last_event(Event::CreatorConfigSet { creator: 10 }.into());

		// Rejects > 25%
		assert_noop!(
			PlimLicenses::set_creator_config(
				RuntimeOrigin::signed(10),
				2501,
				10,
				RoyaltyAsset::Native,
			),
			Error::<Test>::InvalidRoyaltyPct
		);
	});
}

#[test]
fn revoke_license_admin_only() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let item_id = mint_default_license(42, 10);

		// Signed origin cannot revoke (AdminOrigin = EnsureRoot)
		assert_noop!(
			PlimLicenses::revoke_license(RuntimeOrigin::signed(1), item_id),
			sp_runtime::DispatchError::BadOrigin
		);

		// Root can revoke
		assert_ok!(PlimLicenses::revoke_license(RuntimeOrigin::root(), item_id));
		assert!(Licenses::<Test>::get(item_id).is_none());

		System::assert_last_event(Event::LicenseRevoked { item_id }.into());
	});
}

#[test]
fn public_helpers_work() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		let item_id = mint_default_license(42, 10);

		// is_transferable
		assert!(crate::Pallet::<Test>::is_transferable(item_id));
		assert!(!crate::Pallet::<Test>::is_transferable(999));

		// royalty_info
		let (creator, bp) =
			crate::Pallet::<Test>::royalty_info(item_id).expect("should have royalty");
		assert_eq!(creator, 10);
		assert_eq!(bp, 500);

		// Non-existent returns None
		assert!(crate::Pallet::<Test>::royalty_info(999).is_none());
	});
}
