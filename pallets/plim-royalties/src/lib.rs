//! # pallet-plim-royalties
//!
//! Royalty tracking and claiming for the P:L:I:M:/Protocol.
//!
//! Tracks accumulated royalties per creator per currency, allows creators to
//! claim their accumulated royalties, and enforces platform fee configuration.
//!
//! Implements the `OnRoyaltyPayment` trait that `pallet-plim-marketplace`
//! (or any upstream pallet) calls when a secondary sale generates a royalty.
//!
//! ## Settlement paths
//!
//! | Currency  | Settlement                                      |
//! |-----------|-------------------------------------------------|
//! | PLIM      | On-chain via `NativeCurrency::deposit_into_existing` |
//! | PEUR      | Off-chain — event emitted for indexer           |
//! | EURFiat   | Off-chain — event emitted for indexer           |

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::traits::Currency;

/// Balance type derived from the configured NativeCurrency.
pub type BalanceOf<T> =
	<<T as Config>::NativeCurrency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

// ---------------------------------------------------------------------------
// OnRoyaltyPayment trait — exported for marketplace / upstream pallets
// ---------------------------------------------------------------------------

/// Callback trait invoked when a royalty payment is generated (e.g. on a
/// secondary NFT sale). Implementors accumulate the royalty for later claiming.
pub trait OnRoyaltyPayment<AccountId, ItemId, Balance> {
	fn on_royalty_paid(
		creator: &AccountId,
		item_id: &ItemId,
		amount: Balance,
		currency: RoyaltyCurrency,
	);
}

/// No-op implementation for runtimes / tests that don't wire royalties.
impl<A, I, B> OnRoyaltyPayment<A, I, B> for () {
	fn on_royalty_paid(_creator: &A, _item_id: &I, _amount: B, _currency: RoyaltyCurrency) {}
}

// ---------------------------------------------------------------------------
// Pallet definition
// ---------------------------------------------------------------------------

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Zero;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---- Config ----

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that may configure platform treasury and fees (root / council).
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The native currency (PLIM). Used for on-chain royalty settlement.
		type NativeCurrency: Currency<Self::AccountId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// ---- Storage ----

	/// Accumulated (unclaimed) royalties per creator per currency.
	#[pallet::storage]
	pub type AccumulatedRoyalties<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat, T::AccountId,
		Blake2_128Concat, RoyaltyCurrency,
		BalanceOf<T>,
		ValueQuery,
	>;

	/// Individual royalty payment history, keyed by a deterministic payment id.
	#[pallet::storage]
	pub type RoyaltyHistory<T: Config> = StorageMap<
		_,
		Blake2_128Concat, T::Hash,
		RoyaltyPayment<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Lifetime total royalties paid out across all creators and currencies.
	#[pallet::storage]
	pub type TotalRoyaltiesPaid<T: Config> =
		StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Platform treasury account that receives the platform fee cut.
	#[pallet::storage]
	pub type PlatformTreasury<T: Config> =
		StorageValue<_, T::AccountId, OptionQuery>;

	/// Platform fee expressed in basis points (1 bp = 0.01%). Max 3000 (30%).
	#[pallet::storage]
	pub type PlatformFeeBp<T: Config> =
		StorageValue<_, u16, ValueQuery>;

	/// Running count of royalty payment events.
	#[pallet::storage]
	pub type PaymentCount<T: Config> =
		StorageValue<_, u32, ValueQuery>;

	// ---- Events ----

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A royalty payment was recorded and accumulated for a creator.
		RoyaltyAccumulated {
			creator: T::AccountId,
			item_id: u32,
			amount: BalanceOf<T>,
			currency: RoyaltyCurrency,
		},
		/// A creator claimed their accumulated royalties.
		RoyaltyClaimed {
			creator: T::AccountId,
			amount: BalanceOf<T>,
			currency: RoyaltyCurrency,
		},
		/// The platform treasury account was updated.
		PlatformTreasuryUpdated {
			old: Option<T::AccountId>,
			new: T::AccountId,
		},
		/// The platform fee (basis points) was updated.
		PlatformFeeUpdated {
			old_bp: u16,
			new_bp: u16,
		},
	}

	// ---- Errors ----

	#[pallet::error]
	pub enum Error<T> {
		/// The creator has no accumulated royalties for the requested currency.
		NoAccumulatedRoyalties,
		/// The fee exceeds the maximum of 3000 basis points (30%).
		InvalidFee,
		/// The platform treasury account has not been configured.
		TreasuryNotSet,
		/// The caller is not authorised to perform this action.
		Unauthorized,
	}

	// ---- Genesis ----

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Optional treasury account to set at genesis.
		pub platform_treasury: Option<T::AccountId>,
		/// Default platform fee in basis points (capped at 3000).
		pub default_fee_bp: u16,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(ref treasury) = self.platform_treasury {
				PlatformTreasury::<T>::put(treasury);
			}
			// Silently cap at 3000 bp during genesis to prevent misconfiguration.
			let fee = if self.default_fee_bp > 3000 { 3000 } else { self.default_fee_bp };
			PlatformFeeBp::<T>::put(fee);
		}
	}

	// ---- Extrinsics ----

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Claim accumulated royalties for a given currency.
		///
		/// - For **PLIM**: transfers native tokens to the caller via
		///   `NativeCurrency::deposit_into_existing`.
		/// - For **PEUR** / **EURFiat**: emits a `RoyaltyClaimed` event only;
		///   actual settlement happens off-chain.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_accumulated_royalties())]
		pub fn claim_accumulated_royalties(
			origin: OriginFor<T>,
			currency: RoyaltyCurrency,
		) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			let amount = AccumulatedRoyalties::<T>::get(&creator, &currency);
			ensure!(!amount.is_zero(), Error::<T>::NoAccumulatedRoyalties);

			// For PLIM: on-chain deposit. PEUR/EURFiat: event-only.
			if currency == RoyaltyCurrency::PLIM {
				// deposit_into_existing will fail if the account doesn't exist,
				// which is fine — a creator must have an existential deposit.
				let _imbalance = T::NativeCurrency::deposit_into_existing(&creator, amount)
					.map_err(|_| Error::<T>::NoAccumulatedRoyalties)?;
			}

			// Reset accumulated balance to zero.
			AccumulatedRoyalties::<T>::insert(&creator, &currency, BalanceOf::<T>::zero());

			Self::deposit_event(Event::RoyaltyClaimed {
				creator,
				amount,
				currency,
			});

			Ok(())
		}

		/// Set the platform treasury account. Admin-only.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_platform_treasury())]
		pub fn set_platform_treasury(
			origin: OriginFor<T>,
			new: T::AccountId,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			let old = PlatformTreasury::<T>::get();
			PlatformTreasury::<T>::put(&new);

			Self::deposit_event(Event::PlatformTreasuryUpdated { old, new });

			Ok(())
		}

		/// Update the platform fee in basis points. Admin-only. Max 3000 (30%).
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::update_platform_fee())]
		pub fn update_platform_fee(
			origin: OriginFor<T>,
			new_bp: u16,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			ensure!(new_bp <= 3000, Error::<T>::InvalidFee);

			let old_bp = PlatformFeeBp::<T>::get();
			PlatformFeeBp::<T>::put(new_bp);

			Self::deposit_event(Event::PlatformFeeUpdated { old_bp, new_bp });

			Ok(())
		}
	}
}

