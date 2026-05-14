//! Benchmarks for `pallet-plim-mesh-relay`.
//!
//! Run with the standard pLim/Chain frame-omni-bencher harness:
//!
//! ```text
//! cd plim-chain
//! cargo build --release --features runtime-benchmarks
//! ./target/release/plim-chain benchmark pallet \
//!     --pallet pallet-plim-mesh-relay \
//!     --extrinsic '*' \
//!     --steps 50 --repeat 20 \
//!     --output pallets/plim-mesh-relay/src/weights.rs
//! ```
//!
//! Until weights.rs is generated, the pallet uses the static placeholder
//! weight `Weight::from_parts(50_000_000, 1024)` declared in `lib.rs`. That
//! placeholder is conservative for the worst case (1 KiB payload, ed25519
//! verification, blake2_256 hashing, 1 storage write to a BoundedVec value
//! and 1 write to a StorageMap entry).

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_core::crypto::KeyTypeId;

/// Key type used only by this benchmark to mint an ed25519 keypair in the
/// runtime keystore. `sp_core::ed25519::Pair::sign` requires the `full_crypto`
/// feature and is unavailable in the no_std runtime build, so the benchmark
/// signs through the `sp_io::crypto` host functions instead.
const BENCH_KEY_TYPE: KeyTypeId = KeyTypeId(*b"l99m");

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit_relayed_transaction() {
		// 256-byte payload, 5 EUR trailing-u64 value (under the 10 EUR cap).
		let mut payload = alloc::vec![0u8; 248];
		payload.extend_from_slice(&5_000_000_000_000u64.to_le_bytes());
		let mandate_ref = [42u8; 32];
		let nonce = 1u64;

		// Mint the authorising keypair via the keystore host function —
		// `Pair::sign` is not available in the no_std runtime context.
		let pubkey = sp_io::crypto::ed25519_generate(BENCH_KEY_TYPE, None);

		// Compute the same content hash the extrinsic computes so the
		// signature verifies inside the benched call (otherwise we measure
		// the failure path).
		let zero_block: frame_system::pallet_prelude::BlockNumberFor<T> = 0u32.into();
		let chain_id = <frame_system::Pallet<T>>::block_hash(zero_block);
		let mut input = alloc::vec::Vec::with_capacity(32 + 8 + 32 + payload.len());
		input.extend_from_slice(&mandate_ref);
		input.extend_from_slice(&nonce.to_le_bytes());
		input.extend_from_slice(chain_id.as_ref());
		input.extend_from_slice(&payload);
		let content_hash = sp_io::hashing::blake2_256(&input);
		let sig = sp_io::crypto::ed25519_sign(BENCH_KEY_TYPE, &pubkey, &content_hash)
			.expect("the signing key was just generated in the keystore");

		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), payload, sig, pubkey, mandate_ref, nonce);

		assert!(RelayedHashIndex::<T>::contains_key(content_hash));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
