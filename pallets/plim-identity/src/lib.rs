#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use alloc::vec::Vec;
	use frame_support::pallet_prelude::*;
	use frame_support::sp_runtime::traits::Hash;
	use frame_system::pallet_prelude::*;

	/// Agent status within the identity system.
	#[derive(
		Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, RuntimeDebug, TypeInfo,
		MaxEncodedLen,
	)]
	pub enum AgentStatus {
		Active,
		Paused,
		Revoked,
	}

	impl Default for AgentStatus {
		fn default() -> Self {
			AgentStatus::Active
		}
	}

	/// On-chain profile for a registered AI agent.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AgentProfile<T: Config> {
		pub owner: T::AccountId,
		pub did: BoundedVec<u8, T::MaxDIDLength>,
		pub name: BoundedVec<u8, T::MaxNameLength>,
		pub model_type: BoundedVec<u8, T::MaxModelTypeLength>,
		pub capability_commitment: T::Hash,
		pub reputation_score: u32,
		pub status: AgentStatus,
		pub created_at: BlockNumberFor<T>,
	}

	/// A DID document stored on-chain.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct DIDDocument<T: Config> {
		pub controller: T::AccountId,
		pub created_at: BlockNumberFor<T>,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum length for a DID string.
		#[pallet::constant]
		type MaxDIDLength: Get<u32>;

		/// Maximum length for an agent name.
		#[pallet::constant]
		type MaxNameLength: Get<u32>;

		/// Maximum length for a model type descriptor.
		#[pallet::constant]
		type MaxModelTypeLength: Get<u32>;

		/// Maximum number of credential commitments per agent.
		#[pallet::constant]
		type MaxCredentials: Get<u32>;

		/// Maximum byte length for a ZK proof payload.
		#[pallet::constant]
		type MaxProofSize: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// Maps an AccountId to its AgentProfile.
	#[pallet::storage]
	#[pallet::getter(fn agent_registry)]
	pub type AgentRegistry<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, AgentProfile<T>, OptionQuery>;

	/// Maps an AccountId to a bounded vector of credential commitment hashes.
	#[pallet::storage]
	#[pallet::getter(fn credential_commitments)]
	pub type CredentialCommitments<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<T::Hash, T::MaxCredentials>,
		ValueQuery,
	>;

	/// Maps a DID (bounded byte vec) to its DIDDocument.
	#[pallet::storage]
	#[pallet::getter(fn did_documents)]
	pub type DIDDocuments<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		BoundedVec<u8, T::MaxDIDLength>,
		DIDDocument<T>,
		OptionQuery,
	>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An agent was registered. \[who, did\]
		AgentRegistered {
			who: T::AccountId,
			did: BoundedVec<u8, T::MaxDIDLength>,
		},
		/// A credential commitment was issued. \[issuer, agent_id, commitment\]
		CredentialIssued {
			issuer: T::AccountId,
			agent_id: T::AccountId,
			commitment: T::Hash,
		},
		/// A ZK proof was submitted and (placeholder) verified. \[verifier, agent_id\]
		ZKProofVerified {
			verifier: T::AccountId,
			agent_id: T::AccountId,
		},
		/// An agent's status was changed. \[who, new_status\]
		AgentStatusChanged {
			who: T::AccountId,
			new_status: AgentStatus,
		},
		/// A credential was revoked. \[who, agent_id, credential_index\]
		CredentialRevoked {
			who: T::AccountId,
			agent_id: T::AccountId,
			credential_index: u32,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// The caller already has a registered agent profile.
		AgentAlreadyRegistered,
		/// No agent profile found for the given account.
		AgentNotFound,
		/// The caller is not the owner of the specified agent.
		NotAgentOwner,
		/// The credential index is out of bounds or otherwise invalid.
		InvalidCredential,
		/// The agent has been revoked and cannot be acted upon.
		AgentRevoked,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new AI agent identity on-chain.
		///
		/// Generates a deterministic DID from the caller's account, stores the
		/// `AgentProfile`, and creates a corresponding `DIDDocument`.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 3))]
		pub fn register_agent(
			origin: OriginFor<T>,
			name: BoundedVec<u8, T::MaxNameLength>,
			model_type: BoundedVec<u8, T::MaxModelTypeLength>,
			capabilities: BoundedVec<u8, T::MaxProofSize>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				!AgentRegistry::<T>::contains_key(&who),
				Error::<T>::AgentAlreadyRegistered
			);

			// Build a deterministic DID: "did:plim:<hex(account)>"
			let prefix = b"did:plim:";
			let account_encoded = who.encode();
			let account_hash = T::Hashing::hash(&account_encoded);
			let hash_bytes = account_hash.encode();

			let mut did_raw: Vec<u8> = prefix.to_vec();
			for byte in hash_bytes.iter().take(16) {
				// Hex-encode each byte (lowercase)
				let hi = byte >> 4;
				let lo = byte & 0x0f;
				did_raw.push(Self::nibble_to_hex(hi));
				did_raw.push(Self::nibble_to_hex(lo));
			}
			let did: BoundedVec<u8, T::MaxDIDLength> =
				BoundedVec::try_from(did_raw).map_err(|_| Error::<T>::InvalidCredential)?;

			// Compute capability commitment hash
			let capability_commitment = T::Hashing::hash(&capabilities);

			let now = <frame_system::Pallet<T>>::block_number();

			let profile = AgentProfile::<T> {
				owner: who.clone(),
				did: did.clone(),
				name,
				model_type,
				capability_commitment,
				reputation_score: 0,
				status: AgentStatus::Active,
				created_at: now,
			};

			let did_doc = DIDDocument::<T> {
				controller: who.clone(),
				created_at: now,
			};

			AgentRegistry::<T>::insert(&who, profile);
			DIDDocuments::<T>::insert(&did, did_doc);

			Self::deposit_event(Event::AgentRegistered { who, did });

			Ok(())
		}

		/// Issue a credential commitment to a registered agent.
		///
		/// The caller (issuer) attaches a hashed credential to the target
		/// agent's on-chain record.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn issue_credential(
			origin: OriginFor<T>,
			agent_id: T::AccountId,
			_credential_type: BoundedVec<u8, T::MaxNameLength>,
			commitment: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let agent = AgentRegistry::<T>::get(&agent_id).ok_or(Error::<T>::AgentNotFound)?;
			ensure!(agent.status != AgentStatus::Revoked, Error::<T>::AgentRevoked);

			CredentialCommitments::<T>::try_mutate(&agent_id, |creds| {
				creds
					.try_push(commitment)
					.map_err(|_| Error::<T>::InvalidCredential)
			})?;

			Self::deposit_event(Event::CredentialIssued {
				issuer: who,
				agent_id,
				commitment,
			});

			Ok(())
		}

		/// Submit a ZK proof for placeholder verification.
		///
		/// In this initial version the proof is not cryptographically verified;
		/// the call simply emits a `ZKProofVerified` event when invoked.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads(1))]
		pub fn verify_zk_proof(
			origin: OriginFor<T>,
			agent_id: T::AccountId,
			_proof_bytes: BoundedVec<u8, T::MaxProofSize>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let agent = AgentRegistry::<T>::get(&agent_id).ok_or(Error::<T>::AgentNotFound)?;
			ensure!(agent.status != AgentStatus::Revoked, Error::<T>::AgentRevoked);

			// Placeholder: real ZK verification goes here.
			Self::deposit_event(Event::ZKProofVerified {
				verifier: who,
				agent_id,
			});

			Ok(())
		}

		/// Revoke a credential from a registered agent by index.
		///
		/// Only the agent owner may revoke credentials.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn revoke_credential(
			origin: OriginFor<T>,
			agent_id: T::AccountId,
			credential_index: u32,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let agent = AgentRegistry::<T>::get(&agent_id).ok_or(Error::<T>::AgentNotFound)?;
			ensure!(agent.owner == who, Error::<T>::NotAgentOwner);

			CredentialCommitments::<T>::try_mutate(&agent_id, |creds| -> DispatchResult {
				let idx = credential_index as usize;
				ensure!(idx < creds.len(), Error::<T>::InvalidCredential);
				creds.swap_remove(idx);
				Ok(())
			})?;

			Self::deposit_event(Event::CredentialRevoked {
				who,
				agent_id,
				credential_index,
			});

			Ok(())
		}

		/// Update the status of a registered agent (owner only).
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn update_agent_status(
			origin: OriginFor<T>,
			agent_id: T::AccountId,
			new_status: AgentStatus,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			AgentRegistry::<T>::try_mutate(&agent_id, |maybe_agent| -> DispatchResult {
				let agent = maybe_agent.as_mut().ok_or(Error::<T>::AgentNotFound)?;
				ensure!(agent.owner == who, Error::<T>::NotAgentOwner);
				agent.status = new_status.clone();
				Ok(())
			})?;

			Self::deposit_event(Event::AgentStatusChanged {
				who: agent_id,
				new_status,
			});

			Ok(())
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Convert a 4-bit nibble to its ASCII hex character.
		fn nibble_to_hex(n: u8) -> u8 {
			match n {
				0..=9 => b'0' + n,
				_ => b'a' + (n - 10),
			}
		}
	}
}
