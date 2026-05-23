//! §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23.
//!
//! On-chain commitment record for an off-chain KYC verification.
//!
//! Rationale: NO PII goes on-chain. The chain stores only:
//!  - a commitment `hash` (e.g. keccak256 over the off-chain dossier),
//!  - the verifying `jurisdiction` (ISO-3166 country code, bounded),
//!  - the block height range `[attested_at, expires_at]`,
//!  - the granted KYC `level`,
//!  - and a flag indicating whether an eIDAS-grade signature was
//!    presented at verification time.
//!
//! GDPR + eIDAS compliance: the on-chain footprint is content-addressed
//! and rotatable — re-attestation overwrites the hash, expired entries
//! are dropped via off-chain workers (TODO §6: housekeeping pallet hook).

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;

use crate::pallet::Config;

/// KYC tiers per v2 spec.
///
/// Ordering is intentional (`None < Basic < ... < GovernmentVerified`)
/// so that gating logic can compare with `>=`.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum KYCLevel {
	None,
	Basic,
	Enhanced,
	BusinessVerified,
	GovernmentVerified,
}

impl Default for KYCLevel {
	fn default() -> Self {
		KYCLevel::None
	}
}

/// Per-account KYC attestation record stored on-chain.
///
/// Bounded by `T::MaxJurisdictionLength` so MaxEncodedLen is well-defined.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct KYCAttestation<T: Config> {
	/// Commitment hash over the off-chain dossier (no PII on-chain).
	pub hash: T::Hash,
	/// ISO-3166 alpha-2/alpha-3 country code of the verifying jurisdiction.
	pub jurisdiction: BoundedVec<u8, T::MaxJurisdictionLength>,
	/// Block at which the attestation was registered.
	pub attested_at: BlockNumberFor<T>,
	/// Block at which the attestation expires and must be renewed.
	pub expires_at: BlockNumberFor<T>,
	/// Granted KYC tier.
	pub level: KYCLevel,
	/// Whether an eIDAS-grade qualified signature was presented at
	/// verification time (uplifts trust for cross-border CT routing).
	pub eidas_present: bool,
}
