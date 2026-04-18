//! # pallet-plim-kyc
//!
//! KYC whitelist used by `pallet-rwa` to gate share ownership and transfers
//! of real-world-asset tokens on the P:L:I:M:/Protocol chain.
//!
//! Attestation happens off-chain (Sumsub, Onfido, …); a permissioned set of
//! **attestors** signs the resulting verification level and submits it on
//! chain via [`Pallet::set_kyc`]. **No PII is stored on chain** — only the
//! verification level, expiry block, country code, and a 32-byte hash of the
//! off-chain document bundle.
//!
//! ## Sanctions
//!
//! Root may add accounts to a sanction list (OFAC SDN, EU, UK, internal).
//! Sanctioning auto-revokes any existing KYC record for that account and
//! prevents future records from being set until the account is removed from
//! the list.
//!
//! ## Read API
//!
//! Other pallets (notably `pallet-rwa`) consume KYC information through the
//! [`KycProvider`] trait, which is implemented for [`Pallet`]. The trait
//! exposes level lookups, sanction checks, expiry checks, and a combined
//! `require_at_least` guard suitable for use inside extrinsics.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub mod types;
pub mod weights;
pub use types::*;
pub use weights::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::pallet_prelude::DispatchError;
use frame_system::pallet_prelude::BlockNumberFor;

/// Read API consumed by downstream pallets (e.g. `pallet-rwa`) that need to
/// gate logic on KYC verification level and sanctions status.
pub trait KycProvider<AccountId, BlockNumber> {
	/// Returns the current KYC level for `account` (or [`KycLevel::None`]).
	///
	/// Note: this **does not** check expiry. Callers that need expiry-aware
	/// behaviour should use [`KycProvider::require_at_least`] or combine
	/// `level_of` with [`KycProvider::is_expired`].
	fn level_of(account: &AccountId) -> KycLevel;

	/// Returns `true` if `account` is on the sanction list.
	fn is_sanctioned(account: &AccountId) -> bool;

	/// Returns `true` if `account` has a KYC record whose `expires_at` is
	/// strictly less than `now`. Returns `false` if no record exists (so an
	/// unknown account is *not* considered "expired", just unverified).
	fn is_expired(account: &AccountId, now: BlockNumber) -> bool;

	/// Combined guard: errors if `account` is sanctioned, has an expired
	/// record, or has a level strictly below `required`.
	fn require_at_least(
		account: &AccountId,
		required: KycLevel,
		now: BlockNumber,
	) -> Result<(), DispatchError>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::types::{KycLevel, KycRecord, SanctionReason};
	use crate::weights::WeightInfo;
	use frame_support::pallet_prelude::*;
	use frame_support::BoundedBTreeSet;
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use sp_runtime::traits::BlakeTwo256;
	use sp_runtime::traits::Hash;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum number of attestors that can hold the right to submit
		/// KYC records concurrently.
		#[pallet::constant]
		type MaxAttestors: Get<u32>;

