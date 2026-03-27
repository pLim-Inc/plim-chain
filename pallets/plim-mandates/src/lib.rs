#![cfg_attr(not(feature = "std"), no_std)]

//! # pallet-plim-mandates
//!
//! Peter Todd Single-Use Seals for Plim Protocol.
//!
//! A seal is a cryptographic commitment that closes EXACTLY ONCE.
//! Once closed, a seal can NEVER be reopened — this is the mathematical
//! double-spend guarantee at the heart of P:L:I:M:/Protocol.

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	extern crate alloc;

	use alloc::vec::Vec;
	use codec::{Encode, MaxEncodedLen};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use scale_info::TypeInfo;
	use frame_support::sp_runtime::traits::Hash as _;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Balance type for seal constraints (max_amount).
		type Balance: Parameter
			+ Member
			+ Default
			+ Copy
			+ MaxEncodedLen
			+ TypeInfo;

		/// Maximum number of seals that can be created in a single recurring chain.
		#[pallet::constant]
		type MaxChainLength: Get<u32>;
	}

	// ──────────────────────────────────────────────────────────────
	// Types
	// ──────────────────────────────────────────────────────────────

	/// Whether a seal is open (awaiting closure) or irrevocably closed.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum SealStatus {
		Open,
		Closed,
	}

	/// Constraints governing what a seal may authorise.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct SealConstraints<T: Config> {
		/// Upper bound on the value this seal can commit to.
		pub max_amount: T::Balance,
		/// Optional block number after which the seal expires and cannot be closed.
		pub expires_at: Option<BlockNumberFor<T>>,
	}

	/// Full on-chain state of a single-use seal.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct SealState<T: Config> {
		/// Account that owns (and may close) this seal.
		pub owner: T::AccountId,
		/// `None` while open; `Some(hash)` once irrevocably closed.
		pub commitment: Option<T::Hash>,
		/// Current status — transitions Open -> Closed exactly once.
		pub status: SealStatus,
		/// Constraints that were fixed at creation time.
		pub constraints: SealConstraints<T>,
		/// Block in which the seal was created.
		pub created_at: BlockNumberFor<T>,
		/// Block in which the seal was closed (`None` while open).
		pub closed_at: Option<BlockNumberFor<T>>,
		/// Optional pointer to the next seal in a recurring chain.
		pub next_seal: Option<T::Hash>,
	}

	// ──────────────────────────────────────────────────────────────
	// Storage
	// ──────────────────────────────────────────────────────────────

	/// Primary seal registry.  seal_id (Hash) -> SealState
	#[pallet::storage]
	#[pallet::getter(fn seals)]
	pub type Seals<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, SealState<T>, OptionQuery>;

	/// Chain linkage: seal_id -> next_seal_id (if any).
	#[pallet::storage]
	#[pallet::getter(fn seal_chains)]
	pub type SealChains<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, Option<T::Hash>, OptionQuery>;

	/// Monotonic nonce used to derive unique seal IDs per account.
	#[pallet::storage]
	pub type SealNonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	// ──────────────────────────────────────────────────────────────
	// Events
	// ──────────────────────────────────────────────────────────────

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new single-use seal has been created.
		SealCreated {
			seal_id: T::Hash,
			owner: T::AccountId,
		},
		/// A seal has been irrevocably closed with a commitment.
		SealClosed {
			seal_id: T::Hash,
			message_hash: T::Hash,
		},
		/// A chain of N linked recurring seals has been created.
		RecurringChainCreated {
			first_seal_id: T::Hash,
			count: u32,
		},
	}

	// ──────────────────────────────────────────────────────────────
	// Errors
	// ──────────────────────────────────────────────────────────────

	#[pallet::error]
	pub enum Error<T> {
		/// The referenced seal does not exist in storage.
		SealNotFound,
		/// Caller is not the owner of this seal.
		NotSealOwner,
		/// Cannot close a seal that has already been closed.
		SealAlreadyClosed,
		/// The seal's expiry block has passed; it can no longer be closed.
		SealExpired,
		/// The provided next_seal reference is invalid (e.g. does not exist).
		InvalidSealChain,
		/// The requested chain length exceeds `MaxChainLength`.
		ChainTooLong,
	}

	// ──────────────────────────────────────────────────────────────
	// Extrinsics
	// ──────────────────────────────────────────────────────────────

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new open single-use seal.
		///
		/// The seal is identified by a deterministic hash derived from
		/// the caller's account and a global nonce, guaranteeing uniqueness.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(2))]
		pub fn create_seal(
			origin: OriginFor<T>,
			constraints: SealConstraints<T>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let seal_id = Self::next_seal_id(&owner);

			let now = <frame_system::Pallet<T>>::block_number();

			let state = SealState {
				owner: owner.clone(),
				commitment: None,
				status: SealStatus::Open,
				constraints,
				created_at: now,
				closed_at: None,
				next_seal: None,
			};

			<Seals<T>>::insert(&seal_id, state);
			<SealChains<T>>::insert(&seal_id, None::<T::Hash>);

			Self::deposit_event(Event::SealCreated { seal_id, owner });
			Ok(())
		}

		/// Irrevocably close a seal with a message commitment.
		///
		/// Once closed a seal can **never** be reopened — this is the
		/// fundamental single-use property that prevents double-spends.
		///
		/// Optionally links to a `next_seal` to form a recurring chain.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(15_000, 0) + T::DbWeight::get().reads_writes(2, 2))]
		pub fn close_seal(
			origin: OriginFor<T>,
			seal_id: T::Hash,
			message_hash: T::Hash,
			next_seal: Option<T::Hash>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			<Seals<T>>::try_mutate(&seal_id, |maybe_state| -> DispatchResult {
				let state = maybe_state.as_mut().ok_or(Error::<T>::SealNotFound)?;

				// --- Guards ---
				ensure!(state.owner == caller, Error::<T>::NotSealOwner);
				ensure!(state.status == SealStatus::Open, Error::<T>::SealAlreadyClosed);

				// Check expiry if one was set.
				if let Some(expires_at) = state.constraints.expires_at {
					let now = <frame_system::Pallet<T>>::block_number();
					ensure!(now <= expires_at, Error::<T>::SealExpired);
				}

				// If a next_seal is provided, it must already exist and be Open.
				if let Some(ref ns) = next_seal {
					let ns_state = <Seals<T>>::get(ns).ok_or(Error::<T>::InvalidSealChain)?;
					ensure!(ns_state.status == SealStatus::Open, Error::<T>::InvalidSealChain);
				}

				// --- Irreversible state transition ---
				let now = <frame_system::Pallet<T>>::block_number();
				state.commitment = Some(message_hash);
				state.status = SealStatus::Closed;
				state.closed_at = Some(now);
				state.next_seal = next_seal;

				// Update chain linkage.
				<SealChains<T>>::insert(&seal_id, &state.next_seal);

				Ok(())
			})?;

			Self::deposit_event(Event::SealClosed { seal_id, message_hash });
			Ok(())
		}

		/// Create a chain of `count` linked recurring seals in one call.
		///
		/// Each seal's `next_seal` field points to the subsequently created
		/// seal, forming a forward-linked list. All seals start Open.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000u64.saturating_mul(*count as u64), 0)
			+ T::DbWeight::get().writes((*count as u64) * 2))]
		pub fn create_recurring_chain(
			origin: OriginFor<T>,
			count: u32,
			constraints: SealConstraints<T>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;

			ensure!(count >= 1, Error::<T>::InvalidSealChain);
			ensure!(count <= T::MaxChainLength::get(), Error::<T>::ChainTooLong);

			let now = <frame_system::Pallet<T>>::block_number();

			// Pre-generate all seal IDs so we can link them forward.
			let mut seal_ids = Vec::with_capacity(count as usize);
			for _ in 0..count {
				seal_ids.push(Self::next_seal_id(&owner));
			}

			let first_seal_id = seal_ids[0];

			// Write seals in forward-linked order.
			for i in 0..(count as usize) {
				let next = if i + 1 < count as usize {
					Some(seal_ids[i + 1])
				} else {
					None
				};

				let state = SealState {
					owner: owner.clone(),
					commitment: None,
					status: SealStatus::Open,
					constraints: constraints.clone(),
					created_at: now,
					closed_at: None,
					next_seal: next,
				};

				<Seals<T>>::insert(&seal_ids[i], state);
				<SealChains<T>>::insert(&seal_ids[i], next);
			}

			Self::deposit_event(Event::RecurringChainCreated {
				first_seal_id,
				count,
			});
			Ok(())
		}
	}

	// ──────────────────────────────────────────────────────────────
	// Internal helpers
	// ──────────────────────────────────────────────────────────────

	impl<T: Config> Pallet<T> {
		/// Derive a unique, deterministic seal ID from the owner and a
		/// monotonically increasing global nonce.
		fn next_seal_id(owner: &T::AccountId) -> T::Hash {
			let nonce = <SealNonce<T>>::get();
			<SealNonce<T>>::put(nonce.wrapping_add(1));
			T::Hashing::hash_of(&(owner, nonce))
		}
	}
}
