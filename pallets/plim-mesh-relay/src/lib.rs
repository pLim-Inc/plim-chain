//! # pallet-plim-mesh-relay
//!
//! L99 mesh-relay pallet — accepts offline payment transactions that arrived
//! via the L99 mesh / LoRa transport and records them for later reconciliation.
//!
//! Verifies on each `submit_relayed_transaction` call:
//!   1. **Idempotency by content hash** (mirrors the
//!      `pallet-plim-timestamps::Anchors` `contains_key` early-return pattern).
//!   2. **ed25519 signature** over the content hash by the originating mandate
//!      authority (verified via `sp_io::crypto::ed25519_verify`).
//!   3. **Hard cap** on the per-tx value (default 10 EUR equivalent in
//!      12-decimal stablecoin units), encoded as the trailing 8-byte
//!      little-endian u64 of the signed payload (v1 convention with Agent B's
//!      gateway adapter).
//!   4. **Bounded queue** capacity (default 10_000 records via
//!      `T::MaxRelayedQueue`).
//!
//! Storage layout (ISO 11179 column naming — every field carries an `_at`,
//! `_hash`, `_value`, `_id`, or `_code` suffix):
//!
//! - `RelayedTransactions` — `BoundedVec<OfflineTxRecord, MaxRelayedQueue>`,
//!   the canonical history.
//! - `RelayedHashIndex` — `StorageMap<content_hash → u32>` for O(1)
//!   idempotency checks (the index into `RelayedTransactions`).
//!
//! Spec source of truth: `docs/specs/L99_OODA_v1.md` §3.1.
//!
//! `[CONFIRM]` gate: deploying this pallet to mainnet runtime requires
//! Pedro's `[APPROVED]` reply on the PR description (which carries the
//! extrinsic weight + storage growth estimates from the benchmark output).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::ConstU32;
use scale_info::TypeInfo;
use sp_core::ed25519;

/// Max bytes per signed payload — matches `T::MaxPayloadLen` upper bound.
/// Hard-coded here so `OfflineTxRecord`'s `BoundedVec` parameter is monomorphic
/// (avoids leaking `T` through every public type for downstream consumers).
pub type OfflinePayloadCap = ConstU32<1024>;

/// One row per offline transaction relayed through L99 mesh transport.
///
/// ISO 11179 naming convention: every field carries an `_at`, `_hash`,
/// `_value`, `_code`, or `_id` suffix to disambiguate semantics from raw
/// scalar Rust types.
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug,
)]
pub struct OfflineTxRecord {
    /// Blake2-256 hash of `(mandate_ref || nonce || chain_id || payload)`.
    /// Primary identity for idempotency.
    pub offline_transaction_content_hash: [u8; 32],
    /// Block number at which this record was accepted on chain.
    pub offline_transaction_accepted_at_block: u32,
    /// ed25519 signature over `content_hash` by the mandate's authorising key.
    pub offline_transaction_signature_value: ed25519::Signature,
    /// Pubkey that produced the signature (audit trail + replay).
    pub offline_transaction_authority_pubkey_value: ed25519::Public,
    /// SCALE-encoded inner extrinsic that the gateway will dispatch into
    /// `pallet-plim-payments` once the relay reconciles online.
    pub offline_transaction_signed_payload: frame_support::BoundedVec<u8, OfflinePayloadCap>,
    /// Cumulative byte size of the original frame (billing / metrics).
    pub offline_transaction_size_bytes_value: u32,
    /// Mandate that authorised this offline tx (existing in
    /// `pallet-plim-payments::Mandates`).
    pub offline_transaction_mandate_ref_value: [u8; 32],
    /// Monotonic mandate-scoped nonce — replay protection.
    pub offline_transaction_mandate_nonce_value: u64,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::SaturatedConversion;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Aggregated runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Maximum offline tx records held simultaneously in the bounded queue.
        /// Spec §3.1 — 10_000.
        #[pallet::constant]
        type MaxRelayedQueue: Get<u32>;

        /// Maximum bytes per signed payload.
        #[pallet::constant]
        type MaxPayloadLen: Get<u32>;