		/// Weight information for this pallet's extrinsics.
		type WeightInfo: WeightInfo;
	}

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// account → KYC record.
	#[pallet::storage]
	pub type KycRecords<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, KycRecord<T>, OptionQuery>;

	/// Permissioned set of attestor accounts authorised to call
	/// [`Pallet::set_kyc`] / [`Pallet::revoke_kyc`].
	#[pallet::storage]
	pub type Attestors<T: Config> =
		StorageValue<_, BoundedBTreeSet<T::AccountId, T::MaxAttestors>, ValueQuery>;

	/// account → reason it is on the sanction list.
	#[pallet::storage]
	pub type SanctionList<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, SanctionReason, OptionQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A KYC record was set or overwritten.
		KycSet {
			account: T::AccountId,
			level: KycLevel,
			expires_at: BlockNumberFor<T>,
		},
		/// A KYC record was revoked. `reason_hash` is `blake2_256(reason)` so
		/// the free-form reason string is not exposed on chain.
		KycRevoked { account: T::AccountId, reason_hash: H256 },
		/// A new attestor was added to the permissioned set.
		AttestorAdded { attestor: T::AccountId },
		/// An attestor was removed from the permissioned set.
		AttestorRemoved { attestor: T::AccountId },
		/// An account was added to the sanction list.
		AccountSanctioned { account: T::AccountId, reason: SanctionReason },
		/// An account was removed from the sanction list.
		AccountUnsanctioned { account: T::AccountId },
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Caller is not in the [`Attestors`] set.
		NotAttestor,
		/// The target account is on the sanction list.
		AccountSanctioned,
		/// The KYC record exists but has expired.
		KycExpired,
		/// The KYC level is below the level required by the caller.
		KycBelowRequiredLevel,
		/// Country code `[0, 0]` is reserved as the unset sentinel.
		InvalidCountryCode,
		/// `expires_at` must be strictly greater than the current block.
		ExpiryInPast,
		/// Attestor is already in the set.
		AlreadyAttestor,
		/// Attestor is not in the set.
		UnknownAttestor,
		/// The attestor set is at `MaxAttestors` capacity.
		AttestorsFull,
		/// `record.attested_by` does not match the calling attestor.
		AttestorMismatch,
	}

	// ---------------------------------------------------------------------------
	// Genesis
	// ---------------------------------------------------------------------------

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial set of attestor accounts.
		pub initial_attestors: alloc::vec::Vec<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let mut set: BoundedBTreeSet<T::AccountId, T::MaxAttestors> =
				BoundedBTreeSet::new();
			for a in self.initial_attestors.iter() {
				set.try_insert(a.clone())
					.expect("initial_attestors must fit within MaxAttestors at genesis");
			}
			Attestors::<T>::put(set);
		}
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set (or overwrite) the KYC record for `account`.
		///
		/// The caller must be a registered attestor and must equal
		/// `record.attested_by`. The target account must not be on the
		/// sanction list. The record's expiry must be in the future and the
		/// country code must not be `[0, 0]`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_kyc())]
		pub fn set_kyc(
			origin: OriginFor<T>,
			account: T::AccountId,
			record: KycRecord<T>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(Attestors::<T>::get().contains(&caller), Error::<T>::NotAttestor);
			ensure!(
				!SanctionList::<T>::contains_key(&account),
				Error::<T>::AccountSanctioned
			);
			ensure!(record.attested_by == caller, Error::<T>::AttestorMismatch);

			let now = <frame_system::Pallet<T>>::block_number();
			ensure!(record.expires_at > now, Error::<T>::ExpiryInPast);
			ensure!(record.country_code != [0u8, 0u8], Error::<T>::InvalidCountryCode);

			let level = record.level;
			let expires_at = record.expires_at;
			KycRecords::<T>::insert(&account, record);

			Self::deposit_event(Event::KycSet { account, level, expires_at });
			Ok(())
		}

		/// Revoke the KYC record for `account`.
		///
		/// Any registered attestor (or root) may revoke any record. The
		/// free-form `reason` is hashed with blake2-256 before being emitted
		/// in the [`Event::KycRevoked`] event so that it is not stored
		/// verbatim on chain.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::revoke_kyc())]
		pub fn revoke_kyc(
			origin: OriginFor<T>,
			account: T::AccountId,
			reason: BoundedVec<u8, ConstU32<64>>,
		) -> DispatchResult {
			// Root OR any registered attestor.
			let maybe_signed = ensure_signed_or_root(origin)?;
			if let Some(who) = maybe_signed {
				ensure!(Attestors::<T>::get().contains(&who), Error::<T>::NotAttestor);
			}

			KycRecords::<T>::remove(&account);
			let reason_hash = BlakeTwo256::hash(reason.as_slice());
			Self::deposit_event(Event::KycRevoked { account, reason_hash });
			Ok(())
		}

		/// Add `attestor` to the permissioned attestor set. Root only.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::add_attestor())]
		pub fn add_attestor(
			origin: OriginFor<T>,
			attestor: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;
			Attestors::<T>::try_mutate(|set| -> DispatchResult {
				ensure!(!set.contains(&attestor), Error::<T>::AlreadyAttestor);
				set.try_insert(attestor.clone())
					.map_err(|_| Error::<T>::AttestorsFull)?;
				Ok(())
			})?;
			Self::deposit_event(Event::AttestorAdded { attestor });
			Ok(())
		}

		/// Remove `attestor` from the permissioned attestor set. Root only.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::remove_attestor())]
		pub fn remove_attestor(
			origin: OriginFor<T>,
			attestor: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;
			Attestors::<T>::try_mutate(|set| -> DispatchResult {
				ensure!(set.remove(&attestor), Error::<T>::UnknownAttestor);
				Ok(())
			})?;
			Self::deposit_event(Event::AttestorRemoved { attestor });
			Ok(())
		}

		/// Add `account` to the sanction list and auto-revoke any existing
		/// KYC record for that account. Root only.
		///
		/// Emits [`Event::KycRevoked`] (with `reason_hash` of the literal
		/// bytes `b"sanctioned"`) **in addition to** [`Event::AccountSanctioned`]
		/// when an existing record is removed.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::add_to_sanction_list())]
		pub fn add_to_sanction_list(
			origin: OriginFor<T>,
			account: T::AccountId,
			reason: SanctionReason,
		) -> DispatchResult {
			ensure_root(origin)?;

			if KycRecords::<T>::contains_key(&account) {
				KycRecords::<T>::remove(&account);
				let reason_hash = BlakeTwo256::hash(b"sanctioned");
				Self::deposit_event(Event::KycRevoked {
					account: account.clone(),
					reason_hash,
				});
			}

			SanctionList::<T>::insert(&account, reason);
			Self::deposit_event(Event::AccountSanctioned { account, reason });
			Ok(())
		}

		/// Remove `account` from the sanction list. Root only.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::remove_from_sanction_list())]
		pub fn remove_from_sanction_list(
			origin: OriginFor<T>,
			account: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;
			SanctionList::<T>::remove(&account);
			Self::deposit_event(Event::AccountUnsanctioned { account });
			Ok(())
		}
	}
}

