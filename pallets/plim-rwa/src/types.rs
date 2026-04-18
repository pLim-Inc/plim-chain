//! Types for `pallet-rwa`.
//!
//! Mirrors the marketplace `Currency` enum locally to avoid a hard dependency
//! on any other pallet — the orchestrator wires equivalent enums together at
//! the runtime layer if/when needed.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::ConstU32;
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::RuntimeDebug;

use crate::{BalanceOf, Config, KycLevel};

/// Settlement currency for RWA mints, NAV reporting, and yield distributions.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug, Copy,
)]
pub enum Currency {
	/// Native PLIM token.
	Plim,
	/// On-chain pEUR stablecoin (pallet-assets).
	PEur,
	/// Off-chain fiat settlement (Stripe / SEPA).
	Fiat,
}

impl Default for Currency {
	fn default() -> Self {
		Currency::Plim
	}
}

/// Lifecycle status of an RWA asset.
///
/// NB: the original spec PDF used "Wound Down" with a space — this enum
/// uses `WoundDown` to satisfy Rust identifier rules.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug, Copy,
)]
pub enum RwaStatus {
	/// Asset is fully operational: mints, transfers, distributions allowed.
	Active,
	/// Mints/transfers blocked; existing yield can still be claimed.
	Frozen,
	/// Terminal state: asset has been wound down / liquidated.
	WoundDown,
}

impl Default for RwaStatus {
	fn default() -> Self {
		RwaStatus::Active
	}
}

/// Off-chain payment proof attached to an on-chain mint.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug,
)]
pub struct PaymentProof<AccountId, Balance> {
	pub payer: AccountId,
	pub amount: Balance,
	pub proof_hash: H256,
}

/// On-chain registration of a real-world asset.
#[derive(
	frame_support::CloneNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	frame_support::PartialEqNoBound,
	frame_support::EqNoBound,
	TypeInfo,
	MaxEncodedLen,
	frame_support::DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct RwaAsset<T: Config> {
	/// Short trading symbol, e.g. `PLIM-RE1` (max 16 bytes).
	pub symbol: BoundedVec<u8, ConstU32<16>>,
	/// Human-readable name (max 128 bytes).
	pub name: BoundedVec<u8, ConstU32<128>>,
	/// Hash of the off-chain prospectus / description.
	pub description_hash: H256,
	/// Maximum issuable supply (cap).
	pub total_supply: BalanceOf<T>,
	/// Minimum KYC level required to hold/transfer.
	pub kyc_required: KycLevel,
	/// Currency yield is paid in.
	pub yield_currency: Currency,
	/// Currency NAV is reported in.
	pub nav_currency: Currency,
	/// ISO-3166-1 alpha-2 country code (e.g. `*b"CH"`).
	pub jurisdiction: [u8; 2],
	/// Block height at which the asset was registered.
	pub created_at_block: BlockNumberFor<T>,
	/// Account authorised to mint, distribute yield, etc.
	pub manager: T::AccountId,
}

/// On-chain record of a yield distribution event.
#[derive(
	frame_support::CloneNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	frame_support::PartialEqNoBound,
	frame_support::EqNoBound,
	TypeInfo,
	MaxEncodedLen,
	frame_support::DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct YieldDistribution<T: Config> {
	/// Total amount being distributed pro-rata.
	pub total_amount: BalanceOf<T>,
	/// Currency in which the yield is paid.
	pub currency: Currency,
	/// Block at which the manager triggered distribution.
	pub distributed_at: BlockNumberFor<T>,
	/// Block whose shareholder snapshot was used (== distributed_at here).
	pub snapshot_at: BlockNumberFor<T>,
	/// Hash of off-chain narrative / accounting docs.
	pub description_hash: H256,
	/// Amount still claimable. Decreases on each `claim_yield` call;
	/// includes any rounding remainder left in the pot for accounting.
	pub remaining_unclaimed: BalanceOf<T>,
}
