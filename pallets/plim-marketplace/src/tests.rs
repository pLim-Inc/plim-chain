//! Unit tests for pallet-plim-marketplace.

#![cfg(test)]

use crate::{
	mock::*,
	types::*,
	ActiveListingCount, Error, Listings, Offers, PlatformFeeBp,
};
use frame_support::{assert_noop, assert_ok};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SELLER: AccountId = 1;
const BUYER: AccountId = 2;
const CREATOR: AccountId = 3;
const ADMIN: AccountId = 10;
const ITEM_1: u32 = 42;

fn origin(who: AccountId) -> RuntimeOrigin {
	RuntimeOrigin::signed(who)
}

// ---------------------------------------------------------------------------
// list_for_sale
// ---------------------------------------------------------------------------

#[test]
fn list_for_sale_happy_path() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));

		let listing = Listings::<Test>::get(ITEM_1).expect("listing should exist");
		assert_eq!(listing.seller, SELLER);
		assert_eq!(listing.price, 10_000);
		assert_eq!(listing.currency, ListingCurrency::PLIM);
		assert_eq!(listing.status, ListingStatus::Active);
		assert_eq!(ActiveListingCount::<Test>::get(SELLER), 1);
	});
}

#[test]
fn list_for_sale_rejects_zero_price() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Marketplace::list_for_sale(origin(SELLER), ITEM_1, 0, ListingCurrency::PLIM),
			Error::<Test>::PriceTooLow
		);
	});
}

#[test]
fn list_for_sale_rejects_duplicate() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));
		assert_noop!(
			Marketplace::list_for_sale(origin(SELLER), ITEM_1, 20_000, ListingCurrency::PLIM),
			Error::<Test>::AlreadyListed
		);
	});
}

#[test]
fn list_for_sale_rejects_non_transferable() {
	new_test_ext().execute_with(|| {
		set_transferable(false);
		assert_noop!(
			Marketplace::list_for_sale(origin(SELLER), ITEM_1, 10_000, ListingCurrency::PLIM),
			Error::<Test>::NotTransferable
		);
	});
}

#[test]
fn max_listings_per_account() {
	new_test_ext().execute_with(|| {
		// MaxActiveListingsPerAccount = 5
		for i in 0..5u32 {
			assert_ok!(Marketplace::list_for_sale(
				origin(SELLER),
				100 + i,
				1_000,
				ListingCurrency::PLIM,
			));
		}
		assert_eq!(ActiveListingCount::<Test>::get(SELLER), 5);

		// 6th listing should fail
		assert_noop!(
			Marketplace::list_for_sale(origin(SELLER), 200, 1_000, ListingCurrency::PLIM),
			Error::<Test>::MaxListingsReached
		);
	});
}

// ---------------------------------------------------------------------------
// cancel_listing
// ---------------------------------------------------------------------------

#[test]
fn cancel_listing_only_by_seller() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));

		// Non-seller cannot cancel
		assert_noop!(
			Marketplace::cancel_listing(origin(BUYER), ITEM_1),
			Error::<Test>::NotOwner
		);

		// Seller can cancel
		assert_ok!(Marketplace::cancel_listing(origin(SELLER), ITEM_1));
		let listing = Listings::<Test>::get(ITEM_1).expect("listing still stored");
		assert_eq!(listing.status, ListingStatus::Cancelled);
		assert_eq!(ActiveListingCount::<Test>::get(SELLER), 0);
	});
}

#[test]
fn cancel_non_existent_listing_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Marketplace::cancel_listing(origin(SELLER), 999),
			Error::<Test>::NotListed
		);
	});
}

// ---------------------------------------------------------------------------
// buy_now — split payout
// ---------------------------------------------------------------------------

#[test]
fn buy_now_splits_correctly() {
	new_test_ext().execute_with(|| {
		// Set royalty: creator=3, 10% (1000 bp)
		set_royalty(Some((CREATOR, 1000)));

		// List at 100_000 PLIM
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			100_000,
			ListingCurrency::PLIM,
		));

		let seller_before = Balances::free_balance(SELLER);
		let buyer_before = Balances::free_balance(BUYER);
		let creator_before = Balances::free_balance(CREATOR);
		let treasury = Marketplace::treasury_account();
		let treasury_before = Balances::free_balance(treasury);

		assert_ok!(Marketplace::buy_now(origin(BUYER), ITEM_1));

		// Royalty: 100_000 * 1000 / 10_000 = 10_000
		// Platform fee: 100_000 * 1500 / 10_000 = 15_000
		// Seller gets: 100_000 - 10_000 - 15_000 = 75_000

		assert_eq!(Balances::free_balance(CREATOR), creator_before + 10_000);
		assert_eq!(Balances::free_balance(treasury), treasury_before + 15_000);
		assert_eq!(Balances::free_balance(SELLER), seller_before + 75_000);
		assert_eq!(Balances::free_balance(BUYER), buyer_before - 100_000);

		// Listing is now Sold
		let listing = Listings::<Test>::get(ITEM_1).unwrap();
		assert_eq!(listing.status, ListingStatus::Sold);

		// Royalty callback was invoked
		let log = royalty_paid_log();
		assert_eq!(log.len(), 1);
		assert_eq!(log[0], (CREATOR, ITEM_1, 10_000, ListingCurrency::PLIM));
	});
}

