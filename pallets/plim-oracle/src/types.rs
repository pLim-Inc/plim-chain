//! Type definitions for pallet-plim-oracle.

use crate::Config;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Asset pair priced by the oracle.
///
/// All rates are quoted as `base / quote` in micro-units (rate * 1_000_000).
/// EUR is the universal quote currency for v1.
#[derive(
	Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen,
	RuntimeDebug, Hash,
)]
pub enum AssetPair {
	/// PLIM native token priced in EUR.
	PlimEur,
	/// pEUR stablecoin priced in EUR (should be ~1.0).
	PeurEur,
	/// Bitcoin priced in EUR.
	BtcEur,
	/// Ether priced in EUR.
	EthEur,
}

/// An active oracle rate that has reached quorum and can be consumed by other
/// pallets via the [`crate::RateProvider`] trait.
#[derive(Clone, Encode, Decode, PartialEq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
#[scale_info(skip_type_params(T))]
pub struct OracleRate<T: Config> {
	/// The agreed rate, scaled by 1_000_000.
	pub rate_micros: u64,
	/// Block at which the quorum was reached.
	pub updated_at: BlockNumberFor<T>,
	/// The set of updaters whose proposals contributed to this rate.
	pub quorum_attesters: BoundedVec<T::AccountId, T::MaxUpdaters>,
}

/// A single updater's pending proposal awaiting quorum.
#[derive(Clone, Encode, Decode, PartialEq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
#[scale_info(skip_type_params(T))]
pub struct PendingRate<T: Config> {
	/// The proposed rate, scaled by 1_000_000.
	pub rate_micros: u64,
	/// Block at which this proposal was submitted.
	pub proposed_at: BlockNumberFor<T>,
}
