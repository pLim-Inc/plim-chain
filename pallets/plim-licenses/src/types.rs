use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::BoundedVec;
use frame_support::pallet_prelude::ConstU32;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Type of license governing usage rights for a 3D model NFT.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub enum LicenseType {
	/// Personal, non-commercial use only.
	Personal,
	/// Commercial use permitted.
	Commercial,
	/// Derivative works permitted.
	Derivative,
	/// Exclusive ownership; may burn the original.
	Exclusive,
	/// License valid only until a specific block.
	TimeLimited,
	/// Custom terms (details stored off-chain).
	Custom,
}

impl Default for LicenseType {
	fn default() -> Self {
		LicenseType::Personal
	}
}

/// Legal jurisdiction under which the license is issued.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub enum Jurisdiction {
	ES,
	EU,
	CH,
	US,
	Global,
}

impl Default for Jurisdiction {
	fn default() -> Self {
		Jurisdiction::Global
	}
}

/// How the license was paid for.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub enum PaymentMethod {
	StripeFiat,
	PLIM,
	PEUR,
	Custody,
}

impl Default for PaymentMethod {
	fn default() -> Self {
		PaymentMethod::StripeFiat
	}
}

/// Which asset royalties should be paid in.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub enum RoyaltyAsset<AssetId> {
	/// Native chain token (PLIM).
	Native,
	/// A pallet-assets asset (e.g. pEUR, pUSD).
	Asset(AssetId),
	/// Off-chain settlement (Stripe, etc.).
	OffChain,
}

impl<AssetId> Default for RoyaltyAsset<AssetId> {
	fn default() -> Self {
		RoyaltyAsset::Native
	}
}

/// Full license metadata attached to an NFT item.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
#[scale_info(skip_type_params(MaxGeo, MaxPlatform))]
pub struct LicenseData<AccountId, Balance, BlockNumber, MaxGeo, MaxPlatform>
where
	MaxGeo: frame_support::traits::Get<u32>,
	MaxPlatform: frame_support::traits::Get<u32>,
{
	// --- Type & ownership ---
	pub license_type: LicenseType,
	/// Schema version for future migrations.
	pub version: u16,
	pub original_creator: AccountId,
	pub current_owner: AccountId,

	// --- Pricing ---
	/// Original price in EUR cents (e.g. 2999 = 29.99 EUR).
	pub original_price_eur_cents: u32,
	/// Original price in PLIM tokens.
	pub original_price_plim: Balance,

	// --- Permissions ---
	pub transferable: bool,
	pub print_commercial: bool,
	pub derivative_allowed: bool,
	pub derivative_share_alike: bool,
	pub attribution_required: bool,
	pub watermark_required: bool,
	/// If true, claiming this exclusive license burns the original NFT.
	pub exclusive_burns_original: bool,

	// --- Limits ---
	/// Royalty percentage in basis points (max 2500 = 25%).
	pub royalty_pct_bp: u16,
	/// Block number at which the license expires (None = perpetual).
	pub expires_at: Option<BlockNumber>,
	/// Maximum number of physical prints allowed (None = unlimited).
	pub max_prints: Option<u32>,
	/// Maximum number of digital copies allowed (None = unlimited).
	pub max_copies: Option<u32>,

	// --- Restrictions ---
	/// ISO-3166-1 alpha-2 country codes where the license is NOT valid.
	pub geo_restrictions: BoundedVec<[u8; 2], MaxGeo>,
	/// Platform identifiers where the license is NOT valid.
	pub platform_restrictions: BoundedVec<BoundedVec<u8, ConstU32<64>>, MaxPlatform>,
	pub jurisdiction: Jurisdiction,

	// --- Audit trail ---
	/// Block at which the license was issued.
	pub issued_at: BlockNumber,
	pub payment_method: PaymentMethod,
	/// External payment reference (e.g. Stripe charge ID hash), max 128 bytes.
	pub payment_proof: BoundedVec<u8, ConstU32<128>>,
}

/// Custody record for licenses held on behalf of off-chain buyers.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub struct CustodyRecord<AccountId, BlockNumber> {
	/// The custodian account holding the NFT.
	pub custodian: AccountId,
	/// SHA-256 hash of the buyer's email for privacy.
	pub buyer_email_hash: [u8; 32],
	/// Block at which the custody record was created.
	pub created_at: BlockNumber,
	/// Whether the buyer has claimed the NFT.
	pub claimed: bool,
	/// Nonce to prevent replay on claim.
	pub claim_nonce: u64,
}

/// Per-creator default royalty configuration.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub struct CreatorRoyaltyConfig<AccountId, AssetId> {
	/// Default royalty percentage in basis points.
	pub default_pct_bp: u16,
	/// Account to receive royalty payouts.
	pub payout_address: AccountId,
	/// Preferred asset for royalty payment.
	pub preferred_asset: RoyaltyAsset<AssetId>,
}
