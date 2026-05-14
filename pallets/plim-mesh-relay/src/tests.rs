//! Unit tests for `pallet-plim-mesh-relay`.
//!
//! Coverage:
//!   1. First-call accept inserts exactly one record.
//!   2. Idempotency by content hash — second call with identical
//!      `(mandate_ref, nonce, payload)` returns `AlreadyRelayed` and storage
//!      stays at one record.
//!   3. Bad signature rejected with `InvalidSignature`.
//!   4. Hard cap enforced when payload trailing-u64 exceeds the configured
//!      cap (`ExceedsOfflineCap`).
//!   5. Payload exceeding `MaxPayloadLen` rejected with `PayloadTooLarge`.

#![cfg(test)]

use crate::{mock::*, Error, Event, OfflineTxRecord, RelayedHashIndex, RelayedTransactions};
use frame_support::{assert_noop, assert_ok};
use sp_core::{ed25519, Pair};

/// Build a payload whose trailing 8 bytes encode `value_le` (the convention
/// used by the hard-cap check inside `submit_relayed_transaction`).
fn payload_with_trailing_value(prefix: &[u8], value_le: u64) -> alloc::vec::Vec<u8> {
    let mut v = alloc::vec::Vec::from(prefix);
    v.extend_from_slice(&value_le.to_le_bytes());
    v
}

/// Compute the content hash exactly as the extrinsic computes it. Required
/// to produce a valid signature in the tests.
fn compute_content_hash(mandate_ref: &[u8; 32], nonce: u64, payload: &[u8]) -> [u8; 32] {
    let chain_id = <frame_system::Pallet<Test>>::block_hash(0u64);
    let mut input = alloc::vec::Vec::with_capacity(32 + 8 + 32 + payload.len());
    input.extend_from_slice(mandate_ref);
    input.extend_from_slice(&nonce.to_le_bytes());
    input.extend_from_slice(chain_id.as_ref());
    input.extend_from_slice(payload);
    sp_io::hashing::blake2_256(&input)
}

fn signed_call(
    payload: alloc::vec::Vec<u8>,
    mandate_ref: [u8; 32],
    nonce: u64,
) -> (
    alloc::vec::Vec<u8>,
    ed25519::Signature,
    ed25519::Public,
    [u8; 32],
    u64,
) {
    let pair = ed25519::Pair::from_seed(&[1u8; 32]);
    let pubkey = pair.public();
    let content_hash = compute_content_hash(&mandate_ref, nonce, &payload);
    let sig = pair.sign(&content_hash);
    (payload, sig, pubkey, mandate_ref, nonce)
}

extern crate alloc;

#[test]
fn submit_relayed_transaction_accepts_first_call() {
    new_test_ext().execute_with(|| {
        let payload = payload_with_trailing_value(b"pay-alice-", 5_000_000_000_000); // 5 EUR
        let mandate_ref = [0u8; 32];
        let (payload, sig, pubkey, mref, nonce) = signed_call(payload, mandate_ref, 1);

        assert_ok!(PlimMeshRelay::submit_relayed_transaction(
            RuntimeOrigin::signed(1),
            payload.clone(),
            sig,
            pubkey,
            mref,
            nonce,
        ));

        let queue = RelayedTransactions::<Test>::get();
        assert_eq!(queue.len(), 1);
        let row: &OfflineTxRecord = &queue[0];
        assert_eq!(row.offline_transaction_mandate_nonce_value, nonce);
        assert_eq!(
            row.offline_transaction_size_bytes_value,
            payload.len() as u32
        );
        // Hash index must point at slot 0.
        let expected_hash = compute_content_hash(&mref, nonce, &payload);
        assert_eq!(RelayedHashIndex::<Test>::get(expected_hash), Some(0));
        // Event emitted.
        System::assert_last_event(
            Event::OfflineTxAccepted {
                content_hash: expected_hash,
                mandate_ref: mref,
                mandate_nonce: nonce,
                accepted_at_block: 1,
            }
            .into(),
        );
    });
}

#[test]
fn submit_relayed_transaction_is_idempotent_by_content_hash() {
    new_test_ext().execute_with(|| {
        let payload = payload_with_trailing_value(b"pay-bob-", 1_000_000_000_000);
        let (payload, sig, pubkey, mref, nonce) = signed_call(payload, [7u8; 32], 1);

        // First call OK.
        assert_ok!(PlimMeshRelay::submit_relayed_transaction(
            RuntimeOrigin::signed(1),
            payload.clone(),
            sig,
            pubkey,
            mref,
            nonce,
        ));

        // Second call with identical (mandate, nonce, payload) MUST early-return AlreadyRelayed.
        assert_noop!(
            PlimMeshRelay::submit_relayed_transaction(
                RuntimeOrigin::signed(1),
                payload,
                sig,
                pubkey,
                mref,
                nonce,
            ),
            Error::<Test>::AlreadyRelayed
        );

        // Storage still has exactly one entry — no duplicate row.
        assert_eq!(RelayedTransactions::<Test>::get().len(), 1);
    });
}

#[test]
fn submit_relayed_transaction_rejects_bad_signature() {
    new_test_ext().execute_with(|| {
        let payload = payload_with_trailing_value(b"pay-eve-", 1_000_000_000_000);
        let pair = ed25519::Pair::from_seed(&[1u8; 32]);
        let pubkey = pair.public();
        // Sign a DIFFERENT message.
        let bad_sig = pair.sign(b"not-the-real-content-hash");

        assert_noop!(
            PlimMeshRelay::submit_relayed_transaction(
                RuntimeOrigin::signed(1),
                payload,
                bad_sig,
                pubkey,
                [0u8; 32],
                1u64,
            ),
            Error::<Test>::InvalidSignature
        );
        assert_eq!(RelayedTransactions::<Test>::get().len(), 0);
    });
}

#[test]
fn submit_relayed_transaction_enforces_hard_cap() {
    new_test_ext().execute_with(|| {
        // 20 EUR — exceeds the 10 EUR DefaultOfflineTxValueCap.
        let payload = payload_with_trailing_value(b"oversize-", 20_000_000_000_000);
        let (payload, sig, pubkey, mref, nonce) = signed_call(payload, [0u8; 32], 1);

        assert_noop!(
            PlimMeshRelay::submit_relayed_transaction(
                RuntimeOrigin::signed(1),
                payload,
                sig,
                pubkey,
                mref,
                nonce,
            ),
            Error::<Test>::ExceedsOfflineCap
        );
        assert_eq!(RelayedTransactions::<Test>::get().len(), 0);
    });
}

#[test]
fn submit_relayed_transaction_rejects_payload_too_large() {
    new_test_ext().execute_with(|| {
        // MaxPayloadLen in mock = 1024 — go one byte over.
        let payload: alloc::vec::Vec<u8> = alloc::vec![0u8; 1025];
        let pair = ed25519::Pair::from_seed(&[1u8; 32]);
        let pubkey = pair.public();
        let content_hash = compute_content_hash(&[0u8; 32], 1, &payload);
        let sig = pair.sign(&content_hash);

        assert_noop!(
            PlimMeshRelay::submit_relayed_transaction(
                RuntimeOrigin::signed(1),
                payload,
                sig,
                pubkey,
                [0u8; 32],
                1u64,
            ),
            Error::<Test>::PayloadTooLarge
        );
    });
}
