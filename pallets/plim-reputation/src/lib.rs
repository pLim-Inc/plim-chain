//! # P:L:I:M:/Reputation Pallet
//!
//! Maintains on-chain reputation scores for accounts (agents and humans).
//! Score range: 0-1000, starts at 500.
//!
//! Signal weights:
//! - payment_ok   -> +10
//! - good_sla     -> +5
//! - dispute      -> -20
//! - violation    -> -50

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// ---------------------------------------------------------------------------
	// Constants
	// ---------------------------------------------------------------------------

	/// Default starting reputation score.
	pub const INITIAL_SCORE: u32 = 500;
	/// Maximum score.
	pub const MAX_SCORE: u32 = 1000;
	/// Minimum score.
	pub const MIN_SCORE: u32 = 0;

	// ---------------------------------------------------------------------------
	// Enums
	// ---------------------------------------------------------------------------

	/// Reputation signal types with their associated score deltas.
	#[derive(Clone, Copy, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum ReputationSignal {
		/// Successful payment completion (+10).
		PaymentOk,
		/// Good SLA adherence (+5).
		GoodSla,
		/// A dispute was raised against this account (-20).
		Dispute,
		/// A policy or compliance violation (-50).
		Violation,
	}

	impl ReputationSignal {
		/// Returns (delta, is_positive).
		pub fn delta(&self) -> (u32, bool) {
			match self {
				ReputationSignal::PaymentOk => (10, true),
				ReputationSignal::GoodSla => (5, true),
				ReputationSignal::Dispute => (20, false),
				ReputationSignal::Violation => (50, false),
			}
		}
	}

	// ---------------------------------------------------------------------------
	// Structs
	// ---------------------------------------------------------------------------

	/// A single reputation history entry.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct ReputationEntry<T: Config> {
		pub signal: ReputationSignal,
		pub score_after: u32,
		pub block: BlockNumberFor<T>,
	}

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin allowed to submit reputation signals (typically the payments pallet).
		type ReputationOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Maximum number of history entries per account.
		#[pallet::constant]
		type MaxHistoryPerAccount: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// Current reputation score for each account. Defaults to INITIAL_SCORE.
	#[pallet::storage]
	#[pallet::getter(fn reputation_scores)]
	pub type ReputationScores<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	/// Per-account reputation history (bounded FIFO).
	#[pallet::storage]
	#[pallet::getter(fn reputation_history)]
	pub type ReputationHistory<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<ReputationEntry<T>, T::MaxHistoryPerAccount>,
		ValueQuery,
	>;

	// ---------------------------------------------------------------------------
	// Genesis
	// ---------------------------------------------------------------------------

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// No genesis state needed; scores default to 0 in storage and are
			// initialised to INITIAL_SCORE on first signal via the helper.
		}
	}

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A reputation signal was applied.
		ReputationUpdated {
			account: T::AccountId,
			signal: ReputationSignal,
			new_score: u32,
		},
		/// A reputation score was queried (emitted for indexing).
		ReputationQueried {
			account: T::AccountId,
			score: u32,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Score overflow (should never happen due to clamping).
		ScoreOverflow,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Apply a reputation signal to an account. Restricted to `ReputationOrigin`
		/// (typically the payments pallet or governance).
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(25_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(2, 2)))]
		pub fn update_reputation(
			origin: OriginFor<T>,
			account: T::AccountId,
			signal: ReputationSignal,
		) -> DispatchResult {
			T::ReputationOrigin::ensure_origin(origin)?;

			let new_score = Self::apply_signal(&account, signal);

			Self::deposit_event(Event::ReputationUpdated {
				account,
				signal,
				new_score,
			});

			Ok(())
		}

		/// Query the current reputation score for an account. Emits an event
		/// so off-chain indexers can capture the read.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000_000, 0).saturating_add(T::DbWeight::get().reads(1)))]
		pub fn query_reputation(
			origin: OriginFor<T>,
			account: T::AccountId,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let score = Self::get_score(&account);

			Self::deposit_event(Event::ReputationQueried { account, score });

			Ok(())
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Get the current score for an account, returning INITIAL_SCORE if the
		/// account has never been scored.
		pub fn get_score(account: &T::AccountId) -> u32 {
			let stored = ReputationScores::<T>::get(account);
			if stored == 0 && !ReputationScores::<T>::contains_key(account) {
				INITIAL_SCORE
			} else {
				stored
			}
		}

		/// Apply a signal to the account's score, clamping to [MIN_SCORE, MAX_SCORE].
		fn apply_signal(account: &T::AccountId, signal: ReputationSignal) -> u32 {
			let current = Self::get_score(account);
			let (delta, positive) = signal.delta();

			let new_score = if positive {
				current.saturating_add(delta).min(MAX_SCORE)
			} else {
				current.saturating_sub(delta).max(MIN_SCORE)
			};

			ReputationScores::<T>::insert(account, new_score);

			// Append to history (FIFO eviction when full).
			let now = <frame_system::Pallet<T>>::block_number();
			ReputationHistory::<T>::mutate(account, |history| {
				let entry = ReputationEntry {
					signal,
					score_after: new_score,
					block: now,
				};
				if history.try_push(entry.clone()).is_err() {
					if !history.is_empty() {
						history.remove(0);
					}
					let _ = history.try_push(entry);
				}
			});

			new_score
		}
	}
}
