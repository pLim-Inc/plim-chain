//! # pallet-rwa
//!
//! Real-World-Asset (RWA) tokenization pallet for the P:L:I:M:/Protocol.
//!
//! Each registered asset has a finite total supply; users hold fractional
//! shares. Yield is distributed pro-rata via a lazy-claim model: the manager
//! snapshots shareholder balances at the distribution block and writes
//! `UnclaimedYield(asset_id, distribution_id, account)` rows; each holder
//! later calls `claim_yield` (single distribution) or `claim_all_yield`
//! (bounded batch) to pull their cut.
//!
//! The pallet is KYC-gated through a `KycProvider` trait defined locally so
//! this crate can compile and test independently of `pallet-plim-kyc`. The
//! coordinator wires the real implementation at the runtime layer.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub mod types;
pub mod weights;
pub use weights::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::Currency as PalletCurrency;
use scale_info::TypeInfo;
use sp_runtime::{DispatchError, RuntimeDebug};

pub use types::*;

/// Convenience alias for the pallet's native balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as PalletCurrency<<T as frame_system::Config>::AccountId>>::Balance;

// ---------------------------------------------------------------------------
// Local KYC trait (mirror of `pallet-plim-kyc::KycProvider`)
// ---------------------------------------------------------------------------

/// Tiered KYC level. Numeric ordering is meaningful: a higher level satisfies
/// any lower requirement.
#[derive(
	Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, PartialOrd, Ord,
	TypeInfo, MaxEncodedLen, RuntimeDebug,
)]
pub enum KycLevel {
	None = 0,
	Basic = 1,
	Enhanced = 2,
	Institutional = 3,
}

impl Default for KycLevel {
	fn default() -> Self {
		KycLevel::None
	}
}

/// Trait implemented by the runtime KYC provider (e.g. `pallet-plim-kyc`).
///
/// Defined locally so this pallet has no compile-time dependency on any
/// concrete KYC pallet. The runtime aggregator wires the real implementation.
pub trait KycProvider<AccountId, BlockNumber> {
	/// Current KYC tier of `account` (defaults to `None` if unknown).
	fn level_of(account: &AccountId) -> KycLevel;
	/// Whether `account` appears on the sanctions list.
	fn is_sanctioned(account: &AccountId) -> bool;
	/// Whether `account`'s KYC has expired as of block `now`.
	fn is_expired(account: &AccountId, now: BlockNumber) -> bool;
	/// Combined gate used by call sites: ensures `account` is not sanctioned,
	/// not expired, and meets at least `required` tier.
	fn require_at_least(
		account: &AccountId,
		required: KycLevel,
		now: BlockNumber,
	) -> Result<(), DispatchError>;
}

