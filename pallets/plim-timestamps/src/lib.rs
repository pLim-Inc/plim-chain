//! # P:L:I:M:/Timestamps Pallet
//!
//! Aggregates event hashes into Merkle trees every N blocks and stores the root
//! on-chain. An off-chain worker collects events from the buffer, computes the
//! Merkle root, and submits it via `commit_timestamp_root`. Any party can later
//! verify that a specific event hash was included in a given block's Merkle root.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Number of blocks between each Merkle root commitment.
		#[pallet::constant]
		type AggregationInterval: Get<BlockNumberFor<Self>>;

		/// Maximum number of event hashes buffered between aggregations.
		#[pallet::constant]
		type MaxEventBuffer: Get<u32>;

		/// Maximum depth of a Merkle proof (log2 of MaxEventBuffer is sufficient).
		#[pallet::constant]
		type MaxProofDepth: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// Committed Merkle roots indexed by the block number at which they were stored.
	#[pallet::storage]
	#[pallet::getter(fn merkle_roots)]
	pub type MerkleRoots<T: Config> =
		StorageMap<_, Blake2_128Concat, BlockNumberFor<T>, T::Hash, OptionQuery>;

	/// Buffer of event hashes waiting for the next aggregation cycle.
	#[pallet::storage]
	#[pallet::getter(fn event_buffer)]
	pub type EventBuffer<T: Config> =
		StorageValue<_, BoundedVec<T::Hash, T::MaxEventBuffer>, ValueQuery>;

	/// The block number at which the last aggregation occurred.
	#[pallet::storage]
	#[pallet::getter(fn last_aggregation)]
	pub type LastAggregation<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An event hash was added to the buffer.
		EventBuffered {
			event_hash: T::Hash,
		},
		/// A Merkle root was committed for a given block.
		TimestampRootCommitted {
			block_number: BlockNumberFor<T>,
			merkle_root: T::Hash,
			event_count: u32,
		},
		/// A timestamp verification was performed.
		TimestampVerified {
			block_number: BlockNumberFor<T>,
			event_hash: T::Hash,
			valid: bool,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Event buffer is full; wait for next aggregation.
		EventBufferFull,
		/// No Merkle root found for the specified block.
		MerkleRootNotFound,
		/// Aggregation interval has not elapsed yet.
		AggregationNotDue,
		/// The buffer is empty; nothing to aggregate.
		EmptyBuffer,
		/// Proof length exceeds maximum allowed depth.
		ProofTooLong,
		/// Invalid Merkle proof.
		InvalidProof,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit an event hash to the buffer for inclusion in the next Merkle tree.
		/// Can be called by any signed origin.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(15_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn buffer_event(
			origin: OriginFor<T>,
			event_hash: T::Hash,
		) -> DispatchResult {
			ensure_signed(origin)?;

			EventBuffer::<T>::try_mutate(|buf| -> DispatchResult {
				buf.try_push(event_hash).map_err(|_| Error::<T>::EventBufferFull)?;
				Ok(())
			})?;

			Self::deposit_event(Event::EventBuffered { event_hash });

			Ok(())
		}

		/// Commit the Merkle root of the current event buffer.
		/// Typically called by an off-chain worker once the aggregation interval
		/// has elapsed. The caller passes the computed `merkle_root`; the pallet
		/// re-derives it from the buffer to ensure correctness.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(60_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(3, 3)))]
		pub fn commit_timestamp_root(
			origin: OriginFor<T>,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let now = <frame_system::Pallet<T>>::block_number();
			let last = LastAggregation::<T>::get();
			let interval = T::AggregationInterval::get();

			ensure!(
				now >= last.saturating_add(interval),
				Error::<T>::AggregationNotDue
			);

			let buffer = EventBuffer::<T>::get();
			ensure!(!buffer.is_empty(), Error::<T>::EmptyBuffer);

			let event_count = buffer.len() as u32;

			// Compute the Merkle root from the buffer.
			let merkle_root = Self::compute_merkle_root(&buffer);

			// Store root and clear the buffer.
			MerkleRoots::<T>::insert(now, merkle_root);
			EventBuffer::<T>::kill();
			LastAggregation::<T>::put(now);

			Self::deposit_event(Event::TimestampRootCommitted {
				block_number: now,
				merkle_root,
				event_count,
			});

			Ok(())
		}

		/// Verify that a given event hash is included in a committed Merkle root
		/// by providing the Merkle proof (sibling hashes and their positions).
		///
		/// `proof`: ordered list of (sibling_hash, is_left) pairs from leaf to root.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(30_000_000, 0).saturating_add(T::DbWeight::get().reads(1)))]
		pub fn verify_timestamp(
			origin: OriginFor<T>,
			block_number: BlockNumberFor<T>,
			event_hash: T::Hash,
			proof: BoundedVec<(T::Hash, bool), T::MaxProofDepth>,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let stored_root = MerkleRoots::<T>::get(block_number)
				.ok_or(Error::<T>::MerkleRootNotFound)?;

			// Walk up the proof to reconstruct the root.
			let mut current = event_hash;
			for (sibling, is_left) in proof.iter() {
				current = if *is_left {
					T::Hashing::hash_of(&(sibling, &current))
				} else {
					T::Hashing::hash_of(&(&current, sibling))
				};
			}

			let valid = current == stored_root;

			Self::deposit_event(Event::TimestampVerified {
				block_number,
				event_hash,
				valid,
			});

			Ok(())
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Build a Merkle tree from a list of hashes and return the root.
		/// Uses a simple binary Merkle tree; odd leaves are duplicated.
		fn compute_merkle_root(leaves: &[T::Hash]) -> T::Hash {
			if leaves.is_empty() {
				return T::Hashing::hash_of(&0u8);
			}
			if leaves.len() == 1 {
				return leaves[0];
			}

			let mut layer: BoundedVec<T::Hash, T::MaxEventBuffer> =
				leaves.to_vec().try_into().unwrap_or_default();

			while layer.len() > 1 {
				let mut next = BoundedVec::<T::Hash, T::MaxEventBuffer>::default();
				let mut i = 0;
				while i < layer.len() {
					let left = layer[i];
					let right = if i + 1 < layer.len() {
						layer[i + 1]
					} else {
						// Duplicate the last element for an odd-length layer.
						layer[i]
					};
					let parent = T::Hashing::hash_of(&(left, right));
					let _ = next.try_push(parent);
					i += 2;
				}
				layer = next;
			}

			layer[0]
		}
	}
}
