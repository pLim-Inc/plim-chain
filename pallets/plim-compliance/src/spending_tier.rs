//! §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23.
//!
//! Spending tiers gate per-account daily + per-tx caps, and require a
//! minimum KYC level. Tiers are denominated in EUR-cents (1€ = 100 units)
//! so the runtime's actual `Balance` decimals are decoupled from the
//! human-readable spec values. The runtime is responsible for scaling
//! `BalanceOf<T>` → cents when comparing.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

use crate::kyc_attestation::KYCLevel;
use crate::pallet::Config;

/// Balance alias scoped to the pallet — the runtime fills it via
/// `pallet_balances::Pallet<Runtime>` once the runtime config wires the
/// `Currency` associated type in a follow-up PR. For now this just
/// resolves to `u128` everywhere via the default frame-system trait
/// surface; the actual `Balance` substitution happens at the call site.
///
/// TODO §6: once `Config: pallet_balances::Config` is added (separate
/// PR), switch this to `<T as pallet_balances::Config>::Balance`.
pub type BalanceOf<T> = <T as Config>::Balance;

/// Custom-tier caps for institutional accounts (ST_CUSTOM).
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct SpendingTierCustomCaps<T: Config> {
	pub daily_cap: BalanceOf<T>,
	pub per_tx_cap: BalanceOf<T>,
	pub ct_required: KYCLevel,
}

/// Spending tier per v2 spec.
///
/// Cents-precision (1€ = 100). Standard tiers:
///  - ST1: daily 100€,    per-tx 100€    → Basic CT
///  - ST2: daily 1_000€,  per-tx 500€    → Enhanced CT
///  - ST3: daily 10_000€, per-tx 5_000€  → Enhanced CT
///  - ST4: daily 50_000€, per-tx 25_000€ → BusinessVerified CT
///  - ST_CUSTOM: institutional, caps + CT carried in the variant data
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
#[allow(non_camel_case_types)]
pub enum SpendingTier<T: Config> {
	ST1,
	ST2,
	ST3,
	ST4,
	ST_CUSTOM(SpendingTierCustomCaps<T>),
}

impl<T: Config> SpendingTier<T> {
	/// Daily cumulative spend cap in `BalanceOf<T>` (cents-precision).
	pub fn daily_cap(&self) -> BalanceOf<T>
	where
		BalanceOf<T>: From<u32>,
	{
		match self {
			// 100€ = 10_000 cents
			SpendingTier::ST1 => BalanceOf::<T>::from(100u32 * 100),
			// 1_000€ = 100_000 cents
			SpendingTier::ST2 => BalanceOf::<T>::from(1_000u32 * 100),
			// 10_000€ = 1_000_000 cents
			SpendingTier::ST3 => BalanceOf::<T>::from(10_000u32 * 100),
			// 50_000€ = 5_000_000 cents
			SpendingTier::ST4 => BalanceOf::<T>::from(50_000u32 * 100),
			SpendingTier::ST_CUSTOM(caps) => caps.daily_cap.clone(),
		}
	}

	/// Per-transaction cap in `BalanceOf<T>` (cents-precision).
	pub fn per_tx_cap(&self) -> BalanceOf<T>
	where
		BalanceOf<T>: From<u32>,
	{
		match self {
			SpendingTier::ST1 => BalanceOf::<T>::from(100u32 * 100),
			SpendingTier::ST2 => BalanceOf::<T>::from(500u32 * 100),
			SpendingTier::ST3 => BalanceOf::<T>::from(5_000u32 * 100),
			SpendingTier::ST4 => BalanceOf::<T>::from(25_000u32 * 100),
			SpendingTier::ST_CUSTOM(caps) => caps.per_tx_cap.clone(),
		}
	}

	/// Minimum KYC level required to operate at this tier ("CT required").
	pub fn ct_required(&self) -> KYCLevel {
		match self {
			SpendingTier::ST1 => KYCLevel::Basic,
			SpendingTier::ST2 => KYCLevel::Enhanced,
			SpendingTier::ST3 => KYCLevel::Enhanced,
			SpendingTier::ST4 => KYCLevel::BusinessVerified,
			SpendingTier::ST_CUSTOM(caps) => caps.ct_required,
		}
	}
}
