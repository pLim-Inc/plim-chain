//! # pallet-plim-oracle
//!
//! Multi-updater, quorum-based price oracle for the P:L:I:M:/Protocol.
//!
//! A small whitelisted set of *updaters* (root-managed) submit price proposals
//! for a fixed catalogue of [`AssetPair`] values. When at least `Quorum`
//! distinct updaters propose the *same* `rate_micros` within the configured
//! [`Config::StalenessWindow`], the rate is promoted to active storage and
//! becomes consumable through the [`RateProvider`] trait.
//!
//! Pending proposals older than `StalenessWindow` are pruned lazily on every
//! `propose_rate` call so stale data never reaches quorum.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod types;
pub mod weights;

pub use types::{AssetPair, OracleRate, PendingRate};
pub use weights::WeightInfo;

use frame_support::pallet_prelude::DispatchError;
use frame_support::traits::Get;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::Saturating;

/// Read-only API for other pallets that need an EUR-denominated price.
pub trait RateProvider<T: Config> {
	/// Return the latest rate for `pair` if it exists and is fresh.
	fn get_rate(pair: AssetPair) -> Result<u64, DispatchError>;
	/// Returns true iff a rate exists for `pair` and `block - updated_at <= StalenessWindow`.
	fn is_fresh(pair: AssetPair, block: BlockNumberFor<T>) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::types::{AssetPair, OracleRate, PendingRate};
	use alloc::vec::Vec;
	use frame_support::{pallet_prelude::*, BoundedBTreeSet, BoundedVec};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The aggregated runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum number of whitelisted updaters.
		#[pallet::constant]
		type MaxUpdaters: Get<u32>;

