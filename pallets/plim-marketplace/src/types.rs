//! Types for pallet-plim-marketplace.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;

/// Currency denomination for a marketplace listing or offer.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub enum ListingCurrency {
	/// Off-chain EUR (Stripe / bank). On-chain settlement uses `buy_now_with_fiat_proof`.
	EURFiat,
	/// Native PLIM token.
	PLIM,
	/// On-chain EUR stablecoin (pallet-assets).
	PEUR,
}

/// Status of a marketplace listing.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub enum ListingStatus {
	Active,
	Sold,
	Cancelled,
}

/// Status of an offer on a listed item.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub enum OfferStatus {
	Pending,
	Accepted,
	Rejected,
	Expired,
	Withdrawn,
}

/// A marketplace listing for a license NFT.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct Listing<AccountId, Balance, BlockNumber> {
	pub seller: AccountId,
	pub item_id: u32,
	pub price: Balance,
	pub currency: ListingCurrency,
	pub listed_at: BlockNumber,
	pub status: ListingStatus,
}

/// An offer (bid) on a listed item.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct Offer<AccountId, Balance, BlockNumber, Hash> {
	pub offer_id: Hash,
	pub bidder: AccountId,
	pub item_id: u32,
	pub amount: Balance,
	pub currency: ListingCurrency,
	pub expires_at: BlockNumber,
	pub status: OfferStatus,
}
