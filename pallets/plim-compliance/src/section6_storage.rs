//! §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23.
//!
//! Storage layout for §6.
//!
//! NOTE FOR INTEGRATOR: FRAME pallets require `#[pallet::storage]` items
//! to live INSIDE the `#[frame_support::pallet] pub mod pallet { ... }`
//! block. Therefore the storage items below are documented HERE but
//! actually DECLARED in `lib.rs` in the `// ===== §6 KYC/AML v2 =====`
//! region. This file exists as the canonical specification and as the
//! place to add helper accessors later.
//!
//! Storage items added in lib.rs (canonical schema):
//!
//! ```ignore
//! /// Per-account KYC tier (cached for O(1) gate checks).
//! #[pallet::storage]
//! pub type KYCStatus<T: Config> =
//!     StorageMap<_, Blake2_128Concat, T::AccountId, KYCLevel, ValueQuery>;
//!
//! /// Per-account commitment-hash attestation. Read by oracles + UI;
//! /// expiry enforced off-chain via housekeeping worker.
//! #[pallet::storage]
//! pub type KYCAttestations<T: Config> =
//!     StorageMap<_, Blake2_128Concat, T::AccountId, KYCAttestation<T>, OptionQuery>;
//!
//! /// Per-account spending tier. Drives caps + CT-required checks.
//! #[pallet::storage]
//! pub type SpendingTierStorage<T: Config> =
//!     StorageMap<_, Blake2_128Concat, T::AccountId, SpendingTier<T>, OptionQuery>;
//! ```
//!
//! Rationale recap:
//!  - Commitment hash only — no PII ever lands on-chain (GDPR Art. 17
//!    "right to erasure" satisfied because nothing erasable is stored).
//!  - `KYCStatus` is a fast cached read for hot-path gates; the full
//!    `KYCAttestations` record is consulted only when the cache misses
//!    or when an auditor wants the jurisdiction + expiry.
//!  - `SpendingTierStorage` is separated from KYC so that compliance
//!    can downgrade a tier without re-running the whole KYC flow.
