//! # pallet-plim-compliance
//!
//! Compliance gating for regulated assets (pEUR, pUSD). Maintains:
//!  - a sanctioned-account denylist (`Sanctioned`)
//!  - a set of regulated asset ids that require compliance checks
//!    (`RegulatedAssets`)
//!
//! `ensure_allowed(asset_id, who)` is the public hook that other pallets
//! (and a future `pallet_assets::Freezer` adapter) call to enforce the
//! denylist before any transfer of a regulated asset.
//!
//! Updated 2026-04-14T15:00 — concrete genesis allocations + 7 pallet implementations

#![cfg_attr(not(feature = "std"), no_std)]

// ===== §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23 =====
pub mod kyc_attestation;
pub mod spending_tier;
// Documentation-only modules describing the §6 storage + call surface.
// The actual pallet items live inside `pub mod pallet` below because
// FRAME's `#[pallet::storage]` / `#[pallet::call]` macros require it.
pub mod section6_calls;
pub mod section6_storage;

pub use kyc_attestation::*;
pub use spending_tier::*;

#[cfg(test)]
mod tests;
// ===== /§6 KYC/AML v2 =====

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// ===== §6 KYC/AML v2 imports =====
	use crate::kyc_attestation::{KYCAttestation, KYCLevel};
	use crate::spending_tier::SpendingTier;
	use codec::MaxEncodedLen;
	use sp_runtime::traits::AtLeast32BitUnsigned;
	use core::fmt::Debug;
	// ===== /§6 KYC/AML v2 imports =====

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin allowed to manage sanctions and regulated assets (root for v2).
		///
		/// TODO §6: wire ComplianceOrigin to multisig in runtime (currently
		/// `EnsureRoot<AccountId>` per `runtime/src/configs/mod.rs:293`).
		type ComplianceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		// ===== §6 KYC/AML v2 Config bounds =====
		/// Max bytes of a jurisdiction code (ISO-3166 alpha-2/3 → 2 or 3,
		/// pick something forgiving like 8 to allow regional sub-codes).
		#[pallet::constant]
		type MaxJurisdictionLength: Get<u32>;

		/// Balance type used by SpendingTier caps (cents-precision, set
		/// by the runtime — typically `u128`).
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaxEncodedLen
			+ Debug
			+ scale_info::TypeInfo
			+ From<u32>;
		// ===== /§6 KYC/AML v2 Config bounds =====
	}

	#[pallet::storage]
	pub type Sanctioned<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, (), OptionQuery>;

	#[pallet::storage]
	pub type RegulatedAssets<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, (), OptionQuery>;

	// ===== §6 KYC/AML v2 storage =====
	// Rationale: commitment hash only — no PII ever lands on-chain.
	// See `section6_storage.rs` for the canonical schema commentary.

	/// Per-account KYC tier (cached for O(1) gate checks).
	#[pallet::storage]
	pub type KYCStatus<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, KYCLevel, ValueQuery>;

	/// Per-account commitment-hash attestation. Full record consulted
	/// when the cached `KYCStatus` is insufficient (e.g. auditor wants
	/// jurisdiction + expiry).
	#[pallet::storage]
	pub type KYCAttestations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, KYCAttestation<T>, OptionQuery>;

	/// Per-account spending tier. Drives daily/per-tx caps + CT gating.
	#[pallet::storage]
	pub type SpendingTierStorage<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, SpendingTier<T>, OptionQuery>;
	// ===== /§6 KYC/AML v2 storage =====

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Sanctioned { who: T::AccountId },
		Unsanctioned { who: T::AccountId },
		AssetRegulated { asset_id: u32 },
		// ===== §6 KYC/AML v2 =====
		KYCAttestationRegistered {
			subject: T::AccountId,
			kyc_level: KYCLevel,
			attestation_hash: T::Hash,
			expires_at: BlockNumberFor<T>,
			eidas_present: bool,
		},
		// ===== /§6 KYC/AML v2 =====
	}

	#[pallet::error]
	pub enum Error<T> {
		AccountSanctioned,
		AssetNotRegulated,
		// ===== §6 KYC/AML v2 =====
		/// Account's current KYC level is below the tier's `ct_required`.
		InsufficientKYCForSpendingTier,
		/// The proposed transfer would push 24h cumulative spend over the
		/// tier's daily cap.
		DailyCapExceeded,
		/// The proposed transfer exceeds the tier's per-transaction cap.
		PerTxCapExceeded,
		/// Transfer is paused pending a human-in-the-loop compliance
		/// review (e.g. fuzzy sanctions hit).
		AwaitingHumanApproval,
		// ===== /§6 KYC/AML v2 =====
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn sanction(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::ComplianceOrigin::ensure_origin(origin)?;
			Sanctioned::<T>::insert(&who, ());
			Self::deposit_event(Event::Sanctioned { who });
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn unsanction(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::ComplianceOrigin::ensure_origin(origin)?;
			Sanctioned::<T>::remove(&who);
			Self::deposit_event(Event::Unsanctioned { who });
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn mark_regulated(origin: OriginFor<T>, asset_id: u32) -> DispatchResult {
			T::ComplianceOrigin::ensure_origin(origin)?;
			RegulatedAssets::<T>::insert(asset_id, ());
			Self::deposit_event(Event::AssetRegulated { asset_id });
			Ok(())
		}

		// ===== §6 KYC/AML v2 =====
		/// Register (or refresh) a KYC attestation for `subject`.
		///
		/// Stores only a commitment hash + bounded jurisdiction code; no
		/// PII ever lands on-chain. Callable only by `ComplianceOrigin`.
		///
		/// TODO §6: confirm call_index does not collide once the rest of
		/// section 6 lands.
		/// TODO §6: wire ComplianceOrigin to multisig in runtime.
		/// TODO §6: replace placeholder weight with benchmarked weight.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(20_000, 0))]
		pub fn register_kyc_attestation(
			origin: OriginFor<T>,
			subject: T::AccountId,
			attestation_hash: T::Hash,
			jurisdiction: BoundedVec<u8, T::MaxJurisdictionLength>,
			expires_at: BlockNumberFor<T>,
			kyc_level: KYCLevel,
			eidas_present: bool,
		) -> DispatchResult {
			T::ComplianceOrigin::ensure_origin(origin)?;

			let now = <frame_system::Pallet<T>>::block_number();
			let attestation = KYCAttestation::<T> {
				hash: attestation_hash,
				jurisdiction,
				attested_at: now,
				expires_at,
				level: kyc_level,
				eidas_present,
			};

			KYCStatus::<T>::insert(&subject, kyc_level);
			KYCAttestations::<T>::insert(&subject, attestation);

			Self::deposit_event(Event::KYCAttestationRegistered {
				subject,
				kyc_level,
				attestation_hash,
				expires_at,
				eidas_present,
			});
			Ok(())
		}
		// ===== /§6 KYC/AML v2 =====
	}

	impl<T: Config> Pallet<T> {
		/// Compliance gate: returns `Ok(())` if `who` is allowed to transact
		/// `asset_id`. Non-regulated assets are always allowed; sanctioned
		/// accounts are blocked from regulated assets.
		pub fn ensure_allowed(asset_id: u32, who: &T::AccountId) -> DispatchResult {
			if RegulatedAssets::<T>::contains_key(asset_id) {
				ensure!(!Sanctioned::<T>::contains_key(who), Error::<T>::AccountSanctioned);
			}
			Ok(())
		}
	}
}
