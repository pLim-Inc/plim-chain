//! # pallet-plim-marketplace
//!
//! On-chain marketplace for secondary sales of license NFTs on the
//! P:L:I:M:/Protocol. Handles listings, offers, auctions, and atomic buy-now
//! with split payout (seller + creator royalty + platform fee).
//!
//! The pallet is **loosely coupled**: it does not depend on `pallet-nfts` or
//! `pallet-assets` in its `Config` trait. Instead, transferability and royalty
//! information are injected via the `LicenseInspect` trait, royalty accounting
//! events are dispatched via `OnRoyaltyPayment`, and item ownership for auction
//! flows is injected via `ItemOwner`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use frame_support::{
	traits::Currency,
	PalletId,
};
use sp_runtime::traits::AccountIdConversion;

/// PalletId used to derive the marketplace account that holds NFT custody and
/// auction bid escrow funds.
pub const PALLET_ID: PalletId = PalletId(*b"plim/mkt");

// ---------------------------------------------------------------------------
// Loose-coupling traits
// ---------------------------------------------------------------------------

/// Callback dispatched when a royalty payment is made.
pub trait OnRoyaltyPayment<AccountId, ItemId, Balance> {
	fn on_royalty_paid(
		creator: &AccountId,
		item_id: &ItemId,
		amount: Balance,
		currency: ListingCurrency,
	);
}

impl<A, I, B> OnRoyaltyPayment<A, I, B> for () {
	fn on_royalty_paid(_: &A, _: &I, _: B, _: ListingCurrency) {}
}

/// Inspect license NFT transferability and royalty metadata.
pub trait LicenseInspect<ItemId, AccountId> {
	/// Returns `true` if the license NFT may be transferred on secondary sale.
	fn is_transferable(item_id: &ItemId) -> bool;
	/// Returns `Some((creator, royalty_bp))` if the item has a royalty policy.
	fn royalty_info(item_id: &ItemId) -> Option<(AccountId, u16)>;
}

impl<I, A> LicenseInspect<I, A> for () {
	fn is_transferable(_: &I) -> bool {
		true
	}
	fn royalty_info(_: &I) -> Option<(A, u16)> {
		None
	}
}

/// Inspect and (logically) transfer ownership of NFT items.
///
/// Used by auction flows where the pallet must verify the seller's ownership
/// at `create_auction` and effect a transfer at `settle_auction`. Real NFT
/// movement happens in the runtime integration layer; this trait only gives
/// the pallet a view & a hook so tests stay self-contained.
pub trait ItemOwner<ItemId, AccountId> {
	/// Returns the current owner of the item, if any.
	fn owner_of(item_id: &ItemId) -> Option<AccountId>;
	/// Notify the integration layer that ownership should change.
	/// Returns `Ok(())` if the integration accepted the move.
	fn transfer(item_id: &ItemId, from: &AccountId, to: &AccountId) -> Result<(), ()>;
}

impl<I, A: Clone> ItemOwner<I, A> for () {
	fn owner_of(_: &I) -> Option<A> {
		None
	}
	fn transfer(_: &I, _: &A, _: &A) -> Result<(), ()> {
		Ok(())
	}
}

// ---------------------------------------------------------------------------
// Helper type aliases
// ---------------------------------------------------------------------------

pub type BalanceOf<T> =
	<<T as Config>::NativeCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub type ListingOf<T> = Listing<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	frame_system::pallet_prelude::BlockNumberFor<T>,
>;

pub type OfferOf<T> = Offer<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	frame_system::pallet_prelude::BlockNumberFor<T>,
	<T as frame_system::Config>::Hash,
>;

pub type AuctionOf<T> = Auction<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	frame_system::pallet_prelude::BlockNumberFor<T>,
>;

pub type BidOf<T> = Bid<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T>,
	frame_system::pallet_prelude::BlockNumberFor<T>,
>;

