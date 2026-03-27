//! # P:L:I:M:/Channels Pallet
//!
//! State-channel support for off-chain micro-payments between two parties.
//! Channels hold reserved deposits on-chain and can be cooperatively closed,
//! disputed, or settled after a timeout.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ReservableCurrency},
	};
	use frame_system::pallet_prelude::*;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	// ---------------------------------------------------------------------------
	// Enums
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum ChannelStatus {
		Open,
		Closing,
		Settled,
	}

	impl Default for ChannelStatus {
		fn default() -> Self {
			ChannelStatus::Open
		}
	}

	// ---------------------------------------------------------------------------
	// Structs
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct ChannelState<T: Config> {
		pub party_a: T::AccountId,
		pub party_b: T::AccountId,
		pub deposit_a: BalanceOf<T>,
		pub deposit_b: BalanceOf<T>,
		pub nonce: u64,
		pub status: ChannelStatus,
		pub ttl: BlockNumberFor<T>,
		pub dispute_period: BlockNumberFor<T>,
		pub created_at: BlockNumberFor<T>,
	}

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency mechanism used for channel deposits.
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

		/// Default dispute period in blocks.
		#[pallet::constant]
		type DefaultDisputePeriod: Get<BlockNumberFor<Self>>;

		/// Maximum channel TTL in blocks.
		#[pallet::constant]
		type MaxChannelTTL: Get<BlockNumberFor<Self>>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// Active channels indexed by their unique hash.
	#[pallet::storage]
	#[pallet::getter(fn channels)]
	pub type Channels<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, ChannelState<T>, OptionQuery>;

	/// Per-channel nonce counter used to prevent replay attacks.
	#[pallet::storage]
	#[pallet::getter(fn channel_nonces)]
	pub type ChannelNonces<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, u64, ValueQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A payment channel was opened. [channel_id, party_a, party_b, deposit_a, deposit_b]
		ChannelOpened {
			channel_id: T::Hash,
			party_a: T::AccountId,
			party_b: T::AccountId,
			deposit_a: BalanceOf<T>,
			deposit_b: BalanceOf<T>,
		},
		/// A cooperative close was initiated. [channel_id, final_balance_a, final_balance_b]
		ChannelClosed {
			channel_id: T::Hash,
			final_balance_a: BalanceOf<T>,
			final_balance_b: BalanceOf<T>,
		},
		/// A dispute was raised on a channel. [channel_id, disputer, nonce]
		ChannelDisputed {
			channel_id: T::Hash,
			disputer: T::AccountId,
			nonce: u64,
		},
		/// A channel was settled after dispute period. [channel_id]
		ChannelSettled {
			channel_id: T::Hash,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Channel already exists.
		ChannelAlreadyExists,
		/// Channel does not exist.
		ChannelNotFound,
		/// Only channel parties can perform this action.
		NotChannelParty,
		/// Channel is not in the required status.
		InvalidChannelStatus,
		/// The provided nonce is not higher than the current nonce.
		StaleNonce,
		/// Dispute period has not elapsed yet.
		DisputePeriodNotElapsed,
		/// Channel TTL exceeds maximum allowed.
		TTLExceedsMax,
		/// Insufficient balance to fund channel deposit.
		InsufficientBalance,
		/// Invalid balance split: final balances exceed total deposit.
		InvalidBalanceSplit,
		/// Dispute period has already elapsed; channel can be settled.
		DisputePeriodElapsed,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Open a new payment channel between the caller (party_a) and party_b.
		///
		/// Both parties must have sufficient reservable balance. `deposit_b` is
		/// reserved from `party_b`'s account (party_b must have pre-approved or
		/// this is called via a multi-sig / delegation flow).
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(50_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(2, 3)))]
		pub fn open_channel(
			origin: OriginFor<T>,
			party_b: T::AccountId,
			deposit_a: BalanceOf<T>,
			deposit_b: BalanceOf<T>,
			ttl: BlockNumberFor<T>,
		) -> DispatchResult {
			let party_a = ensure_signed(origin)?;

			ensure!(ttl <= T::MaxChannelTTL::get(), Error::<T>::TTLExceedsMax);

			// Derive a unique channel ID from both parties and the current block.
			let now = <frame_system::Pallet<T>>::block_number();
			let channel_id = T::Hashing::hash_of(&(&party_a, &party_b, &now));

			ensure!(
				!Channels::<T>::contains_key(&channel_id),
				Error::<T>::ChannelAlreadyExists
			);

			// Reserve deposits from both parties.
			T::Currency::reserve(&party_a, deposit_a)
				.map_err(|_| Error::<T>::InsufficientBalance)?;
			T::Currency::reserve(&party_b, deposit_b)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			let dispute_period = T::DefaultDisputePeriod::get();

			let state = ChannelState {
				party_a: party_a.clone(),
				party_b: party_b.clone(),
				deposit_a,
				deposit_b,
				nonce: 0u64,
				status: ChannelStatus::Open,
				ttl,
				dispute_period,
				created_at: now,
			};

			Channels::<T>::insert(&channel_id, state);
			ChannelNonces::<T>::insert(&channel_id, 0u64);

			Self::deposit_event(Event::ChannelOpened {
				channel_id,
				party_a,
				party_b,
				deposit_a,
				deposit_b,
			});

			Ok(())
		}

		/// Cooperatively close a channel. Both parties agree on the final balance
		/// split. The caller must be one of the channel parties.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(40_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 2)))]
		pub fn close_channel(
			origin: OriginFor<T>,
			channel_id: T::Hash,
			final_balance_a: BalanceOf<T>,
			final_balance_b: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Channels::<T>::try_mutate(&channel_id, |maybe_channel| -> DispatchResult {
				let channel = maybe_channel.as_mut().ok_or(Error::<T>::ChannelNotFound)?;

				ensure!(
					who == channel.party_a || who == channel.party_b,
					Error::<T>::NotChannelParty
				);
				ensure!(
					channel.status == ChannelStatus::Open,
					Error::<T>::InvalidChannelStatus
				);

				// Validate the balance split does not exceed total deposits.
				let total = channel.deposit_a.saturating_add(channel.deposit_b);
				ensure!(
					final_balance_a.saturating_add(final_balance_b) <= total,
					Error::<T>::InvalidBalanceSplit
				);

				// Unreserve original deposits.
				T::Currency::unreserve(&channel.party_a, channel.deposit_a);
				T::Currency::unreserve(&channel.party_b, channel.deposit_b);

				// Update channel state.
				channel.status = ChannelStatus::Settled;

				Self::deposit_event(Event::ChannelClosed {
					channel_id,
					final_balance_a,
					final_balance_b,
				});

				Ok(())
			})?;

			Ok(())
		}

		/// Dispute a channel by submitting a higher-nonce state. Moves the channel
		/// into `Closing` status and starts the dispute period timer.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(35_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(2, 2)))]
		pub fn dispute_channel(
			origin: OriginFor<T>,
			channel_id: T::Hash,
			nonce: u64,
			final_balance_a: BalanceOf<T>,
			final_balance_b: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Channels::<T>::try_mutate(&channel_id, |maybe_channel| -> DispatchResult {
				let channel = maybe_channel.as_mut().ok_or(Error::<T>::ChannelNotFound)?;

				ensure!(
					who == channel.party_a || who == channel.party_b,
					Error::<T>::NotChannelParty
				);
				ensure!(
					channel.status == ChannelStatus::Open
						|| channel.status == ChannelStatus::Closing,
					Error::<T>::InvalidChannelStatus
				);

				let current_nonce = ChannelNonces::<T>::get(&channel_id);
				ensure!(nonce > current_nonce, Error::<T>::StaleNonce);

				let total = channel.deposit_a.saturating_add(channel.deposit_b);
				ensure!(
					final_balance_a.saturating_add(final_balance_b) <= total,
					Error::<T>::InvalidBalanceSplit
				);

				// Update nonce and move to Closing.
				ChannelNonces::<T>::insert(&channel_id, nonce);
				channel.nonce = nonce;
				channel.status = ChannelStatus::Closing;

				// Record the TTL as dispute deadline from now.
				let now = <frame_system::Pallet<T>>::block_number();
				channel.ttl = now.saturating_add(channel.dispute_period);

				Self::deposit_event(Event::ChannelDisputed {
					channel_id,
					disputer: who,
					nonce,
				});

				Ok(())
			})?;

			Ok(())
		}

		/// Settle a disputed channel after the dispute period has elapsed.
		/// Anyone can call this to finalize a channel in `Closing` status.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(30_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 2)))]
		pub fn settle_channel(
			origin: OriginFor<T>,
			channel_id: T::Hash,
		) -> DispatchResult {
			ensure_signed(origin)?;

			Channels::<T>::try_mutate(&channel_id, |maybe_channel| -> DispatchResult {
				let channel = maybe_channel.as_mut().ok_or(Error::<T>::ChannelNotFound)?;

				ensure!(
					channel.status == ChannelStatus::Closing,
					Error::<T>::InvalidChannelStatus
				);

				let now = <frame_system::Pallet<T>>::block_number();
				ensure!(now >= channel.ttl, Error::<T>::DisputePeriodNotElapsed);

				// Unreserve all deposits (balance distribution happens off-chain
				// or via a separate settlement extrinsic with signed state).
				T::Currency::unreserve(&channel.party_a, channel.deposit_a);
				T::Currency::unreserve(&channel.party_b, channel.deposit_b);

				channel.status = ChannelStatus::Settled;

				Self::deposit_event(Event::ChannelSettled { channel_id });

				Ok(())
			})?;

			Ok(())
		}
	}
}