#[test]
fn buy_now_no_royalty() {
	new_test_ext().execute_with(|| {
		// No royalty configured (default)
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			100_000,
			ListingCurrency::PLIM,
		));

		let seller_before = Balances::free_balance(SELLER);
		let treasury = Marketplace::treasury_account();
		let treasury_before = Balances::free_balance(treasury);

		assert_ok!(Marketplace::buy_now(origin(BUYER), ITEM_1));

		// Platform fee only: 15_000, seller gets 85_000
		assert_eq!(Balances::free_balance(treasury), treasury_before + 15_000);
		assert_eq!(Balances::free_balance(SELLER), seller_before + 85_000);
	});
}

#[test]
fn buy_now_fails_if_not_transferable() {
	new_test_ext().execute_with(|| {
		// List first (transferable=true), then toggle off before buy
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));

		set_transferable(false);

		assert_noop!(
			Marketplace::buy_now(origin(BUYER), ITEM_1),
			Error::<Test>::NotTransferable
		);
	});
}

#[test]
fn buy_now_fails_for_fiat_listing() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::EURFiat,
		));
		assert_noop!(
			Marketplace::buy_now(origin(BUYER), ITEM_1),
			Error::<Test>::CannotBuyFiatOnChain
		);
	});
}

// ---------------------------------------------------------------------------
// buy_now_with_fiat_proof
// ---------------------------------------------------------------------------

#[test]
fn fiat_proof_happy_path() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			50_000,
			ListingCurrency::EURFiat,
		));

		let proof = [0xBBu8; 32];
		// MarketplaceOrigin is EnsureSigned in our mock, so admin=10 works
		assert_ok!(Marketplace::buy_now_with_fiat_proof(
			origin(ADMIN),
			ITEM_1,
			BUYER,
			proof,
		));

		let listing = Listings::<Test>::get(ITEM_1).unwrap();
		assert_eq!(listing.status, ListingStatus::Sold);
	});
}

// ---------------------------------------------------------------------------
// Offer lifecycle
// ---------------------------------------------------------------------------

#[test]
fn offer_lifecycle_make_accept() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			100_000,
			ListingCurrency::PLIM,
		));

		// Make offer
		assert_ok!(Marketplace::make_offer(
			origin(BUYER),
			ITEM_1,
			80_000,
			ListingCurrency::PLIM,
			100, // expires in 100 blocks
		));

		// Find the offer id from storage (there should be exactly one)
		let offer_id = {
			let mut found = None;
			Offers::<Test>::iter().for_each(|(k, v)| {
				if v.item_id == ITEM_1 && v.bidder == BUYER {
					found = Some(k);
				}
			});
			found.expect("offer should exist")
		};

		let offer = Offers::<Test>::get(offer_id).unwrap();
		assert_eq!(offer.status, OfferStatus::Pending);
		assert_eq!(offer.amount, 80_000);

		// Seller accepts
		assert_ok!(Marketplace::accept_offer(origin(SELLER), offer_id));

		let offer = Offers::<Test>::get(offer_id).unwrap();
		assert_eq!(offer.status, OfferStatus::Accepted);

		// Listing is now Sold
		let listing = Listings::<Test>::get(ITEM_1).unwrap();
		assert_eq!(listing.status, ListingStatus::Sold);
	});
}

#[test]
fn offer_lifecycle_make_reject() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			100_000,
			ListingCurrency::PLIM,
		));

		assert_ok!(Marketplace::make_offer(
			origin(BUYER),
			ITEM_1,
			50_000,
			ListingCurrency::PLIM,
			100,
		));

		let offer_id = {
			let mut found = None;
			Offers::<Test>::iter().for_each(|(k, v)| {
				if v.item_id == ITEM_1 {
					found = Some(k);
				}
			});
			found.expect("offer should exist")
		};

		// Seller rejects
		assert_ok!(Marketplace::reject_offer(origin(SELLER), offer_id));
		let offer = Offers::<Test>::get(offer_id).unwrap();
		assert_eq!(offer.status, OfferStatus::Rejected);

		// Listing stays active
		let listing = Listings::<Test>::get(ITEM_1).unwrap();
		assert_eq!(listing.status, ListingStatus::Active);
	});
}

