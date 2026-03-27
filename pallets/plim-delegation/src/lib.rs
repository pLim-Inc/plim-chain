//! # P:L:I:M:/Delegation Pallet
//!
//! Manages spending policies that allow AI agents to act on behalf of human
//! principals within configurable financial limits. Includes a kill switch for
//! immediate agent suspension.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// ---------------------------------------------------------------------------
	// Enums
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum DelegationStatus {
		Active,
		Paused,
		Expired,
	}

	impl Default for DelegationStatus {
		fn default() -> Self {
			DelegationStatus::Active
		}
	}

	// ---------------------------------------------------------------------------
	// Structs
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct SpendingPolicy<T: Config> {
		/// The principal (human) who created this delegation.
		pub principal: T::AccountId,
		/// The agent account that can spend on behalf of the principal.
		pub agent_id: T::AccountId,
		/// Maximum amount per single transaction.
		pub max_per_tx: u128,
		/// Maximum daily aggregate spend.
		pub max_daily: u128,
		/// Maximum monthly aggregate spend.
		pub max_monthly: u128,
		/// Transactions above this amount require human approval.
		pub human_approval_threshold: u128,
		/// Block number from which the delegation is valid.
		pub valid_from: BlockNumberFor<T>,
		/// Block number at which the delegation expires.
		pub valid_to: BlockNumberFor<T>,
		/// Current status of this delegation.
		pub status: DelegationStatus,
		/// Block at which this policy was created.
		pub created_at: BlockNumberFor<T>,
	}

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum number of active delegations a single agent can hold.
		#[pallet::constant]
		type MaxDelegationsPerAgent: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// Spending policies indexed by a unique delegation hash.
	#[pallet::storage]
	#[pallet::getter(fn delegations)]
	pub type Delegations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, SpendingPolicy<T>, OptionQuery>;

	/// Lookup from an agent account to its list of delegation IDs.
	#[pallet::storage]
	#[pallet::getter(fn agent_delegations)]
	pub type AgentDelegations<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<T::Hash, T::MaxDelegationsPerAgent>,
		ValueQuery,
	>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new delegation was created.
		DelegationCreated {
			delegation_id: T::Hash,
			principal: T::AccountId,
			agent_id: T::AccountId,
		},
		/// A delegation policy was updated.
		PolicyUpdated {
			delegation_id: T::Hash,
		},
		/// A delegation was permanently revoked.
		DelegationRevoked {
			delegation_id: T::Hash,
		},
		/// An agent was paused (kill switch).
		AgentPaused {
			delegation_id: T::Hash,
			agent_id: T::AccountId,
		},
		/// A previously paused agent was resumed.
		AgentResumed {
			delegation_id: T::Hash,
			agent_id: T::AccountId,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Delegation does not exist.
		DelegationNotFound,
		/// Caller is not the principal of this delegation.
		NotPrincipal,
		/// Delegation has already been revoked or expired.
		DelegationInactive,
		/// Agent has reached the maximum number of delegations.
		TooManyDelegations,
		/// `valid_from` must be before `valid_to`.
		InvalidValidityRange,
		/// Agent is not currently paused.
		AgentNotPaused,
		/// Agent is already paused.
		AgentAlreadyPaused,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new spending delegation for an AI agent.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(40_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(2, 3)))]
		pub fn create_delegation(
			origin: OriginFor<T>,
			agent_id: T::AccountId,
			max_per_tx: u128,
			max_daily: u128,
			max_monthly: u128,
			human_approval_threshold: u128,
			valid_from: BlockNumberFor<T>,
			valid_to: BlockNumberFor<T>,
		) -> DispatchResult {
			let principal = ensure_signed(origin)?;

			ensure!(valid_from < valid_to, Error::<T>::InvalidValidityRange);

			let now = <frame_system::Pallet<T>>::block_number();
			let delegation_id = T::Hashing::hash_of(&(&principal, &agent_id, &now));

			let policy = SpendingPolicy {
				principal: principal.clone(),
				agent_id: agent_id.clone(),
				max_per_tx,
				max_daily,
				max_monthly,
				human_approval_threshold,
				valid_from,
				valid_to,
				status: DelegationStatus::Active,
				created_at: now,
			};

			Delegations::<T>::insert(&delegation_id, policy);

			AgentDelegations::<T>::try_mutate(&agent_id, |ids| -> DispatchResult {
				ids.try_push(delegation_id)
					.map_err(|_| Error::<T>::TooManyDelegations)?;
				Ok(())
			})?;

			Self::deposit_event(Event::DelegationCreated {
				delegation_id,
				principal,
				agent_id,
			});

			Ok(())
		}

		/// Update the spending limits of an existing delegation.
		/// Only the principal who created the delegation may call this.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(30_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn update_policy(
			origin: OriginFor<T>,
			delegation_id: T::Hash,
			max_per_tx: u128,
			max_daily: u128,
			max_monthly: u128,
			human_approval_threshold: u128,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Delegations::<T>::try_mutate(&delegation_id, |maybe_policy| -> DispatchResult {
				let policy = maybe_policy.as_mut().ok_or(Error::<T>::DelegationNotFound)?;
				ensure!(who == policy.principal, Error::<T>::NotPrincipal);
				ensure!(
					policy.status == DelegationStatus::Active,
					Error::<T>::DelegationInactive
				);

				policy.max_per_tx = max_per_tx;
				policy.max_daily = max_daily;
				policy.max_monthly = max_monthly;
				policy.human_approval_threshold = human_approval_threshold;

				Self::deposit_event(Event::PolicyUpdated { delegation_id });
				Ok(())
			})?;

			Ok(())
		}

		/// Permanently revoke a delegation. Cannot be undone.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(30_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn revoke_delegation(
			origin: OriginFor<T>,
			delegation_id: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Delegations::<T>::try_mutate(&delegation_id, |maybe_policy| -> DispatchResult {
				let policy = maybe_policy.as_mut().ok_or(Error::<T>::DelegationNotFound)?;
				ensure!(who == policy.principal, Error::<T>::NotPrincipal);
				ensure!(
					policy.status != DelegationStatus::Expired,
					Error::<T>::DelegationInactive
				);

				policy.status = DelegationStatus::Expired;

				Self::deposit_event(Event::DelegationRevoked { delegation_id });
				Ok(())
			})?;

			Ok(())
		}

		/// KILL SWITCH: Immediately pause an agent's delegation.
		/// Blocks all spending until `resume_agent` is called.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(25_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn pause_agent(
			origin: OriginFor<T>,
			delegation_id: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Delegations::<T>::try_mutate(&delegation_id, |maybe_policy| -> DispatchResult {
				let policy = maybe_policy.as_mut().ok_or(Error::<T>::DelegationNotFound)?;
				ensure!(who == policy.principal, Error::<T>::NotPrincipal);
				ensure!(
					policy.status == DelegationStatus::Active,
					Error::<T>::AgentAlreadyPaused
				);

				policy.status = DelegationStatus::Paused;

				Self::deposit_event(Event::AgentPaused {
					delegation_id,
					agent_id: policy.agent_id.clone(),
				});
				Ok(())
			})?;

			Ok(())
		}

		/// Resume a previously paused agent delegation.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(25_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn resume_agent(
			origin: OriginFor<T>,
			delegation_id: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Delegations::<T>::try_mutate(&delegation_id, |maybe_policy| -> DispatchResult {
				let policy = maybe_policy.as_mut().ok_or(Error::<T>::DelegationNotFound)?;
				ensure!(who == policy.principal, Error::<T>::NotPrincipal);
				ensure!(
					policy.status == DelegationStatus::Paused,
					Error::<T>::AgentNotPaused
				);

				policy.status = DelegationStatus::Active;

				Self::deposit_event(Event::AgentResumed {
					delegation_id,
					agent_id: policy.agent_id.clone(),
				});
				Ok(())
			})?;

			Ok(())
		}
	}
}