// ---------------------------------------------------------------------------
// Public API for runtime bridges
// ---------------------------------------------------------------------------

impl<T: Config> Pallet<T> {
	/// Called by runtime bridge adapters to record a royalty payment.
	/// This delegates to the internal `on_royalty_paid` logic.
	pub fn record_royalty_payment(
		creator: &T::AccountId,
		item_id: &u32,
		amount: BalanceOf<T>,
		currency: RoyaltyCurrency,
	) {
		<Self as OnRoyaltyPayment<T::AccountId, u32, BalanceOf<T>>>::on_royalty_paid(
			creator, item_id, amount, currency,
		);
	}
}

// ---------------------------------------------------------------------------
// OnRoyaltyPayment implementation for Pallet<T>
// ---------------------------------------------------------------------------

impl<T: Config> OnRoyaltyPayment<T::AccountId, u32, BalanceOf<T>> for Pallet<T> {
	fn on_royalty_paid(
		creator: &T::AccountId,
		item_id: &u32,
		amount: BalanceOf<T>,
		currency: RoyaltyCurrency,
	) {
		use sp_runtime::traits::{Hash, Saturating};

		// 1. Accumulate unclaimed royalties.
		AccumulatedRoyalties::<T>::mutate(creator, &currency, |acc| {
			*acc = acc.saturating_add(amount);
		});

		// 2. Record in history.
		let now = <frame_system::Pallet<T>>::block_number();
		let payment_id = T::Hashing::hash_of(&(creator, item_id, now));
		RoyaltyHistory::<T>::insert(
			&payment_id,
			RoyaltyPayment {
				creator: creator.clone(),
				item_id: *item_id,
				amount,
				currency: currency.clone(),
				block_number: now,
				claimed: false,
			},
		);

		// 3. Update lifetime totals.
		TotalRoyaltiesPaid::<T>::mutate(|t| *t = t.saturating_add(amount));
		PaymentCount::<T>::mutate(|c| *c = c.saturating_add(1));

		// 4. Emit event.
		Self::deposit_event(Event::RoyaltyAccumulated {
			creator: creator.clone(),
			item_id: *item_id,
			amount,
			currency,
		});
	}
}