        /// Default per-tx hard cap in 12-decimal stablecoin units.
        /// Spec §3.1 — €10 default = 10_000_000_000_000 (10 EUR @ 12 decimals).
        #[pallet::constant]
        type DefaultOfflineTxValueCap: Get<u128>;
    }

    #[pallet::storage]
    pub type RelayedTransactions<T: Config> =
        StorageValue<_, BoundedVec<OfflineTxRecord, T::MaxRelayedQueue>, ValueQuery>;

    /// `content_hash → index in RelayedTransactions` — O(1) idempotency lookup.
    /// Mirrors the `pallet-plim-timestamps::Anchors` pattern.
    #[pallet::storage]
    pub type RelayedHashIndex<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 32], u32, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// An offline tx was successfully relayed and recorded.
        OfflineTxAccepted {
            content_hash: [u8; 32],
            mandate_ref: [u8; 32],
            mandate_nonce: u64,
            accepted_at_block: u32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Same content_hash already relayed — idempotent NoOp.
        AlreadyRelayed,
        /// ed25519 signature verification failed.
        InvalidSignature,
        /// Decoded value exceeds `DefaultOfflineTxValueCap`.
        ExceedsOfflineCap,
        /// `RelayedTransactions` BoundedVec is full.
        QueueFull,
        /// Payload exceeds `MaxPayloadLen`.
        PayloadTooLarge,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Accept an offline-relayed transaction. Idempotent by content hash.
        ///
        /// Spec §3.1: "verifies ed25519 signature, checks the relayed tx is
        /// within the offline mandate of the originating DID, and dispatches
        /// to pallet-plim-payments as if it had arrived online. Idempotent by
        /// content hash."
        ///
        /// v1 design notes:
        /// - The dispatch into `pallet-plim-payments` is deferred to the
        ///   gateway adapter (Agent B); this pallet only **records** the
        ///   accepted offline tx for the gateway to pick up.
        /// - The mandate existence / allowance checks happen at gateway
        ///   reconciliation; this pallet only enforces hard caps on raw
        ///   payload value to prevent abuse of the relay storage itself.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(50_000_000, 1024))]
        pub fn submit_relayed_transaction(
            origin: OriginFor<T>,
            signed_payload: alloc::vec::Vec<u8>,
            signature: ed25519::Signature,
            authority_pubkey: ed25519::Public,
            mandate_ref: [u8; 32],
            mandate_nonce: u64,
        ) -> DispatchResult {
            let _who = ensure_signed(origin)?;

            // 1. Bound the payload eagerly.
            let payload_len = signed_payload.len() as u32;
            ensure!(
                payload_len <= T::MaxPayloadLen::get(),
                Error::<T>::PayloadTooLarge
            );

            // 2. Compute content hash =
            //    blake2_256(mandate_ref || nonce_le || chain_id || payload).
            //    chain_id derived from frame_system genesis hash so a relay
            //    intended for testnet cannot replay against mainnet.
            let chain_id =
                <frame_system::Pallet<T>>::block_hash(0u32.saturated_into::<BlockNumberFor<T>>());
            let mut hasher_input =
                alloc::vec::Vec::with_capacity(32 + 8 + 32 + signed_payload.len());
            hasher_input.extend_from_slice(&mandate_ref);
            hasher_input.extend_from_slice(&mandate_nonce.to_le_bytes());
            hasher_input.extend_from_slice(chain_id.as_ref());
            hasher_input.extend_from_slice(&signed_payload);
            let content_hash = sp_io::hashing::blake2_256(&hasher_input);

            // 3. Idempotency check FIRST — duplicate-relay attempts must be
            //    cheap (skip signature verify on known hashes).
            ensure!(
                !RelayedHashIndex::<T>::contains_key(content_hash),
                Error::<T>::AlreadyRelayed
            );

            // 4. ed25519 signature must verify over the content hash.
            ensure!(
                sp_io::crypto::ed25519_verify(&signature, &content_hash, &authority_pubkey),
                Error::<T>::InvalidSignature
            );

            // 5. Hard cap: extract trailing u64 value from payload (v1
            //    convention agreed with Agent B's gateway adapter — once
            //    mesh.adapter.ts is merged this becomes a SCALE-decoded
            //    extraction of the inner extrinsic's amount field).
            if signed_payload.len() >= 8 {
                let val_bytes: [u8; 8] = signed_payload[signed_payload.len() - 8..]
                    .try_into()
                    .unwrap_or([0u8; 8]);
                let candidate_value = u64::from_le_bytes(val_bytes) as u128;
                ensure!(
                    candidate_value <= T::DefaultOfflineTxValueCap::get(),
                    Error::<T>::ExceedsOfflineCap
                );
            }

            // 6. Build the record and append into the bounded queue.
            let bounded_payload: BoundedVec<u8, OfflinePayloadCap> = signed_payload
                .try_into()
                .map_err(|_| Error::<T>::PayloadTooLarge)?;

            let now_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
            let payload_size = bounded_payload.len() as u32;

            let record = OfflineTxRecord {
                offline_transaction_content_hash: content_hash,
                offline_transaction_accepted_at_block: now_block,
                offline_transaction_signature_value: signature,
                offline_transaction_authority_pubkey_value: authority_pubkey,
                offline_transaction_signed_payload: bounded_payload,
                offline_transaction_size_bytes_value: payload_size,
                offline_transaction_mandate_ref_value: mandate_ref,
                offline_transaction_mandate_nonce_value: mandate_nonce,
            };

            RelayedTransactions::<T>::try_mutate(|queue| -> DispatchResult {
                queue
                    .try_push(record)
                    .map_err(|_| Error::<T>::QueueFull.into())
            })?;

            let new_idx = RelayedTransactions::<T>::decode_len()
                .map(|n| (n - 1) as u32)
                .unwrap_or(0);
            RelayedHashIndex::<T>::insert(content_hash, new_idx);

            Self::deposit_event(Event::OfflineTxAccepted {
                content_hash,
                mandate_ref,
                mandate_nonce,
                accepted_at_block: now_block,
            });
            Ok(())
        }
    }
}