// ---------------------------------------------------------------------------
// KycProvider implementation
// ---------------------------------------------------------------------------

impl<T: Config> KycProvider<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
	fn level_of(account: &T::AccountId) -> KycLevel {
		pallet::KycRecords::<T>::get(account)
			.map(|r| r.level)
			.unwrap_or(KycLevel::None)
	}

	fn is_sanctioned(account: &T::AccountId) -> bool {
		pallet::SanctionList::<T>::contains_key(account)
	}

	fn is_expired(account: &T::AccountId, now: BlockNumberFor<T>) -> bool {
		match pallet::KycRecords::<T>::get(account) {
			Some(r) => r.expires_at < now,
			None => false,
		}
	}

	fn require_at_least(
		account: &T::AccountId,
		required: KycLevel,
		now: BlockNumberFor<T>,
	) -> Result<(), DispatchError> {
		if Self::is_sanctioned(account) {
			return Err(pallet::Error::<T>::AccountSanctioned.into());
		}
		match pallet::KycRecords::<T>::get(account) {
			None => {
				if required == KycLevel::None {
					Ok(())
				} else {
					Err(pallet::Error::<T>::KycBelowRequiredLevel.into())
				}
			}
			Some(record) => {
				if record.expires_at < now {
					return Err(pallet::Error::<T>::KycExpired.into());
				}
				if record.level < required {
					return Err(pallet::Error::<T>::KycBelowRequiredLevel.into());
				}
				Ok(())
			}
		}
	}
}
