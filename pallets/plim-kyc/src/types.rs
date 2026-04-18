//! Shared types for pallet-plim-kyc.
//!
//! Stores **no PII** on chain — only the verification level, expiry, and a
//! 32-byte hash of the off-chain document bundle held by the attestor.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_core::H256;

use crate::Config;

/// KYC verification level for an account.
///
/// Levels are strictly ordered: `None < Basic < Enhanced < Institutional`.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
)]
pub enum KycLevel {
	/// No verification (default for unknown accounts).
	None = 0,
	/// Basic identity verification (email + government ID).
	Basic = 1,
	/// Enhanced verification (proof of address, source of funds).
	Enhanced = 2,
	/// Institutional / accredited investor verification.
	Institutional = 3,
}

impl Default for KycLevel {
	fn default() -> Self {
		KycLevel::None
	}
}

/// Reason an account appears on the on-chain sanction list.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
	PartialEq,
	Eq,
)]
pub enum SanctionReason {
	/// US OFAC Specially Designated Nationals list.
	OfacSdn,
	/// EU consolidated sanctions list.
	EuSanctions,
	/// UK HMT sanctions list.
	UkSanctions,
	/// Internal compliance decision.
	Internal,
}

/// On-chain KYC record.
///
/// Contains **no PII** — only the verification level, attestor metadata,
/// expiry block, country code, and a hash of the off-chain document bundle.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, frame_support::DebugNoBound,
	frame_support::PartialEqNoBound, frame_support::EqNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct KycRecord<T: Config> {
	/// KYC verification level achieved.
	pub level: KycLevel,
	/// Attestor account that signed/submitted this record.
	pub attested_by: T::AccountId,
	/// Block number at which the record was attested.
	pub attested_at: BlockNumberFor<T>,
	/// Block number at which the record expires.
	pub expires_at: BlockNumberFor<T>,
	/// Hash of the off-chain document bundle (e.g. blake2-256 of the
	/// concatenated KYC documents held by the attestor).
	pub document_hash: H256,
	/// ISO-3166-1 alpha-2 country code of the verified subject.
	pub country_code: [u8; 2],
}
