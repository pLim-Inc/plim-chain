//! §6 KYC/AML v2 — created by parallel-agent dispatch 2026-05-23.
//!
//! Section 6 extrinsics — canonical spec.
//!
//! NOTE FOR INTEGRATOR: FRAME pallets require `#[pallet::call]` items to
//! live INSIDE the `#[frame_support::pallet] pub mod pallet { ... }`
//! block. The extrinsic below is documented HERE as the source-of-truth
//! signature, but actually DECLARED in `lib.rs` in the
//! `// ===== §6 KYC/AML v2 =====` region.
//!
//! ## `register_kyc_attestation`
//!
//! Picked `call_index(3)` — next free in the current pallet (existing
//! calls occupy 0 = `sanction`, 1 = `unsanction`, 2 = `mark_regulated`).
//! The v2 spec references "call_index 4" as a placeholder; the actual
//! contiguous index inside this pallet is 3.
//!
//! ```ignore
//! #[pallet::call_index(3)]
//! #[pallet::weight(Weight::from_parts(20_000, 0))]
//! pub fn register_kyc_attestation(
//!     origin: OriginFor<T>,
//!     subject: T::AccountId,
//!     attestation_hash: T::Hash,
//!     jurisdiction: BoundedVec<u8, T::MaxJurisdictionLength>,
//!     expires_at: BlockNumberFor<T>,
//!     kyc_level: KYCLevel,
//!     eidas_present: bool,
//! ) -> DispatchResult {
//!     // TODO §6: wire ComplianceOrigin to multisig in runtime
//!     T::ComplianceOrigin::ensure_origin(origin)?;
//!
//!     let now = <frame_system::Pallet<T>>::block_number();
//!     let attestation = KYCAttestation::<T> {
//!         hash: attestation_hash,
//!         jurisdiction,
//!         attested_at: now,
//!         expires_at,
//!         level: kyc_level,
//!         eidas_present,
//!     };
//!
//!     KYCStatus::<T>::insert(&subject, kyc_level);
//!     KYCAttestations::<T>::insert(&subject, attestation);
//!
//!     Self::deposit_event(Event::KYCAttestationRegistered {
//!         subject,
//!         kyc_level,
//!         attestation_hash,
//!         expires_at,
//!         eidas_present,
//!     });
//!     Ok(())
//! }
//! ```
//!
//! ### TODOs for the integrator
//!  - TODO §6: confirm call_index does not collide once §6 lands on
//!    `feature/plimcontrol-section6-kyc-aml-v2`.
//!  - TODO §6: wire `ComplianceOrigin` to a 3-of-5 council multisig in
//!    the runtime (currently `EnsureRoot<AccountId>`).
//!  - TODO §6: replace placeholder weight (`20_000`) with a benchmarked
//!    weight via `pallet-benchmarking`.
//!  - TODO §6: emit an additional `RuntimeEvent` to bridge into
//!    `pallet-plim-kyc` for the existing chain-side KYC oracle.