		/// Block window after which a pending proposal — or an active rate —
		/// is considered stale.
		#[pallet::constant]
		type StalenessWindow: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: crate::weights::WeightInfo;
	}

	// ---------- Storage ----------

	/// Active per-pair rate that has reached quorum.
	#[pallet::storage]
	pub type Rates<T: Config> =
		StorageMap<_, Blake2_128Concat, AssetPair, OracleRate<T>, OptionQuery>;

	/// Per-(pair, updater) pending proposal awaiting quorum.
	#[pallet::storage]
	pub type PendingRates<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AssetPair,
		Blake2_128Concat,
		T::AccountId,
		PendingRate<T>,
		OptionQuery,
	>;

	/// Whitelisted updater set. Bounded by `MaxUpdaters`.
	#[pallet::storage]
	pub type Updaters<T: Config> =
		StorageValue<_, BoundedBTreeSet<T::AccountId, T::MaxUpdaters>, ValueQuery>;

	/// Number of distinct updaters required to agree on a value before it
	/// becomes the active rate.
	#[pallet::storage]
	pub type Quorum<T: Config> = StorageValue<_, u32, ValueQuery>;

	// ---------- Events ----------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An updater proposed a new rate. Quorum may or may not have been reached.
		RateProposed { pair: AssetPair, by: T::AccountId, rate_micros: u64 },
		/// A rate reached quorum and is now the active value for `pair`.
		RateUpdated { pair: AssetPair, rate_micros: u64, attesters: Vec<T::AccountId> },
		/// A new account was whitelisted as an updater.
		UpdaterAdded { updater: T::AccountId },
		/// An account was removed from the updater whitelist.
		UpdaterRemoved { updater: T::AccountId },
		/// The quorum threshold changed.
		QuorumChanged { new_quorum: u32 },
	}

	// ---------- Errors ----------

	#[pallet::error]
	pub enum Error<T> {
		/// The signing account is not in the updater whitelist.
		NotUpdater,
		/// No active rate exists for the requested pair.
		RateNotSet,
		/// The active rate has aged past `StalenessWindow`.
		StaleRate,
		/// Proposed quorum is zero or larger than the current updater count.
		BadQuorum,
		/// Account is already in the updater whitelist.
		AlreadyUpdater,
		/// The updater set is full (`MaxUpdaters` reached).
		UpdatersFull,
		/// Account is not currently in the updater whitelist.
		UnknownUpdater,
	}

	// ---------- Genesis ----------

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial whitelisted updater accounts.
		pub initial_updaters: Vec<T::AccountId>,
		/// Initial quorum threshold (must be >= 1 and <= initial_updaters.len()).
		pub initial_quorum: u32,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let mut set: BoundedBTreeSet<T::AccountId, T::MaxUpdaters> = BoundedBTreeSet::new();
			for u in &self.initial_updaters {
				let _ = set.try_insert(u.clone());
			}
			Updaters::<T>::put(set);
			Quorum::<T>::put(self.initial_quorum);
		}
	}

	// ---------- Extrinsics ----------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Propose a price for `pair`. Caller must be in the updater whitelist.
		///
		/// Stale pending proposals (older than `StalenessWindow`) are pruned
		/// before recomputing quorum. If `Quorum` distinct updaters now agree
		/// on `rate_micros`, the rate is promoted to [`Rates`] and the
		/// contributing pending entries are cleared.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::propose_rate())]
		pub fn propose_rate(
			origin: OriginFor<T>,
			pair: AssetPair,
			rate_micros: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Updaters::<T>::get().contains(&who), Error::<T>::NotUpdater);

			let now = frame_system::Pallet::<T>::block_number();
			let window = T::StalenessWindow::get();

			// Lazy cleanup: drop pending proposals older than the staleness window.
			let stale: Vec<T::AccountId> = PendingRates::<T>::iter_prefix(pair)
				.filter_map(|(acc, p)| {
					if now.saturating_sub(p.proposed_at) > window { Some(acc) } else { None }
				})
				.collect();
			for acc in stale {
				PendingRates::<T>::remove(pair, acc);
			}

			// Insert/overwrite this updater's proposal.
			PendingRates::<T>::insert(
				pair,
				&who,
				PendingRate::<T> { rate_micros, proposed_at: now },
			);

			Self::deposit_event(Event::RateProposed { pair, by: who.clone(), rate_micros });

			// Recompute quorum: count distinct fresh proposals matching this value.
			let mut attesters: Vec<T::AccountId> = PendingRates::<T>::iter_prefix(pair)
				.filter_map(|(acc, p)| {
					if p.rate_micros == rate_micros
						&& now.saturating_sub(p.proposed_at) <= window
					{
						Some(acc)
					} else {
						None
					}
				})
				.collect();
			attesters.sort();

			let q = Quorum::<T>::get() as usize;
			if q > 0 && attesters.len() >= q {
				let mut bounded: BoundedVec<T::AccountId, T::MaxUpdaters> = BoundedVec::new();
				for a in &attesters {
					let _ = bounded.try_push(a.clone());
				}
				Rates::<T>::insert(
					pair,
					OracleRate::<T> {
						rate_micros,
						updated_at: now,
						quorum_attesters: bounded,
					},
				);
				// Clear pending proposals for the attesters that contributed.
				for a in &attesters {
					PendingRates::<T>::remove(pair, a);
				}
				Self::deposit_event(Event::RateUpdated {
					pair,
					rate_micros,
					attesters,
				});
			}

			Ok(())
		}

		/// Add a new updater to the whitelist. Root-only.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::add_updater())]
		pub fn add_updater(origin: OriginFor<T>, new_updater: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;
			Updaters::<T>::try_mutate(|set| -> DispatchResult {
				ensure!(!set.contains(&new_updater), Error::<T>::AlreadyUpdater);
				set.try_insert(new_updater.clone()).map_err(|_| Error::<T>::UpdatersFull)?;
				Ok(())
			})?;
			Self::deposit_event(Event::UpdaterAdded { updater: new_updater });
			Ok(())
		}

		/// Remove an updater. Strips all of their pending proposals across every
		/// asset pair to prevent ghost-quorum attacks. Root-only.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::remove_updater())]
		pub fn remove_updater(origin: OriginFor<T>, updater: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;
			Updaters::<T>::try_mutate(|set| -> DispatchResult {
				ensure!(set.remove(&updater), Error::<T>::UnknownUpdater);
				Ok(())
			})?;
			// Clear pending proposals from this updater across ALL pairs.
			for pair in [
				AssetPair::PlimEur,
				AssetPair::PeurEur,
				AssetPair::BtcEur,
				AssetPair::EthEur,
			] {
				PendingRates::<T>::remove(pair, &updater);
			}
			Self::deposit_event(Event::UpdaterRemoved { updater });
			Ok(())
		}

		/// Set the quorum threshold. Must be `1 <= new_quorum <= Updaters.len()`.
		/// Root-only.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::set_quorum())]
		pub fn set_quorum(origin: OriginFor<T>, new_quorum: u32) -> DispatchResult {
			ensure_root(origin)?;
			let n = Updaters::<T>::get().len() as u32;
			ensure!(new_quorum >= 1 && new_quorum <= n, Error::<T>::BadQuorum);
			Quorum::<T>::put(new_quorum);
			Self::deposit_event(Event::QuorumChanged { new_quorum });
			Ok(())
		}
	}
}

// ---------- RateProvider impl ----------

impl<T: Config> RateProvider<T> for Pallet<T> {
	fn get_rate(pair: AssetPair) -> Result<u64, DispatchError> {
		let rate = pallet::Rates::<T>::get(pair).ok_or(pallet::Error::<T>::RateNotSet)?;
		let now = frame_system::Pallet::<T>::block_number();
		let window = T::StalenessWindow::get();
		if now.saturating_sub(rate.updated_at) > window {
			return Err(pallet::Error::<T>::StaleRate.into());
		}
		Ok(rate.rate_micros)
	}

	fn is_fresh(pair: AssetPair, block: BlockNumberFor<T>) -> bool {
		match pallet::Rates::<T>::get(pair) {
			Some(rate) => block.saturating_sub(rate.updated_at) <= T::StalenessWindow::get(),
			None => false,
		}
	}
}