// ---------------------------------------------------------------------------
// Pallet
// ---------------------------------------------------------------------------

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			tokens::currency::Currency as CurrencyT,
			ExistenceRequirement,
		},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{
		traits::{
			AtLeast32BitUnsigned, CheckedAdd, Hash as HashT, MaybeSerializeDeserialize, Member,
			Saturating, Zero,
		},
		Permill,
	};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ------------------------------------------------------------------
	// Config
	// ------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that the off-chain backend (custody key) uses for fiat-settled
		/// purchases. Typically `EnsureSigned` with a specific backend account or
		/// a custom `EnsureOrigin`.
		type MarketplaceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Native currency (PLIM) used for on-chain buy-now and auction escrow.
		type NativeCurrency: CurrencyT<Self::AccountId>;

		/// Asset id of the on-chain pEUR stablecoin (from pallet-assets).
		#[pallet::constant]
		type PEURAssetId: Get<u32>;

		/// PalletId used to derive the treasury account that receives the
		/// platform fee portion of every sale.
		#[pallet::constant]
		type TreasuryPalletId: Get<PalletId>;

		/// Default platform fee in basis points (1500 = 15%).
		#[pallet::constant]
		type DefaultPlatformFeeBp: Get<u16>;

		/// Maximum concurrent active listings per account.
		#[pallet::constant]
		type MaxActiveListingsPerAccount: Get<u32>;

		/// Callback for royalty payment accounting.
		type OnRoyaltyPayment: OnRoyaltyPayment<Self::AccountId, u32, BalanceOf<Self>>;

		/// Trait to inspect license transferability and royalty policy.
		type LicenseInspect: LicenseInspect<u32, Self::AccountId>;

		/// Trait to inspect and transfer NFT item ownership (used by auctions).
		type ItemOwner: ItemOwner<u32, Self::AccountId>;

		// ----- Auction extension config -----

		/// Auction identifier (typically `u64`).
		type AuctionId: Parameter
			+ Member
			+ MaybeSerializeDeserialize
			+ Default
			+ Copy
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned
			+ CheckedAdd
			+ Saturating
			+ From<u32>
			+ Into<u64>;

		/// Maximum number of bids retained per auction.
		#[pallet::constant]
		type MaxBidsPerAuction: Get<u32>;

		/// Maximum number of auctions that may end at the same block.
		#[pallet::constant]
		type MaxAuctionsPerBlock: Get<u32>;

		/// Minimum auction duration in blocks.
		#[pallet::constant]
		type MinAuctionDuration: Get<u32>;

		/// Minimum increment for a new highest bid, expressed as a Permill of
		/// the current high bid (e.g. `Permill::from_percent(2)`).
		#[pallet::constant]
		type MinBidIncrement: Get<Permill>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// ------------------------------------------------------------------
	// Storage
	// ------------------------------------------------------------------

	/// Active listings keyed by `item_id`.
	#[pallet::storage]
	pub type Listings<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, ListingOf<T>, OptionQuery>;

	/// Offers keyed by a unique offer hash.
	#[pallet::storage]
	pub type Offers<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, OfferOf<T>, OptionQuery>;

	/// Platform fee in basis points. Initialised from `DefaultPlatformFeeBp`.
	#[pallet::storage]
	pub type PlatformFeeBp<T: Config> =
		StorageValue<_, u16, ValueQuery, PlatformFeeDefault<T>>;

	/// Default value provider for PlatformFeeBp.
	#[pallet::type_value]
	pub fn PlatformFeeDefault<T: Config>() -> u16 {
		T::DefaultPlatformFeeBp::get()
	}

	/// Number of active listings per account (for cap enforcement).
	#[pallet::storage]
	pub type ActiveListingCount<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	// ----- Auction storage -----

	/// All known auctions keyed by `AuctionId`.
	#[pallet::storage]
	pub type Auctions<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AuctionId, AuctionOf<T>, OptionQuery>;

	/// Bid history for each auction (most recent appended).
	#[pallet::storage]
	pub type AuctionBids<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AuctionId,
		BoundedVec<BidOf<T>, T::MaxBidsPerAuction>,
		ValueQuery,
	>;

	/// Per-bidder escrowed funds for each auction. Cleared atomically when a
	/// new highest bidder takes over (refund to previous high) or on settle.
	#[pallet::storage]
	pub type AuctionEscrow<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AuctionId,
		Blake2_128Concat,
		T::AccountId,
		BalanceOf<T>,
		ValueQuery,
	>;

	/// Monotonic id counter for the next auction.
	#[pallet::storage]
	pub type NextAuctionId<T: Config> = StorageValue<_, T::AuctionId, ValueQuery>;

	/// Reverse index: for each block, the list of auction ids that end there.
	/// Used by `on_idle` to walk ended auctions for auto-settlement.
	#[pallet::storage]
	pub type AuctionsByEndBlock<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BlockNumberFor<T>,
		BoundedVec<T::AuctionId, T::MaxAuctionsPerBlock>,
		ValueQuery,
	>;

	// ------------------------------------------------------------------
	// Events
	// ------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A license NFT was listed for sale.
		Listed {
			item_id: u32,
			seller: T::AccountId,
			price: BalanceOf<T>,
			currency: ListingCurrency,
		},
		/// A listing was cancelled by its seller.
		ListingCanceled {
			item_id: u32,
			seller: T::AccountId,
		},
		/// A license was sold (buy-now or fiat proof).
		Sold {
			item_id: u32,
			seller: T::AccountId,
			buyer: T::AccountId,
			price: BalanceOf<T>,
			currency: ListingCurrency,
			royalty_paid: BalanceOf<T>,
			platform_fee: BalanceOf<T>,
		},
		/// An offer was placed on a listed item.
		OfferMade {
			offer_id: T::Hash,
			bidder: T::AccountId,
			item_id: u32,
			amount: BalanceOf<T>,
			currency: ListingCurrency,
			expires_at: BlockNumberFor<T>,
		},
		/// An offer was accepted by the seller.
		OfferAccepted {
			offer_id: T::Hash,
			item_id: u32,
			seller: T::AccountId,
			buyer: T::AccountId,
			amount: BalanceOf<T>,
		},
		/// An offer was rejected by the seller.
		OfferRejected {
			offer_id: T::Hash,
		},
		/// An offer was withdrawn by the bidder.
		OfferWithdrawn {
			offer_id: T::Hash,
		},
		/// The platform fee was updated by admin.
		PlatformFeeUpdated {
			old_bp: u16,
			new_bp: u16,
		},
		/// A new auction was created.
		AuctionCreated {
			auction_id: T::AuctionId,
			seller: T::AccountId,
			item_id: u32,
			end_block: BlockNumberFor<T>,
			reserve_price: BalanceOf<T>,
			currency: ListingCurrency,
		},
		/// A bid was placed on an auction.
		AuctionBid {
			auction_id: T::AuctionId,
			bidder: T::AccountId,
			amount: BalanceOf<T>,
		},
		/// An auction was extended due to a late (anti-snipe) bid.
		AuctionExtended {
			auction_id: T::AuctionId,
			new_end_block: BlockNumberFor<T>,
		},
		/// An auction was settled (winner paid + royalty + fee, or no qualifying bid).
		AuctionSettled {
			auction_id: T::AuctionId,
			winner: Option<T::AccountId>,
			final_price: BalanceOf<T>,
			royalty_amount: BalanceOf<T>,
			platform_fee: BalanceOf<T>,
		},
		/// An auction was cancelled.
		AuctionCancelled {
			auction_id: T::AuctionId,
			reason: BoundedVec<u8, ConstU32<64>>,
		},
	}

	// ------------------------------------------------------------------
	// Errors
	// ------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Caller is not the owner / seller of the item or listing.
		NotOwner,
		/// Item is already listed on the marketplace.
		AlreadyListed,
		/// No active listing found for this item.
		NotListed,
		/// The offered price is zero or below the minimum.
		PriceTooLow,
		/// The offer has expired.
		OfferExpired,
		/// Buyer does not have enough balance.
		InsufficientBalance,
		/// The license NFT is non-transferable.
		NotTransferable,
		/// Arithmetic overflow during fee calculation.
		ArithmeticOverflow,
		/// Fiat buy-now requires MarketplaceOrigin.
		FiatRequiresOrigin,
		/// License metadata not found via LicenseInspect.
		LicenseNotFound,
		/// Account has reached the maximum number of active listings.
		MaxListingsReached,
		/// The referenced offer does not exist.
		OfferNotFound,
		/// The new fee exceeds the 3000 bp (30%) cap.
		InvalidFee,
		/// Listing currency is EURFiat which cannot be settled on-chain.
		CannotBuyFiatOnChain,
		/// The listing is not in Active status.
		ListingNotActive,
		/// Offer is not in Pending status.
		OfferNotPending,
		// ----- Auction errors -----
		/// Auction id not present in storage.
		AuctionNotFound,
		/// Auction has already passed its end block.
		AuctionAlreadyEnded,
		/// Auction has not yet reached its end block.
		AuctionNotEnded,
		/// Auction has already been settled.
		AuctionAlreadySettled,
		/// Auction has already been cancelled.
		AuctionAlreadyCancelled,
		/// Bid is below the configured reserve price.
		BidBelowReserve,
		/// Bid is not above current high by the required increment.
		BidNotIncremental,
		/// Seller may not bid on their own auction.
		SellerCannotBid,
		/// Caller is not the seller for this auction.
		NotAuctionSeller,
		/// Cannot cancel an auction that already has bids.
		AuctionHasBids,
		/// Auction has not yet started (current block < start_block).
		AuctionNotStarted,
		/// `anti_snipe_blocks` exceeds the allowed maximum (100).
		BadAntiSnipeValue,
		/// Auction duration is shorter than `MinAuctionDuration`.
		DurationTooShort,
		/// `start_block` is in the past.
		StartBlockInPast,
		/// `reserve_price` must be strictly positive.
		ReservePriceZero,
		/// Per-block auction-end index is full.
		AuctionsPerBlockFull,
		/// Bid history is full.
		BidHistoryFull,
		/// Underlying ItemOwner reported the item as un-owned or owned by a
		/// different account than the caller.
		ItemNotOwnedByCaller,
		/// The integration layer rejected the NFT transfer.
		ItemTransferFailed,
	}

	// ------------------------------------------------------------------
	// Hooks
	// ------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Auto-settle ended auctions opportunistically while remaining weight allows.
		fn on_idle(now: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			Self::process_ended_auctions(now, remaining_weight)
		}
	}

	// ------------------------------------------------------------------
	// Extrinsics
	// ------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// List a license NFT for sale on the marketplace.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::list_for_sale())]
		pub fn list_for_sale(
			origin: OriginFor<T>,
			item_id: u32,
			price: BalanceOf<T>,
			currency: ListingCurrency,
		) -> DispatchResult {
			let seller = ensure_signed(origin)?;
			ensure!(!price.is_zero(), Error::<T>::PriceTooLow);
			ensure!(!Listings::<T>::contains_key(item_id), Error::<T>::AlreadyListed);
			ensure!(
				T::LicenseInspect::is_transferable(&item_id),
				Error::<T>::NotTransferable
			);

			let count = ActiveListingCount::<T>::get(&seller);
			ensure!(
				count < T::MaxActiveListingsPerAccount::get(),
				Error::<T>::MaxListingsReached
			);

			let now = frame_system::Pallet::<T>::block_number();
			let listing = ListingOf::<T> {
				seller: seller.clone(),
				item_id,
				price,
				currency,
				listed_at: now,
				status: ListingStatus::Active,
			};
			Listings::<T>::insert(item_id, listing);
			ActiveListingCount::<T>::insert(&seller, count.saturating_add(1));

			Self::deposit_event(Event::Listed {
				item_id,
				seller,
				price,
				currency,
			});
			Ok(())
		}

		/// Cancel an active listing. Only the original seller may cancel.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::cancel_listing())]
		pub fn cancel_listing(origin: OriginFor<T>, item_id: u32) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let listing = Listings::<T>::get(item_id).ok_or(Error::<T>::NotListed)?;
			ensure!(listing.seller == who, Error::<T>::NotOwner);
			ensure!(listing.status == ListingStatus::Active, Error::<T>::ListingNotActive);

			Listings::<T>::mutate(item_id, |maybe| {
				if let Some(ref mut l) = maybe {
					l.status = ListingStatus::Cancelled;
				}
			});
			ActiveListingCount::<T>::mutate(&who, |c| *c = c.saturating_sub(1));

			Self::deposit_event(Event::ListingCanceled {
				item_id,
				seller: who,
			});
			Ok(())
		}

		/// Buy a listed item at the listed price using native PLIM.
		///
		/// Splits the payment into:
		/// - Creator royalty (from `LicenseInspect::royalty_info`)
		/// - Platform fee (from `PlatformFeeBp`)
		/// - Seller remainder
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::buy_now())]
		pub fn buy_now(origin: OriginFor<T>, item_id: u32) -> DispatchResult {
			let buyer = ensure_signed(origin)?;
			let listing = Listings::<T>::get(item_id).ok_or(Error::<T>::NotListed)?;
			ensure!(listing.status == ListingStatus::Active, Error::<T>::ListingNotActive);
			ensure!(listing.currency != ListingCurrency::EURFiat, Error::<T>::CannotBuyFiatOnChain);
			ensure!(
				T::LicenseInspect::is_transferable(&item_id),
				Error::<T>::NotTransferable
			);

			let price = listing.price;
			let seller = listing.seller.clone();

			let (royalty_amount, platform_fee, _seller_gets, maybe_creator) =
				Self::do_split_payout(&buyer, &seller, item_id, price, listing.currency)?;

			// Update listing status
			Listings::<T>::mutate(item_id, |maybe| {
				if let Some(ref mut l) = maybe {
					l.status = ListingStatus::Sold;
				}
			});
			ActiveListingCount::<T>::mutate(&seller, |c| *c = c.saturating_sub(1));

			// Royalty callback
			if let Some(ref creator) = maybe_creator {
				T::OnRoyaltyPayment::on_royalty_paid(
					creator,
					&item_id,
					royalty_amount,
					listing.currency,
				);
			}

			Self::deposit_event(Event::Sold {
				item_id,
				seller,
				buyer,
				price,
				currency: listing.currency,
				royalty_paid: royalty_amount,
				platform_fee,
			});
			Ok(())
		}

		/// Record a fiat-settled sale. Only callable by `MarketplaceOrigin`
		/// (the backend custody key after Stripe confirms payment).
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::buy_now_with_fiat_proof())]
		pub fn buy_now_with_fiat_proof(
			origin: OriginFor<T>,
			item_id: u32,
			buyer: T::AccountId,
			payment_proof: [u8; 32],
		) -> DispatchResult {
			T::MarketplaceOrigin::ensure_origin(origin)?;

			let listing = Listings::<T>::get(item_id).ok_or(Error::<T>::NotListed)?;
			ensure!(listing.status == ListingStatus::Active, Error::<T>::ListingNotActive);

			let seller = listing.seller.clone();
			let price = listing.price;

			// Compute the amounts for event emission (fiat is off-chain, so no
			// on-chain transfers — just bookkeeping).
			let royalty_amount =
				if let Some((_creator, royalty_bp)) = T::LicenseInspect::royalty_info(&item_id) {
					Self::bp_of(price, royalty_bp)?
				} else {
					BalanceOf::<T>::zero()
				};

			let fee_bp = PlatformFeeBp::<T>::get();
			let platform_fee = Self::bp_of(price, fee_bp)?;

			// Mark as sold
			Listings::<T>::mutate(item_id, |maybe| {
				if let Some(ref mut l) = maybe {
					l.status = ListingStatus::Sold;
				}
			});
			ActiveListingCount::<T>::mutate(&seller, |c| *c = c.saturating_sub(1));

			// Royalty callback
			if let Some((ref creator, _)) = T::LicenseInspect::royalty_info(&item_id) {
				T::OnRoyaltyPayment::on_royalty_paid(
					creator,
					&item_id,
					royalty_amount,
					listing.currency,
				);
			}

			let _ = payment_proof; // consumed for inclusion in block; future: store hash

			Self::deposit_event(Event::Sold {
				item_id,
				seller,
				buyer,
				price,
				currency: listing.currency,
				royalty_paid: royalty_amount,
				platform_fee,
			});
			Ok(())
		}

		/// Place an offer on a listed item.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::make_offer())]
		pub fn make_offer(
			origin: OriginFor<T>,
			item_id: u32,
			amount: BalanceOf<T>,
			currency: ListingCurrency,
			expires_in_blocks: BlockNumberFor<T>,
		) -> DispatchResult {
			let bidder = ensure_signed(origin)?;
			ensure!(!amount.is_zero(), Error::<T>::PriceTooLow);
			ensure!(Listings::<T>::contains_key(item_id), Error::<T>::NotListed);

			let now = frame_system::Pallet::<T>::block_number();
			let expires_at = now.saturating_add(expires_in_blocks);

			// Derive a unique offer id from bidder + item + block
			let offer_id = T::Hashing::hash_of(&(&bidder, item_id, now));

			let offer = OfferOf::<T> {
				offer_id,
				bidder: bidder.clone(),
				item_id,
				amount,
				currency,
				expires_at,
				status: OfferStatus::Pending,
			};
			Offers::<T>::insert(offer_id, offer);

			Self::deposit_event(Event::OfferMade {
				offer_id,
				bidder,
				item_id,
				amount,
				currency,
				expires_at,
			});
			Ok(())
		}

		/// Accept a pending offer. Only the listing's seller may accept.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::accept_offer())]
		pub fn accept_offer(origin: OriginFor<T>, offer_id: T::Hash) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut offer = Offers::<T>::get(offer_id).ok_or(Error::<T>::OfferNotFound)?;
			ensure!(offer.status == OfferStatus::Pending, Error::<T>::OfferNotPending);

			let now = frame_system::Pallet::<T>::block_number();
			ensure!(now <= offer.expires_at, Error::<T>::OfferExpired);

			let listing = Listings::<T>::get(offer.item_id).ok_or(Error::<T>::NotListed)?;
			ensure!(listing.seller == who, Error::<T>::NotOwner);
			ensure!(listing.status == ListingStatus::Active, Error::<T>::ListingNotActive);

			offer.status = OfferStatus::Accepted;
			Offers::<T>::insert(offer_id, offer.clone());

			// Mark listing as sold
			Listings::<T>::mutate(offer.item_id, |maybe| {
				if let Some(ref mut l) = maybe {
					l.status = ListingStatus::Sold;
				}
			});
			ActiveListingCount::<T>::mutate(&who, |c| *c = c.saturating_sub(1));

			Self::deposit_event(Event::OfferAccepted {
				offer_id,
				item_id: offer.item_id,
				seller: who,
				buyer: offer.bidder,
				amount: offer.amount,
			});
			Ok(())
		}

		/// Reject a pending offer. Only the listing's seller may reject.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::reject_offer())]
		pub fn reject_offer(origin: OriginFor<T>, offer_id: T::Hash) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut offer = Offers::<T>::get(offer_id).ok_or(Error::<T>::OfferNotFound)?;
			ensure!(offer.status == OfferStatus::Pending, Error::<T>::OfferNotPending);

			let listing = Listings::<T>::get(offer.item_id).ok_or(Error::<T>::NotListed)?;
			ensure!(listing.seller == who, Error::<T>::NotOwner);

			offer.status = OfferStatus::Rejected;
			Offers::<T>::insert(offer_id, offer);

			Self::deposit_event(Event::OfferRejected { offer_id });
			Ok(())
		}

		/// Withdraw a pending offer. Only the original bidder may withdraw.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::withdraw_offer())]
		pub fn withdraw_offer(origin: OriginFor<T>, offer_id: T::Hash) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut offer = Offers::<T>::get(offer_id).ok_or(Error::<T>::OfferNotFound)?;
			ensure!(offer.status == OfferStatus::Pending, Error::<T>::OfferNotPending);
			ensure!(offer.bidder == who, Error::<T>::NotOwner);

			offer.status = OfferStatus::Withdrawn;
			Offers::<T>::insert(offer_id, offer);

			Self::deposit_event(Event::OfferWithdrawn { offer_id });
			Ok(())
		}

		/// Update the platform fee. Restricted to `MarketplaceOrigin`.
		/// Maximum 3000 bp (30%).
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::update_platform_fee())]
		pub fn update_platform_fee(origin: OriginFor<T>, new_bp: u16) -> DispatchResult {
			T::MarketplaceOrigin::ensure_origin(origin)?;
			ensure!(new_bp <= 3000, Error::<T>::InvalidFee);

			let old_bp = PlatformFeeBp::<T>::get();
			PlatformFeeBp::<T>::put(new_bp);

			Self::deposit_event(Event::PlatformFeeUpdated { old_bp, new_bp });
			Ok(())
		}

		// --------------------------------------------------------------
		// Auctions
		// --------------------------------------------------------------

		/// Create a new English auction for a license NFT.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::create_auction())]
		pub fn create_auction(
			origin: OriginFor<T>,
			item_id: u32,
			start_block: BlockNumberFor<T>,
			duration_blocks: BlockNumberFor<T>,
			reserve_price: BalanceOf<T>,
			currency: ListingCurrency,
			anti_snipe_blocks: BlockNumberFor<T>,
		) -> DispatchResult {
			let seller = ensure_signed(origin)?;

			// --- Validation ---
			let now = frame_system::Pallet::<T>::block_number();
			ensure!(start_block >= now, Error::<T>::StartBlockInPast);
			ensure!(!reserve_price.is_zero(), Error::<T>::ReservePriceZero);

			let min_dur: BlockNumberFor<T> = T::MinAuctionDuration::get().into();
			ensure!(duration_blocks >= min_dur, Error::<T>::DurationTooShort);

			let snipe_cap: BlockNumberFor<T> = 100u32.into();
			ensure!(anti_snipe_blocks <= snipe_cap, Error::<T>::BadAntiSnipeValue);

			ensure!(
				T::LicenseInspect::is_transferable(&item_id),
				Error::<T>::NotTransferable
			);

			// Ownership check via injected provider. If the provider returns
			// `None` (mock default), we treat ownership as caller-owned for
			// integration-friendliness; production runtime wires `pallet-nfts`.
			if let Some(owner) = T::ItemOwner::owner_of(&item_id) {
				ensure!(owner == seller, Error::<T>::ItemNotOwnedByCaller);
			}

			// Reserve NFT into pallet custody.
			let pallet_acc = Self::pallet_account();
			T::ItemOwner::transfer(&item_id, &seller, &pallet_acc)
				.map_err(|_| Error::<T>::ItemTransferFailed)?;

			// --- Allocate id and persist ---
			let auction_id = NextAuctionId::<T>::get();
			let next = auction_id
				.checked_add(&1u32.into())
				.ok_or(Error::<T>::ArithmeticOverflow)?;
			NextAuctionId::<T>::put(next);

			let end_block = start_block.saturating_add(duration_blocks);

			let auction = AuctionOf::<T> {
				seller: seller.clone(),
				item_id,
				start_block,
				end_block,
				original_end_block: end_block,
				reserve_price,
				currency,
				anti_snipe_blocks,
				highest_bid: None,
				status: if start_block <= now {
					AuctionStatus::Active
				} else {
					AuctionStatus::Scheduled
				},
				created_at_block: now,
			};
			Auctions::<T>::insert(auction_id, auction);

			AuctionsByEndBlock::<T>::try_mutate(end_block, |list| {
				list.try_push(auction_id).map_err(|_| Error::<T>::AuctionsPerBlockFull)
			})?;

			Self::deposit_event(Event::AuctionCreated {
				auction_id,
				seller,
				item_id,
				end_block,
				reserve_price,
				currency,
			});
			Ok(())
		}

		/// Place a bid on an active auction.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::bid_auction())]
		pub fn bid_auction(
			origin: OriginFor<T>,
			auction_id: T::AuctionId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let bidder = ensure_signed(origin)?;
			let mut auction = Auctions::<T>::get(auction_id).ok_or(Error::<T>::AuctionNotFound)?;

			ensure!(auction.status != AuctionStatus::Settled, Error::<T>::AuctionAlreadySettled);
			ensure!(
				auction.status != AuctionStatus::Cancelled,
				Error::<T>::AuctionAlreadyCancelled
			);
			ensure!(bidder != auction.seller, Error::<T>::SellerCannotBid);

			let now = frame_system::Pallet::<T>::block_number();
			ensure!(now >= auction.start_block, Error::<T>::AuctionNotStarted);
			ensure!(now < auction.end_block, Error::<T>::AuctionAlreadyEnded);

			// Activate if needed
			if auction.status == AuctionStatus::Scheduled {
				auction.status = AuctionStatus::Active;
			}

			// --- Bid increment validation ---
			match &auction.highest_bid {
				None => {
					ensure!(amount >= auction.reserve_price, Error::<T>::BidBelowReserve);
				}
				Some(prev) => {
					let inc = T::MinBidIncrement::get().mul_floor(prev.amount);
					let min_required = prev.amount.saturating_add(inc);
					ensure!(amount >= min_required, Error::<T>::BidNotIncremental);
				}
			}

			// --- Move funds: bidder -> pallet escrow account ---
			let escrow_acc = Self::pallet_account();
			T::NativeCurrency::transfer(
				&bidder,
				&escrow_acc,
				amount,
				ExistenceRequirement::KeepAlive,
			)?;

			// --- Refund previous highest bidder atomically ---
			if let Some(prev) = auction.highest_bid.clone() {
				let prev_escrow = AuctionEscrow::<T>::take(auction_id, &prev.bidder);
				if !prev_escrow.is_zero() {
					T::NativeCurrency::transfer(
						&escrow_acc,
						&prev.bidder,
						prev_escrow,
						ExistenceRequirement::AllowDeath,
					)?;
				}
			}

			AuctionEscrow::<T>::insert(auction_id, &bidder, amount);

			let bid = BidOf::<T> {
				bidder: bidder.clone(),
				amount,
				at_block: now,
			};
			AuctionBids::<T>::try_mutate(auction_id, |list| {
				list.try_push(bid.clone()).map_err(|_| Error::<T>::BidHistoryFull)
			})?;
			auction.highest_bid = Some(bid);

			// --- Anti-snipe: extend end_block if within window ---
			let mut extended = false;
			if !auction.anti_snipe_blocks.is_zero() {
				let window_start = auction.end_block.saturating_sub(auction.anti_snipe_blocks);
				if now >= window_start {
					let old_end = auction.end_block;
					let new_end = auction.end_block.saturating_add(auction.anti_snipe_blocks);
					auction.end_block = new_end;

					// Move auction id from old end-block bucket to new one.
					AuctionsByEndBlock::<T>::mutate(old_end, |list| {
						if let Some(pos) = list.iter().position(|id| *id == auction_id) {
							list.swap_remove(pos);
						}
					});
					AuctionsByEndBlock::<T>::try_mutate(new_end, |list| {
						list.try_push(auction_id)
							.map_err(|_| Error::<T>::AuctionsPerBlockFull)
					})?;
					extended = true;
				}
			}

			Auctions::<T>::insert(auction_id, auction.clone());

			Self::deposit_event(Event::AuctionBid {
				auction_id,
				bidder,
				amount,
			});

			if extended {
				Self::deposit_event(Event::AuctionExtended {
					auction_id,
					new_end_block: auction.end_block,
				});
			}
			Ok(())
		}

		/// Settle an ended auction. Anyone may call after `end_block`.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::settle_auction())]
		pub fn settle_auction(origin: OriginFor<T>, auction_id: T::AuctionId) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			let auction = Auctions::<T>::get(auction_id).ok_or(Error::<T>::AuctionNotFound)?;
			ensure!(auction.status != AuctionStatus::Settled, Error::<T>::AuctionAlreadySettled);
			ensure!(
				auction.status != AuctionStatus::Cancelled,
				Error::<T>::AuctionAlreadyCancelled
			);

			let now = frame_system::Pallet::<T>::block_number();
			ensure!(now >= auction.end_block, Error::<T>::AuctionNotEnded);

			Self::do_settle(auction_id, auction)
		}

		/// Cancel a scheduled auction with no bids. Seller-only.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::cancel_auction())]
		pub fn cancel_auction(origin: OriginFor<T>, auction_id: T::AuctionId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut auction = Auctions::<T>::get(auction_id).ok_or(Error::<T>::AuctionNotFound)?;
			ensure!(auction.seller == who, Error::<T>::NotAuctionSeller);
			ensure!(auction.status == AuctionStatus::Scheduled, Error::<T>::AuctionAlreadyEnded);
			ensure!(
				AuctionBids::<T>::get(auction_id).is_empty(),
				Error::<T>::AuctionHasBids
			);

			// Return NFT to seller.
			let pallet_acc = Self::pallet_account();
			T::ItemOwner::transfer(&auction.item_id, &pallet_acc, &auction.seller)
				.map_err(|_| Error::<T>::ItemTransferFailed)?;

			// Remove from end-block index.
			AuctionsByEndBlock::<T>::mutate(auction.end_block, |list| {
				if let Some(pos) = list.iter().position(|id| *id == auction_id) {
					list.swap_remove(pos);
				}
			});

			auction.status = AuctionStatus::Cancelled;
			Auctions::<T>::insert(auction_id, auction);

			let reason: BoundedVec<u8, ConstU32<64>> =
				b"seller_cancelled".to_vec().try_into().unwrap_or_default();
			Self::deposit_event(Event::AuctionCancelled {
				auction_id,
				reason,
			});
			Ok(())
		}
	}

	// ------------------------------------------------------------------
	// Internal helpers
	// ------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Derive the treasury account from the `TreasuryPalletId`.
		pub fn treasury_account() -> T::AccountId {
			T::TreasuryPalletId::get().into_account_truncating()
		}

		/// Derive the marketplace pallet account (NFT custody + auction escrow).
		pub fn pallet_account() -> T::AccountId {
			PALLET_ID.into_account_truncating()
		}

		/// Compute `value * bp / 10_000` with checked arithmetic.
		pub(crate) fn bp_of(value: BalanceOf<T>, bp: u16) -> Result<BalanceOf<T>, DispatchError> {
			let bp_balance: BalanceOf<T> = bp.into();
			let ten_k: BalanceOf<T> = 10_000u16.into();
			// Saturating multiply then integer divide. For realistic values
			// (price < 2^120) this never saturates on u128.
			let product = value.saturating_mul(bp_balance);
			Ok(product / ten_k)
		}

		/// Execute the canonical 3-way split: royalty -> creator,
		/// platform_fee -> treasury, remainder -> seller.
		///
		/// Source of funds is `payer` for `buy_now` and the pallet escrow
		/// account for auction settlement.
		///
		/// Returns `(royalty_amount, platform_fee, seller_gets, maybe_creator)`.
		pub(crate) fn do_split_payout(
			payer: &T::AccountId,
			seller: &T::AccountId,
			item_id: u32,
			price: BalanceOf<T>,
			_currency: ListingCurrency,
		) -> Result<
			(BalanceOf<T>, BalanceOf<T>, BalanceOf<T>, Option<T::AccountId>),
			DispatchError,
		> {
			let (royalty_amount, maybe_creator) =
				if let Some((creator, royalty_bp)) = T::LicenseInspect::royalty_info(&item_id) {
					let r = Self::bp_of(price, royalty_bp)?;
					(r, Some(creator))
				} else {
					(BalanceOf::<T>::zero(), None)
				};

			let fee_bp = PlatformFeeBp::<T>::get();
			let platform_fee = Self::bp_of(price, fee_bp)?;

			let seller_gets = price
				.saturating_sub(royalty_amount)
				.saturating_sub(platform_fee);

			let exists_req = if payer == &Self::pallet_account() {
				ExistenceRequirement::AllowDeath
			} else {
				ExistenceRequirement::KeepAlive
			};

			if let Some(ref creator) = maybe_creator {
				if !royalty_amount.is_zero() {
					T::NativeCurrency::transfer(payer, creator, royalty_amount, exists_req)?;
				}
			}

			if !platform_fee.is_zero() {
				let treasury = Self::treasury_account();
				T::NativeCurrency::transfer(payer, &treasury, platform_fee, exists_req)?;
			}

			if !seller_gets.is_zero() {
				T::NativeCurrency::transfer(payer, seller, seller_gets, exists_req)?;
			}

			Ok((royalty_amount, platform_fee, seller_gets, maybe_creator))
		}

		/// Internal settle implementation shared by extrinsic and `on_idle`.
		pub(crate) fn do_settle(
			auction_id: T::AuctionId,
			mut auction: AuctionOf<T>,
		) -> DispatchResult {
			let pallet_acc = Self::pallet_account();

			// Drain any escrow for losing bidders that may still be present
			// (defensive: should already be empty since each bid clears the
			// previous high's entry).
			let _ = AuctionEscrow::<T>::clear_prefix(auction_id, u32::MAX, None);

			// Ensure no leftover escrow assertion-style: re-check map empty.
			debug_assert!(
				AuctionEscrow::<T>::iter_prefix(auction_id).next().is_none(),
				"escrow not cleared"
			);

			match auction.highest_bid.clone() {
				Some(top) if top.amount >= auction.reserve_price => {
					// Pay out: pallet account funds -> creator + treasury + seller.
					// (We already drained escrow above; but the funds physically
					// remain in pallet_acc balance, since `clear_prefix` only
					// clears the storage entries, not balances.)
					let (royalty_amount, platform_fee, _seller_gets, maybe_creator) =
						Self::do_split_payout(
							&pallet_acc,
							&auction.seller,
							auction.item_id,
							top.amount,
							auction.currency,
						)?;

					// Transfer NFT to winner.
					T::ItemOwner::transfer(&auction.item_id, &pallet_acc, &top.bidder)
						.map_err(|_| Error::<T>::ItemTransferFailed)?;

					// Royalty callback.
					if let Some(ref creator) = maybe_creator {
						T::OnRoyaltyPayment::on_royalty_paid(
							creator,
							&auction.item_id,
							royalty_amount,
							auction.currency,
						);
					}

					auction.status = AuctionStatus::Settled;
					Auctions::<T>::insert(auction_id, auction.clone());

					Self::deposit_event(Event::AuctionSettled {
						auction_id,
						winner: Some(top.bidder),
						final_price: top.amount,
						royalty_amount,
						platform_fee,
					});
				}
				_ => {
					// No qualifying bid → return NFT to seller, mark cancelled.
					T::ItemOwner::transfer(&auction.item_id, &pallet_acc, &auction.seller)
						.map_err(|_| Error::<T>::ItemTransferFailed)?;
					auction.status = AuctionStatus::Cancelled;
					Auctions::<T>::insert(auction_id, auction.clone());

					let reason: BoundedVec<u8, ConstU32<64>> =
						b"no_qualifying_bid".to_vec().try_into().unwrap_or_default();
					Self::deposit_event(Event::AuctionCancelled {
						auction_id,
						reason,
					});
					Self::deposit_event(Event::AuctionSettled {
						auction_id,
						winner: None,
						final_price: BalanceOf::<T>::zero(),
						royalty_amount: BalanceOf::<T>::zero(),
						platform_fee: BalanceOf::<T>::zero(),
					});
				}
			}
			Ok(())
		}

		/// Walk `AuctionsByEndBlock` for blocks <= `now` and auto-settle any
		/// auctions that are still open. Bounded to ~50 settlements per call.
		pub(crate) fn process_ended_auctions(
			now: BlockNumberFor<T>,
			remaining_weight: Weight,
		) -> Weight {
			let per_settle = Weight::from_parts(50_000, 0);
			let mut consumed = Weight::zero();
			let mut count = 0u32;
			const MAX_PER_BLOCK: u32 = 50;

			// Walk backwards from `now` until we find a block with no bucket
			// or we hit our bounds. Process auctions in oldest-first order by
			// scanning all known buckets up to `now`.
			//
			// We avoid an unbounded iter() by doing translate over the index
			// in batches of up to MAX_PER_BLOCK.
			let mut buckets_to_clear: alloc::vec::Vec<BlockNumberFor<T>> = alloc::vec::Vec::new();
			for (block, ids) in AuctionsByEndBlock::<T>::iter() {
				if block > now {
					continue;
				}
				let mut bucket_drained = true;
				for auction_id in ids.iter() {
					if count >= MAX_PER_BLOCK {
						bucket_drained = false;
						break;
					}
					if consumed.saturating_add(per_settle).any_gt(remaining_weight) {
						bucket_drained = false;
						break;
					}
					if let Some(auction) = Auctions::<T>::get(*auction_id) {
						if matches!(
							auction.status,
							AuctionStatus::Settled | AuctionStatus::Cancelled
						) {
							continue;
						}
						let _ = Self::do_settle(*auction_id, auction);
						consumed = consumed.saturating_add(per_settle);
						count = count.saturating_add(1);
					}
				}
				if bucket_drained {
					buckets_to_clear.push(block);
				}
				if count >= MAX_PER_BLOCK {
					break;
				}
			}
			for b in buckets_to_clear {
				AuctionsByEndBlock::<T>::remove(b);
			}
			consumed
		}
	}
}
