//! # pallet-plim-marketplace
//!
//! On-chain marketplace for secondary sales of license NFTs on the
//! P:L:I:M:/Protocol. Handles listings, offers, and atomic buy-now with
//! split payout (seller + creator royalty + platform fee).
//!
//! The pallet is **loosely coupled**: it does not depend on `pallet-nfts` or
//! `pallet-assets` in its `Config` trait. Instead, transferability and royalty
//! information are injected via the `LicenseInspect` trait, and royalty
//! accounting events are dispatched via `OnRoyaltyPayment`.

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
	use sp_runtime::traits::{Hash as HashT, Saturating, Zero};

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

		/// Native currency (PLIM) used for on-chain buy-now.
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

			// Compute royalty
			let (royalty_amount, maybe_creator) =
				if let Some((creator, royalty_bp)) = T::LicenseInspect::royalty_info(&item_id) {
					let r = Self::bp_of(price, royalty_bp)?;
					(r, Some(creator))
				} else {
					(BalanceOf::<T>::zero(), None)
				};

			// Compute platform fee
			let fee_bp = PlatformFeeBp::<T>::get();
			let platform_fee = Self::bp_of(price, fee_bp)?;

			// Seller remainder (saturating to avoid underflow)
			let seller_gets = price
				.saturating_sub(royalty_amount)
				.saturating_sub(platform_fee);

			// Transfer royalty to creator
			if let Some(ref creator) = maybe_creator {
				if !royalty_amount.is_zero() {
					T::NativeCurrency::transfer(
						&buyer,
						creator,
						royalty_amount,
						ExistenceRequirement::KeepAlive,
					)?;
				}
			}

			// Transfer platform fee to treasury
			if !platform_fee.is_zero() {
				let treasury = Self::treasury_account();
				T::NativeCurrency::transfer(
					&buyer,
					&treasury,
					platform_fee,
					ExistenceRequirement::KeepAlive,
				)?;
			}

			// Transfer remainder to seller
			if !seller_gets.is_zero() {
				T::NativeCurrency::transfer(
					&buyer,
					&seller,
					seller_gets,
					ExistenceRequirement::KeepAlive,
				)?;
			}

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
	}

	// ------------------------------------------------------------------
	// Internal helpers
	// ------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Derive the treasury account from the `TreasuryPalletId`.
		pub fn treasury_account() -> T::AccountId {
			T::TreasuryPalletId::get().into_account_truncating()
		}

		/// Compute `value * bp / 10_000` with checked arithmetic.
		fn bp_of(value: BalanceOf<T>, bp: u16) -> Result<BalanceOf<T>, DispatchError> {
			// Convert bp to Balance
			let bp_balance: BalanceOf<T> = bp.into();
			let ten_k: BalanceOf<T> = 10_000u16.into();
			let product = value
				.checked_mul(&bp_balance)
				.ok_or(Error::<T>::ArithmeticOverflow)?;
			Ok(product / ten_k)
		}
	}
}