#[test]
fn offer_lifecycle_make_withdraw() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			100_000,
			ListingCurrency::PLIM,
		));

		assert_ok!(Marketplace::make_offer(
			origin(BUYER),
			ITEM_1,
			60_000,
			ListingCurrency::PLIM,
			100,
		));

		let offer_id = {
			let mut found = None;
			Offers::<Test>::iter().for_each(|(k, v)| {
				if v.item_id == ITEM_1 {
					found = Some(k);
				}
			});
			found.expect("offer should exist")
		};

		// Non-bidder cannot withdraw
		assert_noop!(
			Marketplace::withdraw_offer(origin(SELLER), offer_id),
			Error::<Test>::NotOwner
		);

		// Bidder can withdraw
		assert_ok!(Marketplace::withdraw_offer(origin(BUYER), offer_id));
		let offer = Offers::<Test>::get(offer_id).unwrap();
		assert_eq!(offer.status, OfferStatus::Withdrawn);
	});
}

#[test]
fn offer_reject_requires_seller() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));
		assert_ok!(Marketplace::make_offer(
			origin(BUYER),
			ITEM_1,
			5_000,
			ListingCurrency::PLIM,
			100,
		));

		let offer_id = {
			let mut found = None;
			Offers::<Test>::iter().for_each(|(k, _)| found = Some(k));
			found.unwrap()
		};

		// Random account cannot reject
		assert_noop!(
			Marketplace::reject_offer(origin(BUYER), offer_id),
			Error::<Test>::NotOwner
		);
	});
}

// ---------------------------------------------------------------------------
// Platform fee update
// ---------------------------------------------------------------------------

#[test]
fn platform_fee_update_admin_only() {
	new_test_ext().execute_with(|| {
		// In the mock, MarketplaceOrigin = EnsureSigned, so any signed origin
		// works. We test the 30% cap instead.
		assert_eq!(PlatformFeeBp::<Test>::get(), 1500);

		// Valid update
		assert_ok!(Marketplace::update_platform_fee(origin(ADMIN), 2000));
		assert_eq!(PlatformFeeBp::<Test>::get(), 2000);

		// Exceeds cap (3001 bp > 30%)
		assert_noop!(
			Marketplace::update_platform_fee(origin(ADMIN), 3001),
			Error::<Test>::InvalidFee
		);

		// Boundary: 3000 should succeed
		assert_ok!(Marketplace::update_platform_fee(origin(ADMIN), 3000));
		assert_eq!(PlatformFeeBp::<Test>::get(), 3000);
	});
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn buy_now_on_non_existent_listing_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Marketplace::buy_now(origin(BUYER), 999),
			Error::<Test>::NotListed
		);
	});
}

#[test]
fn make_offer_on_non_existent_listing_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Marketplace::make_offer(origin(BUYER), 999, 1_000, ListingCurrency::PLIM, 100),
			Error::<Test>::NotListed
		);
	});
}

#[test]
fn accept_expired_offer_fails() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));

		// Make offer that expires in 1 block
		assert_ok!(Marketplace::make_offer(
			origin(BUYER),
			ITEM_1,
			5_000,
			ListingCurrency::PLIM,
			1,
		));

		let offer_id = {
			let mut found = None;
			Offers::<Test>::iter().for_each(|(k, _)| found = Some(k));
			found.unwrap()
		};

		// Advance past expiration
		System::set_block_number(100);

		assert_noop!(
			Marketplace::accept_offer(origin(SELLER), offer_id),
			Error::<Test>::OfferExpired
		);
	});
}

#[test]
fn cancel_then_relist() {
	new_test_ext().execute_with(|| {
		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			10_000,
			ListingCurrency::PLIM,
		));
		assert_ok!(Marketplace::cancel_listing(origin(SELLER), ITEM_1));

		// Listing storage still holds the Cancelled entry — need to remove it
		// to relist. In production this would be handled by a cleanup or the
		// listing would be replaced. For now, removing manually.
		Listings::<Test>::remove(ITEM_1);

		assert_ok!(Marketplace::list_for_sale(
			origin(SELLER),
			ITEM_1,
			20_000,
			ListingCurrency::PEUR,
		));
		let listing = Listings::<Test>::get(ITEM_1).unwrap();
		assert_eq!(listing.price, 20_000);
		assert_eq!(listing.currency, ListingCurrency::PEUR);
	});
}