/// Default trivial impl: every account passes as `Institutional`.
///
/// Useful for unit tests in pallets that don't care about KYC. The mock used
/// by this pallet's own tests installs a granular `MockKyc` instead.
impl<AccountId, BlockNumber> KycProvider<AccountId, BlockNumber> for () {
	fn level_of(_account: &AccountId) -> KycLevel {
		KycLevel::Institutional
	}
	fn is_sanctioned(_account: &AccountId) -> bool {
		false
	}
	fn is_expired(_account: &AccountId, _now: BlockNumber) -> bool {
		false
	}
	fn require_at_least(
		_account: &AccountId,
		_required: KycLevel,
		_now: BlockNumber,
	) -> Result<(), DispatchError> {
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency as _, ExistenceRequirement},
	};
	use frame_system::pallet_prelude::*;
	use sp_core::H256;
	use sp_runtime::traits::{AccountIdConversion, AtLeast32BitUnsigned, CheckedAdd, One, Saturating, Zero};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// -----------------------------------------------------------------------
	// Config
	// -----------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Native chain currency used for yield escrow / payouts.
		type Currency: frame_support::traits::Currency<Self::AccountId>;

		/// Identifier for an RWA asset (u32 in mock).
		type RwaAssetId: Parameter
			+ Member
			+ Copy
			+ MaxEncodedLen
			+ Default
			+ AtLeast32BitUnsigned
			+ core::fmt::Debug;

		/// Identifier for a yield distribution event (u64 in mock).
		type DistributionId: Parameter
			+ Member
			+ Copy
			+ MaxEncodedLen
			+ Default
			+ AtLeast32BitUnsigned
			+ core::fmt::Debug;

		/// KYC provider used to gate mints/transfers.
		type Kyc: KycProvider<Self::AccountId, BlockNumberFor<Self>>;

		/// Maximum number of distributions a single `claim_all_yield` call
		/// will iterate over before stopping.
		#[pallet::constant]
		type MaxDistributionsPerClaim: Get<u32>;

		/// Maximum number of shareholder rows a single distribution will pay
		/// out to. Prevents O(n) extrinsics from exceeding block weight.
		#[pallet::constant]
		type MaxShareholdersPerDistribution: Get<u32>;

		/// Pallet-specific weight info.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Registered RWA assets keyed by their asset id.
	#[pallet::storage]
	pub type Assets<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RwaAssetId, RwaAsset<T>, OptionQuery>;

	/// Per-asset shareholder ledger: (asset_id, account) -> balance.
	#[pallet::storage]
	pub type Shareholders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::RwaAssetId,
		Blake2_128Concat,
		T::AccountId,
		BalanceOf<T>,
		ValueQuery,
	>;

	/// Total amount currently issued for an asset (must be <= `total_supply`).
	#[pallet::storage]
	pub type TotalIssued<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RwaAssetId, BalanceOf<T>, ValueQuery>;

	/// (asset_id, distribution_id) -> distribution record.
	#[pallet::storage]
	pub type YieldDistributions<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::RwaAssetId,
		Blake2_128Concat,
		T::DistributionId,
		YieldDistribution<T>,
		OptionQuery,
	>;

	/// Lazy-claim ledger: ((asset_id, distribution_id), account) -> unclaimed.
	#[pallet::storage]
	pub type UnclaimedYield<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		(T::RwaAssetId, T::DistributionId),
		Blake2_128Concat,
		T::AccountId,
		BalanceOf<T>,
		ValueQuery,
	>;

	/// Per-asset lifecycle status.
	#[pallet::storage]
	pub type AssetStatus<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RwaAssetId, RwaStatus, ValueQuery>;

	/// Auto-incrementing distribution-id counter, per asset.
	#[pallet::storage]
	pub type NextDistributionId<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RwaAssetId, T::DistributionId, ValueQuery>;

	// -----------------------------------------------------------------------
	// Genesis
	// -----------------------------------------------------------------------

	// NOTE(spec 300): genesis is empty by design — RWA assets are bootstrapped
	// post-upgrade via sudo `register_asset` calls. Earlier drafts embedded
	// `Vec<(RwaAssetId, RwaAsset<T>)>` here, but that requires `RwaAsset<T>:
	// Deserialize`, which is incompatible with `T::AccountId` not being
	// serde-derived in `no_std`. Empty genesis sidesteps the issue.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		pub _phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Intentionally empty — see note above.
		}
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetRegistered {
			asset_id: T::RwaAssetId,
			symbol: BoundedVec<u8, ConstU32<16>>,
			total_supply: BalanceOf<T>,
			kyc_required: KycLevel,
		},
		SharesMinted {
			asset_id: T::RwaAssetId,
			to: T::AccountId,
			amount: BalanceOf<T>,
			payment_proof_hash: H256,
		},
		SharesBurned {
			asset_id: T::RwaAssetId,
			from: T::AccountId,
			amount: BalanceOf<T>,
		},
		BurnRequested {
			asset_id: T::RwaAssetId,
			account: T::AccountId,
			amount: BalanceOf<T>,
		},
		SharesTransferred {
			asset_id: T::RwaAssetId,
			from: T::AccountId,
			to: T::AccountId,
			amount: BalanceOf<T>,
		},
		YieldDistributed {
			asset_id: T::RwaAssetId,
			distribution_id: T::DistributionId,
			total_amount: BalanceOf<T>,
			snapshot_at: BlockNumberFor<T>,
		},
		YieldClaimed {
			asset_id: T::RwaAssetId,
			distribution_id: T::DistributionId,
			account: T::AccountId,
			amount: BalanceOf<T>,
		},
		AssetFrozen {
			asset_id: T::RwaAssetId,
		},
		AssetUnfrozen {
			asset_id: T::RwaAssetId,
		},
		AssetWoundDown {
			asset_id: T::RwaAssetId,
			final_nav: BalanceOf<T>,
		},
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		AssetNotFound,
		AssetFrozen,
		AssetWoundDown,
		TotalSupplyExceeded,
		InsufficientShares,
		KycRequired,
		SenderKycFailed,
		ReceiverKycFailed,
		ManagerOnly,
		DistributionNotFound,
		NothingToClaim,
		BadPaymentProof,
		ArithmeticOverflow,
		SnapshotBlockInFuture,
		AssetAlreadyExists,
		TooManyShareholders,
	}

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new RWA asset. Root only.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::register_asset())]
		pub fn register_asset(origin: OriginFor<T>, asset: RwaAsset<T>) -> DispatchResult {
			ensure_root(origin)?;

			// Derive an asset id from the symbol via blake2 → first 4 bytes → u32.
			// To avoid extra config surface, we use the count of existing assets +
			// (length-of-symbol << 16) as a deterministic-but-collision-prone id.
			// The orchestrator typically registers via genesis instead; this
			// extrinsic exists primarily for governance-driven additions.
			let id = Self::next_asset_id_from(&asset);
			ensure!(!Assets::<T>::contains_key(id), Error::<T>::AssetAlreadyExists);

			let symbol = asset.symbol.clone();
			let total_supply = asset.total_supply;
			let kyc_required = asset.kyc_required;

			Assets::<T>::insert(id, asset);
			AssetStatus::<T>::insert(id, RwaStatus::Active);

			Self::deposit_event(Event::AssetRegistered {
				asset_id: id,
				symbol,
				total_supply,
				kyc_required,
			});
			Ok(())
		}

		/// Mint fresh shares of `asset_id` to `to`. Manager-only. Enforces
		/// `total_supply` cap and KYC on the recipient.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::mint_shares())]
		pub fn mint_shares(
			origin: OriginFor<T>,
			asset_id: T::RwaAssetId,
			to: T::AccountId,
			amount: BalanceOf<T>,
			payment_proof: PaymentProof<T::AccountId, BalanceOf<T>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let asset = Assets::<T>::get(asset_id).ok_or(Error::<T>::AssetNotFound)?;
			ensure!(asset.manager == who, Error::<T>::ManagerOnly);

			Self::ensure_active(asset_id)?;

			// KYC gate on recipient.
			let now = frame_system::Pallet::<T>::block_number();
			T::Kyc::require_at_least(&to, asset.kyc_required, now)
				.map_err(|_| Error::<T>::ReceiverKycFailed)?;

			// Sanity: payment proof amount must equal mint amount (prevents
			// 0-EUR mints with stale proofs being trivially reused).
			ensure!(payment_proof.amount == amount, Error::<T>::BadPaymentProof);

			// Cap.
			let issued = TotalIssued::<T>::get(asset_id);
			let new_issued = issued.checked_add(&amount).ok_or(Error::<T>::ArithmeticOverflow)?;
			ensure!(new_issued <= asset.total_supply, Error::<T>::TotalSupplyExceeded);

			TotalIssued::<T>::insert(asset_id, new_issued);
			Shareholders::<T>::mutate(asset_id, &to, |bal| {
				*bal = bal.saturating_add(amount);
			});

			Self::deposit_event(Event::SharesMinted {
				asset_id,
				to,
				amount,
				payment_proof_hash: payment_proof.proof_hash,
			});
			Ok(())
		}

		/// Burn caller's own shares. Emits `BurnRequested` so the off-chain
		/// custodian can settle the underlying redemption (fiat / RE buyback).
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::burn_shares())]
		pub fn burn_shares(
			origin: OriginFor<T>,
			asset_id: T::RwaAssetId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);

			Shareholders::<T>::try_mutate(asset_id, &who, |bal| -> DispatchResult {
				ensure!(*bal >= amount, Error::<T>::InsufficientShares);
				*bal = bal.saturating_sub(amount);
				Ok(())
			})?;

			TotalIssued::<T>::mutate(asset_id, |t| {
				*t = t.saturating_sub(amount);
			});

			Self::deposit_event(Event::SharesBurned { asset_id, from: who.clone(), amount });
			Self::deposit_event(Event::BurnRequested { asset_id, account: who, amount });
			Ok(())
		}

		/// Transfer shares between two KYC-cleared accounts.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::transfer_shares())]
		pub fn transfer_shares(
			origin: OriginFor<T>,
			asset_id: T::RwaAssetId,
			to: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let from = ensure_signed(origin)?;
			let asset = Assets::<T>::get(asset_id).ok_or(Error::<T>::AssetNotFound)?;
			Self::ensure_active(asset_id)?;

			let now = frame_system::Pallet::<T>::block_number();
			T::Kyc::require_at_least(&from, asset.kyc_required, now)
				.map_err(|_| Error::<T>::SenderKycFailed)?;
			T::Kyc::require_at_least(&to, asset.kyc_required, now)
				.map_err(|_| Error::<T>::ReceiverKycFailed)?;

			Shareholders::<T>::try_mutate(asset_id, &from, |bal| -> DispatchResult {
				ensure!(*bal >= amount, Error::<T>::InsufficientShares);
				*bal = bal.saturating_sub(amount);
				Ok(())
			})?;
			Shareholders::<T>::mutate(asset_id, &to, |bal| {
				*bal = bal.saturating_add(amount);
			});

			Self::deposit_event(Event::SharesTransferred { asset_id, from, to, amount });
			Ok(())
		}

		/// Distribute `total_amount` of `currency` pro-rata to current
		/// shareholders. Manager-only. Snapshots `Shareholders` at the current
		/// block; iterates up to `MaxShareholdersPerDistribution` rows.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::distribute_yield())]
		pub fn distribute_yield(
			origin: OriginFor<T>,
			asset_id: T::RwaAssetId,
			total_amount: BalanceOf<T>,
			currency: Currency,
			description_hash: H256,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let asset = Assets::<T>::get(asset_id).ok_or(Error::<T>::AssetNotFound)?;
			ensure!(asset.manager == who, Error::<T>::ManagerOnly);
			// Distributions are allowed in Active and Frozen states (unwinding
			// flows still need to pay accrued yield); blocked once WoundDown.
			ensure!(
				AssetStatus::<T>::get(asset_id) != RwaStatus::WoundDown,
				Error::<T>::AssetWoundDown
			);

			let total_issued = TotalIssued::<T>::get(asset_id);
			ensure!(!total_issued.is_zero(), Error::<T>::NothingToClaim);

			// Bounded shareholder check.
			let count = Shareholders::<T>::iter_prefix(asset_id).count() as u32;
			ensure!(
				count <= T::MaxShareholdersPerDistribution::get(),
				Error::<T>::TooManyShareholders
			);

			// Lock funds: transfer from manager to pallet account.
			let pot = Self::account_id();
			<T::Currency as frame_support::traits::Currency<T::AccountId>>::transfer(
				&who,
				&pot,
				total_amount,
				ExistenceRequirement::AllowDeath,
			)?;

			// Allocate distribution id.
			let dist_id = NextDistributionId::<T>::get(asset_id);
			let next = dist_id.saturating_add(One::one());
			NextDistributionId::<T>::insert(asset_id, next);

			let now = frame_system::Pallet::<T>::block_number();

			// Pro-rata write. Use u128 for the intermediate to avoid loss when
			// `BalanceOf<T>` is itself u128 (saturating_mul would silently cap).
			let total_u128 = Self::balance_to_u128(total_amount);
			let issued_u128 = Self::balance_to_u128(total_issued);
			let mut allocated_sum: u128 = 0;

			for (holder, hbal) in Shareholders::<T>::iter_prefix(asset_id) {
				let hbal_u128 = Self::balance_to_u128(hbal);
				// floor( total * hbal / issued )
				let share_u128 = total_u128
					.saturating_mul(hbal_u128)
					.checked_div(issued_u128)
					.unwrap_or(0);
				if share_u128 == 0 {
					continue;
				}
				let share_bal = Self::u128_to_balance(share_u128);
				UnclaimedYield::<T>::insert((asset_id, dist_id), &holder, share_bal);
				allocated_sum = allocated_sum.saturating_add(share_u128);
			}

			// Any rounding remainder stays parked in `remaining_unclaimed`.
			let remaining_u128 = total_u128.saturating_sub(allocated_sum);
			let _ = remaining_u128; // already accounted for via field below

			let dist = YieldDistribution::<T> {
				total_amount,
				currency,
				distributed_at: now,
				snapshot_at: now,
				description_hash,
				remaining_unclaimed: total_amount,
			};
			YieldDistributions::<T>::insert(asset_id, dist_id, dist);

			Self::deposit_event(Event::YieldDistributed {
				asset_id,
				distribution_id: dist_id,
				total_amount,
				snapshot_at: now,
			});
			Ok(())
		}

		/// Claim caller's entry from a single yield distribution.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::claim_yield())]
		pub fn claim_yield(
			origin: OriginFor<T>,
			asset_id: T::RwaAssetId,
			distribution_id: T::DistributionId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(
				YieldDistributions::<T>::contains_key(asset_id, distribution_id),
				Error::<T>::DistributionNotFound
			);

			let amount = UnclaimedYield::<T>::get((asset_id, distribution_id), &who);
			ensure!(!amount.is_zero(), Error::<T>::NothingToClaim);

			// Pay out from pot.
			let pot = Self::account_id();
			<T::Currency as frame_support::traits::Currency<T::AccountId>>::transfer(
				&pot,
				&who,
				amount,
				ExistenceRequirement::AllowDeath,
			)?;

			UnclaimedYield::<T>::remove((asset_id, distribution_id), &who);
			YieldDistributions::<T>::mutate(asset_id, distribution_id, |maybe| {
				if let Some(d) = maybe.as_mut() {
					d.remaining_unclaimed = d.remaining_unclaimed.saturating_sub(amount);
				}
			});

			Self::deposit_event(Event::YieldClaimed {
				asset_id,
				distribution_id,
				account: who,
				amount,
			});
			Ok(())
		}

		/// Sweep up to `MaxDistributionsPerClaim` of the caller's open
		/// entries for `asset_id`, paying the cumulative sum in a single
		/// transfer and emitting one `YieldClaimed` event per cleared row.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::claim_all_yield())]
		pub fn claim_all_yield(origin: OriginFor<T>, asset_id: T::RwaAssetId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);

			let limit = T::MaxDistributionsPerClaim::get();
			let mut cleared: alloc::vec::Vec<(T::DistributionId, BalanceOf<T>)> =
				alloc::vec::Vec::new();
			let mut total: BalanceOf<T> = Zero::zero();

			// Iterate the asset's distributions in storage order; cheap because
			// `YieldDistributions` is keyed by (asset_id, dist_id).
			for (dist_id, _dist) in YieldDistributions::<T>::iter_prefix(asset_id) {
				if (cleared.len() as u32) >= limit {
					break;
				}
				let amount = UnclaimedYield::<T>::get((asset_id, dist_id), &who);
				if amount.is_zero() {
					continue;
				}
				cleared.push((dist_id, amount));
				total = total.saturating_add(amount);
			}

			ensure!(!total.is_zero(), Error::<T>::NothingToClaim);

			// Single transfer for gas efficiency.
			let pot = Self::account_id();
			<T::Currency as frame_support::traits::Currency<T::AccountId>>::transfer(
				&pot,
				&who,
				total,
				ExistenceRequirement::AllowDeath,
			)?;

			for (dist_id, amount) in cleared {
				UnclaimedYield::<T>::remove((asset_id, dist_id), &who);
				YieldDistributions::<T>::mutate(asset_id, dist_id, |maybe| {
					if let Some(d) = maybe.as_mut() {
						d.remaining_unclaimed =
							d.remaining_unclaimed.saturating_sub(amount);
					}
				});
				Self::deposit_event(Event::YieldClaimed {
					asset_id,
					distribution_id: dist_id,
					account: who.clone(),
					amount,
				});
			}

			Ok(())
		}

		/// Freeze the asset (root). Blocks mints/transfers; claims still flow.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::freeze())]
		pub fn freeze(origin: OriginFor<T>, asset_id: T::RwaAssetId) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);
			ensure!(
				AssetStatus::<T>::get(asset_id) != RwaStatus::WoundDown,
				Error::<T>::AssetWoundDown
			);
			AssetStatus::<T>::insert(asset_id, RwaStatus::Frozen);
			Self::deposit_event(Event::AssetFrozen { asset_id });
			Ok(())
		}

		/// Restore a frozen asset to `Active` (root).
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::unfreeze())]
		pub fn unfreeze(origin: OriginFor<T>, asset_id: T::RwaAssetId) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);
			ensure!(
				AssetStatus::<T>::get(asset_id) != RwaStatus::WoundDown,
				Error::<T>::AssetWoundDown
			);
			AssetStatus::<T>::insert(asset_id, RwaStatus::Active);
			Self::deposit_event(Event::AssetUnfrozen { asset_id });
			Ok(())
		}

		/// Permanently wind down the asset (root). Final NAV emitted in event;
		/// here it is reported as the current `TotalIssued` for accounting.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::wind_down())]
		pub fn wind_down(origin: OriginFor<T>, asset_id: T::RwaAssetId) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);
			AssetStatus::<T>::insert(asset_id, RwaStatus::WoundDown);
			let final_nav = TotalIssued::<T>::get(asset_id);
			Self::deposit_event(Event::AssetWoundDown { asset_id, final_nav });
			Ok(())
		}
	}

	// -----------------------------------------------------------------------
	// Internal helpers
	// -----------------------------------------------------------------------

	/// Pallet account id used to escrow yield between distribution and claim.
	pub const PALLET_ID: frame_support::PalletId = frame_support::PalletId(*b"py/plrwa");

	impl<T: Config> Pallet<T> {
		pub fn account_id() -> T::AccountId {
			PALLET_ID.into_account_truncating()
		}

		fn ensure_active(asset_id: T::RwaAssetId) -> DispatchResult {
			match AssetStatus::<T>::get(asset_id) {
				RwaStatus::Active => Ok(()),
				RwaStatus::Frozen => Err(Error::<T>::AssetFrozen.into()),
				RwaStatus::WoundDown => Err(Error::<T>::AssetWoundDown.into()),
			}
		}

		/// Generate a deterministic asset id from the asset record. Uses a
		/// blake2-256 of the SCALE-encoded symbol, narrowed to `RwaAssetId`.
		fn next_asset_id_from(asset: &RwaAsset<T>) -> T::RwaAssetId {
			use sp_runtime::traits::Hash as _;
			let h = <T as frame_system::Config>::Hashing::hash(&asset.symbol.encode());
			let bytes = h.as_ref();
			// Take the first 8 bytes → u64, then convert via the `From<u32>`
			// chain in `AtLeast32BitUnsigned`.
			let mut buf = [0u8; 8];
			buf.copy_from_slice(&bytes[..8]);
			let n = u64::from_le_bytes(buf);
			// Truncate via repeated halving to fit any RwaAssetId width.
			let narrowed = (n as u32).max(1);
			narrowed.into()
		}

		fn balance_to_u128(b: BalanceOf<T>) -> u128 {
			use sp_runtime::SaturatedConversion;
			b.saturated_into::<u128>()
		}

		fn u128_to_balance(n: u128) -> BalanceOf<T> {
			use sp_runtime::SaturatedConversion;
			n.saturated_into()
		}
	}
}
