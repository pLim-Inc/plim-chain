//! # pallet-plim-licenses
//!
//! NFT-based 3D model license management for the P:L:I:M:/Protocol.
//!
//! This pallet manages license metadata for Collection #1 (3D model NFTs).
//! It is loosely coupled: it stores its own data and emits events. Actual NFT
//! minting/transfer is handled by the runtime integration layer calling into
//! `pallet-nfts` separately.
//!
//! ## Features
//! - Reusable license templates for creators
//! - Full license lifecycle: mint, transfer, burn, revoke
//! - Custody queue for off-chain buyers (Stripe fiat)
//! - Per-creator royalty configuration
//! - Automatic expiry sweep every 100 blocks

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

use types::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Hash;

	/// Maximum number of expired licenses to sweep per block.
	const MAX_SWEEP_PER_BLOCK: u32 = 50;
	/// Sweep every N blocks.
	const SWEEP_INTERVAL: u32 = 100;

	// Type aliases for readability.
	type LicenseDataOf<T> = LicenseData<
		<T as frame_system::Config>::AccountId,
		u128,
		BlockNumberFor<T>,
		<T as Config>::MaxGeoRestrictions,
		<T as Config>::MaxPlatformRestrictions,
	>;

	type CustodyRecordOf<T> =
		CustodyRecord<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>;

	type CreatorRoyaltyConfigOf<T> =
		CreatorRoyaltyConfig<<T as frame_system::Config>::AccountId, u32>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that the marketplace backend uses to mint licenses.
		type MarketplaceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin for admin operations (revoke, etc.).
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The NFT collection ID used for 3D model licenses.
		#[pallet::constant]
		type LicenseCollectionId: Get<u32>;

		/// Maximum number of geo restriction entries per license.
		#[pallet::constant]
		type MaxGeoRestrictions: Get<u32>;

		/// Maximum number of platform restriction entries per license.
		#[pallet::constant]
		type MaxPlatformRestrictions: Get<u32>;

		/// Weight information for this pallet's extrinsics.
		type WeightInfo: WeightInfo;
	}

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// License metadata keyed by NFT item ID.
	#[pallet::storage]
	pub type Licenses<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, LicenseDataOf<T>, OptionQuery>;

	/// Reusable license templates keyed by hash.
	#[pallet::storage]
	pub type LicenseTemplates<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, LicenseDataOf<T>, OptionQuery>;

	/// Custody queue: licenses held on behalf of off-chain buyers.
	#[pallet::storage]
	pub type CustodyQueue<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, CustodyRecordOf<T>, OptionQuery>;

	/// Per-creator royalty configuration.
	#[pallet::storage]
	pub type CreatorConfigs<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CreatorRoyaltyConfigOf<T>, OptionQuery>;

	/// Auto-incrementing item ID counter.
	#[pallet::storage]
	pub type NextItemId<T: Config> = StorageValue<_, u32, ValueQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new license was minted.
		LicenseMinted { item_id: u32, owner: T::AccountId, license_type: LicenseType },
		/// A license was transferred to a new owner.
		LicenseTransferred { item_id: u32, from: T::AccountId, to: T::AccountId },
		/// A license was burned by its owner.
		LicenseBurned { item_id: u32, owner: T::AccountId },
		/// A license was revoked by an admin.
		LicenseRevoked { item_id: u32 },
		/// A custody-held license was claimed by the buyer.
		LicenseClaimedFromCustody { item_id: u32, new_owner: T::AccountId },
		/// License template attributes were updated.
		LicenseAttrsUpdated { template_hash: T::Hash },
		/// A creator's royalty config was set or updated.
		CreatorConfigSet { creator: T::AccountId },
		/// A license expired and was swept.
		LicenseExpired { item_id: u32 },
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// The specified license item does not exist.
		LicenseNotFound,
		/// The license has expired.
		LicenseExpired,
		/// The license is not transferable.
		NotTransferable,
		/// Derivative works are not allowed under this license.
		NotDerivativeAllowed,
		/// The license is geographically restricted.
		GeoRestricted,
		/// Maximum number of prints has been reached.
		MaxPrintsReached,
		/// License attributes failed validation.
		InvalidLicenseAttrs,
		/// Royalty percentage exceeds the 25% (2500 bp) cap.
		InvalidRoyaltyPct,
		/// Custody claim signature/nonce is invalid.
		CustodySignatureInvalid,
		/// The license is not in the custody queue.
		NotCustodyOwned,
		/// The custody license has already been claimed.
		AlreadyClaimed,
		/// Caller is not authorized for this operation.
		Unauthorized,
		/// Arithmetic overflow in a calculation.
		ArithmeticOverflow,
		/// A bounded collection exceeded its limit.
		BoundExceeded,
	}

	// ---------------------------------------------------------------------------
	// Hooks
	// ---------------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			// Convert block number to u32 for modulo check.
			let n_u32: u32 = n.try_into().unwrap_or(0u32);
			if n_u32 % SWEEP_INTERVAL != 0 {
				return Weight::zero();
			}

			let mut swept: u32 = 0;
			let mut to_remove: Vec<u32> = Vec::new();

			for (item_id, license) in Licenses::<T>::iter() {
				if swept >= MAX_SWEEP_PER_BLOCK {
					break;
				}
				if let Some(expires) = license.expires_at {
					if n >= expires {
						to_remove.push(item_id);
						swept = swept.saturating_add(1);
					}
				}
			}

			for item_id in to_remove {
				Licenses::<T>::remove(item_id);
				CustodyQueue::<T>::remove(item_id);
				Self::deposit_event(Event::LicenseExpired { item_id });
			}

			T::WeightInfo::sweep_expired(swept)
		}
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a reusable license template. The caller becomes the original_creator.
		/// The template is stored keyed by the hash of its SCALE-encoded data.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_license_template())]
		pub fn create_license_template(
			origin: OriginFor<T>,
			license_type: LicenseType,
			version: u16,
			original_price_eur_cents: u32,
			original_price_plim: u128,
			transferable: bool,
			print_commercial: bool,
			derivative_allowed: bool,
			derivative_share_alike: bool,
			attribution_required: bool,
			watermark_required: bool,
			exclusive_burns_original: bool,
			royalty_pct_bp: u16,
			expires_at: Option<BlockNumberFor<T>>,
			max_prints: Option<u32>,
			max_copies: Option<u32>,
			geo_restrictions: Vec<[u8; 2]>,
			platform_restrictions: Vec<Vec<u8>>,
			jurisdiction: Jurisdiction,
			payment_method: PaymentMethod,
			payment_proof: Vec<u8>,
		) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			Self::validate_license_attrs(
				&license_type,
				royalty_pct_bp,
				transferable,
				exclusive_burns_original,
				derivative_allowed,
				derivative_share_alike,
				&expires_at,
			)?;

			let bounded_geo: BoundedVec<[u8; 2], T::MaxGeoRestrictions> =
				geo_restrictions.try_into().map_err(|_| Error::<T>::BoundExceeded)?;
			let bounded_platforms: BoundedVec<
				BoundedVec<u8, ConstU32<64>>,
				T::MaxPlatformRestrictions,
			> = platform_restrictions
				.into_iter()
				.map(|p| BoundedVec::try_from(p).map_err(|_| Error::<T>::BoundExceeded))
				.collect::<Result<Vec<_>, _>>()?
				.try_into()
				.map_err(|_| Error::<T>::BoundExceeded)?;
			let bounded_proof: BoundedVec<u8, ConstU32<128>> =
				payment_proof.try_into().map_err(|_| Error::<T>::BoundExceeded)?;

			let now = frame_system::Pallet::<T>::block_number();

			let data = LicenseDataOf::<T> {
				license_type,
				version,
				original_creator: creator.clone(),
				current_owner: creator.clone(),
				original_price_eur_cents,
				original_price_plim,
				transferable,
				print_commercial,
				derivative_allowed,
				derivative_share_alike,
				attribution_required,
				watermark_required,
				exclusive_burns_original,
				royalty_pct_bp,
				expires_at,
				max_prints,
				max_copies,
				geo_restrictions: bounded_geo,
				platform_restrictions: bounded_platforms,
				jurisdiction,
				issued_at: now,
				payment_method,
				payment_proof: bounded_proof,
			};

			let hash = T::Hashing::hash_of(&data);
			LicenseTemplates::<T>::insert(hash, data);

			Self::deposit_event(Event::LicenseAttrsUpdated { template_hash: hash });
			Ok(())
		}

		/// Mint a new license for a buyer. Restricted to MarketplaceOrigin.
		/// Assigns the next available item ID and stores full license data.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::mint_license())]
		pub fn mint_license(
			origin: OriginFor<T>,
			owner: T::AccountId,
			license_type: LicenseType,
			version: u16,
			original_creator: T::AccountId,
			original_price_eur_cents: u32,
			original_price_plim: u128,
			transferable: bool,
			print_commercial: bool,
			derivative_allowed: bool,
			derivative_share_alike: bool,
			attribution_required: bool,
			watermark_required: bool,
			exclusive_burns_original: bool,
			royalty_pct_bp: u16,
			expires_at: Option<BlockNumberFor<T>>,
			max_prints: Option<u32>,
			max_copies: Option<u32>,
			geo_restrictions: Vec<[u8; 2]>,
			platform_restrictions: Vec<Vec<u8>>,
			jurisdiction: Jurisdiction,
			payment_method: PaymentMethod,
			payment_proof: Vec<u8>,
			// Optional custody fields
			custody_buyer_email_hash: Option<[u8; 32]>,
		) -> DispatchResult {
			T::MarketplaceOrigin::ensure_origin(origin)?;

			Self::validate_license_attrs(
				&license_type,
				royalty_pct_bp,
				transferable,
				exclusive_burns_original,
				derivative_allowed,
				derivative_share_alike,
				&expires_at,
			)?;

			let bounded_geo: BoundedVec<[u8; 2], T::MaxGeoRestrictions> =
				geo_restrictions.try_into().map_err(|_| Error::<T>::BoundExceeded)?;
			let bounded_platforms: BoundedVec<
				BoundedVec<u8, ConstU32<64>>,
				T::MaxPlatformRestrictions,
			> = platform_restrictions
				.into_iter()
				.map(|p| BoundedVec::try_from(p).map_err(|_| Error::<T>::BoundExceeded))
				.collect::<Result<Vec<_>, _>>()?
				.try_into()
				.map_err(|_| Error::<T>::BoundExceeded)?;
			let bounded_proof: BoundedVec<u8, ConstU32<128>> =
				payment_proof.try_into().map_err(|_| Error::<T>::BoundExceeded)?;

			let item_id = NextItemId::<T>::get();
			let next_id = item_id.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
			NextItemId::<T>::put(next_id);

			let now = frame_system::Pallet::<T>::block_number();
			let lt = license_type.clone();

			let data = LicenseDataOf::<T> {
				license_type,
				version,
				original_creator,
				current_owner: owner.clone(),
				original_price_eur_cents,
				original_price_plim,
				transferable,
				print_commercial,
				derivative_allowed,
				derivative_share_alike,
				attribution_required,
				watermark_required,
				exclusive_burns_original,
				royalty_pct_bp,
				expires_at,
				max_prints,
				max_copies,
				geo_restrictions: bounded_geo,
				platform_restrictions: bounded_platforms,
				jurisdiction,
				issued_at: now,
				payment_method: payment_method.clone(),
				payment_proof: bounded_proof,
			};

			Licenses::<T>::insert(item_id, data);

			// If paid via custody, create a custody record.
			if payment_method == PaymentMethod::Custody {
				if let Some(email_hash) = custody_buyer_email_hash {
					let custody = CustodyRecordOf::<T> {
						custodian: owner.clone(),
						buyer_email_hash: email_hash,
						created_at: now,
						claimed: false,
						claim_nonce: 0,
					};
					CustodyQueue::<T>::insert(item_id, custody);
				}
			}

			Self::deposit_event(Event::LicenseMinted { item_id, owner, license_type: lt });
			Ok(())
		}

		/// Update a license template's attributes. Only the original creator
		/// can update their own template.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::update_license_attrs())]
		pub fn update_license_attrs(
			origin: OriginFor<T>,
			template_hash: T::Hash,
			new_royalty_pct_bp: Option<u16>,
			new_transferable: Option<bool>,
			new_derivative_allowed: Option<bool>,
			new_derivative_share_alike: Option<bool>,
			new_max_prints: Option<Option<u32>>,
			new_max_copies: Option<Option<u32>>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			LicenseTemplates::<T>::try_mutate(template_hash, |maybe| -> DispatchResult {
				let data = maybe.as_mut().ok_or(Error::<T>::LicenseNotFound)?;
				ensure!(data.original_creator == caller, Error::<T>::Unauthorized);

				if let Some(royalty) = new_royalty_pct_bp {
					ensure!(royalty <= 2500, Error::<T>::InvalidRoyaltyPct);
					let transferable_val =
						new_transferable.unwrap_or(data.transferable);
					if royalty > 0 {
						ensure!(transferable_val, Error::<T>::InvalidLicenseAttrs);
					}
					data.royalty_pct_bp = royalty;
				}

				if let Some(t) = new_transferable {
					// If making non-transferable, royalties must be zero.
					if !t {
						ensure!(data.royalty_pct_bp == 0, Error::<T>::InvalidLicenseAttrs);
					}
					data.transferable = t;
				}

				if let Some(d) = new_derivative_allowed {
					if !d {
						data.derivative_share_alike = false;
					}
					data.derivative_allowed = d;
				}

				if let Some(sa) = new_derivative_share_alike {
					if sa {
						ensure!(data.derivative_allowed, Error::<T>::InvalidLicenseAttrs);
					}
					data.derivative_share_alike = sa;
				}

				if let Some(mp) = new_max_prints {
					data.max_prints = mp;
				}

				if let Some(mc) = new_max_copies {
					data.max_copies = mc;
				}

				Ok(())
			})?;

			Self::deposit_event(Event::LicenseAttrsUpdated { template_hash });
			Ok(())
		}

		/// Revoke (burn) a license. Restricted to AdminOrigin.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::revoke_license())]
		pub fn revoke_license(origin: OriginFor<T>, item_id: u32) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(Licenses::<T>::contains_key(item_id), Error::<T>::LicenseNotFound);

			Licenses::<T>::remove(item_id);
			CustodyQueue::<T>::remove(item_id);

			Self::deposit_event(Event::LicenseRevoked { item_id });
			Ok(())
		}

		/// Claim a custody-held license. The caller provides the expected nonce
		/// and becomes the new owner.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::claim_custody_license())]
		pub fn claim_custody_license(
			origin: OriginFor<T>,
			item_id: u32,
			claim_nonce: u64,
		) -> DispatchResult {
			let new_owner = ensure_signed(origin)?;

			CustodyQueue::<T>::try_mutate(item_id, |maybe| -> DispatchResult {
				let custody = maybe.as_mut().ok_or(Error::<T>::NotCustodyOwned)?;
				ensure!(!custody.claimed, Error::<T>::AlreadyClaimed);
				ensure!(custody.claim_nonce == claim_nonce, Error::<T>::CustodySignatureInvalid);

				custody.claimed = true;
				Ok(())
			})?;

			Licenses::<T>::try_mutate(item_id, |maybe| -> DispatchResult {
				let license = maybe.as_mut().ok_or(Error::<T>::LicenseNotFound)?;

				// Check not expired.
				if let Some(expires) = license.expires_at {
					let now = frame_system::Pallet::<T>::block_number();
					ensure!(now < expires, Error::<T>::LicenseExpired);
				}

				license.current_owner = new_owner.clone();
				Ok(())
			})?;

			Self::deposit_event(Event::LicenseClaimedFromCustody { item_id, new_owner });
			Ok(())
		}

		/// Burn a license. Only the current owner can burn.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::burn_license())]
		pub fn burn_license(origin: OriginFor<T>, item_id: u32) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let license =
				Licenses::<T>::get(item_id).ok_or(Error::<T>::LicenseNotFound)?;
			ensure!(license.current_owner == who, Error::<T>::Unauthorized);

			Licenses::<T>::remove(item_id);
			CustodyQueue::<T>::remove(item_id);

			Self::deposit_event(Event::LicenseBurned { item_id, owner: who });
			Ok(())
		}

		/// Set (or update) the caller's default royalty configuration.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_creator_config())]
		pub fn set_creator_config(
			origin: OriginFor<T>,
			default_pct_bp: u16,
			payout_address: T::AccountId,
			preferred_asset: RoyaltyAsset<u32>,
		) -> DispatchResult {
			let creator = ensure_signed(origin)?;
			ensure!(default_pct_bp <= 2500, Error::<T>::InvalidRoyaltyPct);

			let config = CreatorRoyaltyConfigOf::<T> {
				default_pct_bp,
				payout_address,
				preferred_asset,
			};

			CreatorConfigs::<T>::insert(&creator, config);

			Self::deposit_event(Event::CreatorConfigSet { creator });
			Ok(())
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers & public API for other pallets
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Validate license attribute invariants.
		fn validate_license_attrs(
			license_type: &LicenseType,
			royalty_pct_bp: u16,
			transferable: bool,
			exclusive_burns_original: bool,
			derivative_allowed: bool,
			derivative_share_alike: bool,
			expires_at: &Option<BlockNumberFor<T>>,
		) -> DispatchResult {
			// Royalty cap: 25% = 2500 bp.
			ensure!(royalty_pct_bp <= 2500, Error::<T>::InvalidRoyaltyPct);

			// If royalty > 0, license must be transferable (royalties only make
			// sense on secondary sales).
			if royalty_pct_bp > 0 {
				ensure!(transferable, Error::<T>::InvalidLicenseAttrs);
			}

			// exclusive_burns_original only valid for Exclusive licenses.
			if exclusive_burns_original {
				ensure!(
					*license_type == LicenseType::Exclusive,
					Error::<T>::InvalidLicenseAttrs
				);
			}

			// derivative_share_alike requires derivative_allowed.
			if derivative_share_alike {
				ensure!(derivative_allowed, Error::<T>::InvalidLicenseAttrs);
			}

			// TimeLimited requires an expiry block.
			if *license_type == LicenseType::TimeLimited {
				ensure!(expires_at.is_some(), Error::<T>::InvalidLicenseAttrs);
			}

			Ok(())
		}

		/// Check whether a license is transferable. Returns false if not found.
		pub fn is_transferable(item_id: u32) -> bool {
			Licenses::<T>::get(item_id).map_or(false, |l| l.transferable)
		}

		/// Get the royalty info for a license: (creator, basis_points).
		/// Returns None if the license does not exist or has zero royalties.
		pub fn royalty_info(item_id: u32) -> Option<(T::AccountId, u16)> {
			Licenses::<T>::get(item_id).and_then(|l| {
				if l.royalty_pct_bp > 0 {
					Some((l.original_creator, l.royalty_pct_bp))
				} else {
					None
				}
			})
		}
	}
}
