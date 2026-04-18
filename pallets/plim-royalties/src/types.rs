//! Types for pallet-plim-royalties.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Currency denomination for royalty payments.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub enum RoyaltyCurrency {
	/// Native PLIM token — on-chain settlement via NativeCurrency.
	PLIM,
	/// pEUR stablecoin — event-only, off-chain settlement.
	PEUR,
	/// EUR fiat — event-only, off-chain settlement.
	EURFiat,
}

/// A single royalty payment record stored on-chain.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, PartialEq, Eq, Debug)]
pub struct RoyaltyPayment<AccountId, Balance, BlockNumber> {
	/// The creator (royalty recipient).
	pub creator: AccountId,
	/// The item / NFT id that generated this royalty.
	pub item_id: u32,
	/// The royalty amount (gross, before platform fee).
	pub amount: Balance,
	/// The currency denomination.
	pub currency: RoyaltyCurrency,
	/// The block at which this payment was recorded.
	pub block_number: BlockNumber,
	/// Whether the creator has claimed this payment.
	pub claimed: bool,
}
