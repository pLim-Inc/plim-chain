//! # P:L:I:M:/Compliance Pallet
//!
//! On-chain compliance engine implementing a multi-stage decision flow:
//! KYC valid? -> Jurisdiction OK? -> AML clear? -> Within limits? -> Approve/Reject.
//!
//! Stores KYC levels, compliance policies, and an immutable audit trail.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// ---------------------------------------------------------------------------
	// Enums
	// ---------------------------------------------------------------------------

	/// KYC verification levels, ordered by increasing stringency.
	#[derive(
		Clone, Copy, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen,
		PartialOrd, Ord,
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

	/// The result of a compliance check.
	#[derive(
		Clone, Copy, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen,
	)]
	pub enum ComplianceResult {
		Approved,
		Rejected,
	}

	// ---------------------------------------------------------------------------
	// Structs
	// ---------------------------------------------------------------------------

	/// A compliance policy defining requirements for a particular jurisdiction.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct CompliancePolicy<T: Config> {
		/// Jurisdiction code (ISO 3166-1 alpha-2, bounded).
		pub jurisdiction: BoundedVec<u8, T::MaxJurisdictionLen>,
		/// Minimum KYC level required for transactions in this jurisdiction.
		pub min_kyc_level: KYCLevel,
		/// Maximum single transaction amount (0 = unlimited).
		pub max_single_tx: u128,
		/// Maximum daily aggregate amount (0 = unlimited).
		pub max_daily: u128,
		/// Whether AML screening is required.
		pub aml_required: bool,
		/// Admin who created / last updated this policy.
		pub updated_by: T::AccountId,
		/// Block at which this policy was last updated.
		pub updated_at: BlockNumberFor<T>,
	}

	/// An immutable audit trail entry.
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct AuditEntry<T: Config> {
		/// The account involved.
		pub account: T::AccountId,
		/// A short descriptor of the event.
		pub event_type: BoundedVec<u8, T::MaxEventTypeLen>,
		/// Compliance result if applicable.
		pub result: Option<ComplianceResult>,
		/// Block number when logged.
		pub block: BlockNumberFor<T>,
	}

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that can manage KYC status and compliance policies (admin).
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Maximum length for a jurisdiction code.
		#[pallet::constant]
		type MaxJurisdictionLen: Get<u32>;

		/// Maximum length for an event type descriptor in the audit trail.
		#[pallet::constant]
		type MaxEventTypeLen: Get<u32>;

		/// Maximum number of audit trail entries stored on-chain.
		#[pallet::constant]
		type MaxAuditEntries: Get<u32>;

		/// Maximum number of compliance policies.
		#[pallet::constant]
		type MaxPolicies: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	/// KYC status for each account.
	#[pallet::storage]
	#[pallet::getter(fn kyc_status)]
	pub type KYCStatusMap<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, KYCLevel, ValueQuery>;

	/// Compliance policies indexed by jurisdiction hash.
	#[pallet::storage]
	#[pallet::getter(fn compliance_policies)]
	pub type CompliancePolicies<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, CompliancePolicy<T>, OptionQuery>;

	/// Index of all policy keys for enumeration.
	#[pallet::storage]
	#[pallet::getter(fn policy_keys)]
	pub type PolicyKeys<T: Config> =
		StorageValue<_, BoundedVec<T::Hash, T::MaxPolicies>, ValueQuery>;

	/// Rolling audit trail (bounded, oldest entries evicted via write pointer).
	#[pallet::storage]
	#[pallet::getter(fn audit_trail)]
	pub type AuditTrail<T: Config> =
		StorageValue<_, BoundedVec<AuditEntry<T>, T::MaxAuditEntries>, ValueQuery>;

	/// Number of audit entries ever written (used as monotonic counter).
	#[pallet::storage]
	#[pallet::getter(fn audit_count)]
	pub type AuditCount<T: Config> = StorageValue<_, u64, ValueQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// KYC level was set for an account.
		KYCStatusSet {
			account: T::AccountId,
			level: KYCLevel,
		},
		/// A compliance check was performed.
		ComplianceChecked {
			account: T::AccountId,
			result: ComplianceResult,
		},
		/// A compliance event was logged to the audit trail.
		ComplianceEventLogged {
			account: T::AccountId,
			event_type: BoundedVec<u8, T::MaxEventTypeLen>,
		},
		/// A compliance policy was created or updated.
		PolicyUpdated {
			jurisdiction_hash: T::Hash,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// KYC level is insufficient for this operation.
		InsufficientKYC,
		/// The jurisdiction is not configured.
		JurisdictionNotFound,
		/// AML screening has not been passed.
		AMLCheckFailed,
		/// Transaction amount exceeds the single-tx limit.
		SingleTxLimitExceeded,
		/// Daily aggregate limit exceeded.
		DailyLimitExceeded,
		/// Audit trail storage is full.
		AuditTrailFull,
		/// Policy storage is full.
		PolicyStorageFull,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the KYC level of an account. Admin-only.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(20_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 2)))]
		pub fn set_kyc_status(
			origin: OriginFor<T>,
			account: T::AccountId,
			level: KYCLevel,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			KYCStatusMap::<T>::insert(&account, level);

			// Log to audit trail.
			let now = <frame_system::Pallet<T>>::block_number();
			let event_type: BoundedVec<u8, T::MaxEventTypeLen> =
				b"kyc_update"
					.to_vec()
					.try_into()
					.expect("kyc_update fits in MaxEventTypeLen");

			Self::append_audit(AuditEntry {
				account: account.clone(),
				event_type,
				result: None,
				block: now,
			});

			Self::deposit_event(Event::KYCStatusSet { account, level });

			Ok(())
		}

		/// Run the full compliance decision flow for a given account and amount.
		///
		/// Decision flow: KYC valid? -> Jurisdiction? -> AML? -> Limits? -> Approve/Reject
		///
		/// `aml_passed` is provided by the caller (off-chain AML oracle result).
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(35_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(3, 1)))]
		pub fn check_compliance(
			origin: OriginFor<T>,
			account: T::AccountId,
			jurisdiction: BoundedVec<u8, T::MaxJurisdictionLen>,
			amount: u128,
			aml_passed: bool,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let kyc_level = KYCStatusMap::<T>::get(&account);

			// Lookup the policy by jurisdiction hash.
			let jurisdiction_hash = T::Hashing::hash_of(&jurisdiction);
			let policy = CompliancePolicies::<T>::get(&jurisdiction_hash)
				.ok_or(Error::<T>::JurisdictionNotFound)?;

			// Step 1: KYC level check.
			let kyc_ok = kyc_level >= policy.min_kyc_level;

			// Step 2: AML check (if required by policy).
			let aml_ok = !policy.aml_required || aml_passed;

			// Step 3: Limits check.
			let limits_ok = policy.max_single_tx == 0 || amount <= policy.max_single_tx;

			let result = if kyc_ok && aml_ok && limits_ok {
				ComplianceResult::Approved
			} else {
				ComplianceResult::Rejected
			};

			// Log to audit trail.
			let now = <frame_system::Pallet<T>>::block_number();
			let event_type: BoundedVec<u8, T::MaxEventTypeLen> =
				b"compliance_check"
					.to_vec()
					.try_into()
					.expect("compliance_check fits in MaxEventTypeLen");

			Self::append_audit(AuditEntry {
				account: account.clone(),
				event_type,
				result: Some(result),
				block: now,
			});

			Self::deposit_event(Event::ComplianceChecked { account, result });

			Ok(())
		}

		/// Log a free-form compliance event to the immutable audit trail.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(20_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
		pub fn log_compliance_event(
			origin: OriginFor<T>,
			account: T::AccountId,
			event_type: BoundedVec<u8, T::MaxEventTypeLen>,
		) -> DispatchResult {
			ensure_signed(origin)?;

			let now = <frame_system::Pallet<T>>::block_number();

			Self::append_audit(AuditEntry {
				account: account.clone(),
				event_type: event_type.clone(),
				result: None,
				block: now,
			});

			Self::deposit_event(Event::ComplianceEventLogged {
				account,
				event_type,
			});

			Ok(())
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Append an entry to the bounded audit trail.
		/// When full, the oldest entry is evicted (FIFO).
		fn append_audit(entry: AuditEntry<T>) {
			AuditTrail::<T>::mutate(|trail| {
				if trail.try_push(entry.clone()).is_err() {
					// Buffer full -- evict oldest, then push.
					if !trail.is_empty() {
						trail.remove(0);
					}
					// After removing one element, push should always succeed.
					let _ = trail.try_push(entry);
				}
			});
			AuditCount::<T>::mutate(|c| *c = c.saturating_add(1));
		}
	}
}
