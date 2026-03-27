#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, ReservableCurrency},
	};
	use frame_system::pallet_prelude::*;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	// ---------------------------------------------------------------------------
	// Enums
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum FlowType {
		P2AI,
		B2AI,
		G2AI,
		AI2AI,
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum ComplianceLevel {
		Basic,
		Business,
		Government,
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	pub enum PaymentStatus {
		Pending,
		Authorized,
		Executing,
		Settled,
		Failed,
		Refunded,
	}

	// ---------------------------------------------------------------------------
	// Structs
	// ---------------------------------------------------------------------------

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct PaymentIntent<T: Config> {
		pub id: T::Hash,
		pub flow_type: FlowType,
		pub sender: T::AccountId,
		pub receiver: T::AccountId,
		pub amount: BalanceOf<T>,
		pub service_id: BoundedVec<u8, T::MaxServiceIdLength>,
		pub mandate_ref: Option<T::Hash>,
		pub channel_id: Option<T::Hash>,
		pub compliance_level: ComplianceLevel,
		pub status: PaymentStatus,
		pub created_at: BlockNumberFor<T>,
	}

	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct EscrowInfo<T: Config> {
		pub id: T::Hash,
		pub depositor: T::AccountId,
		pub beneficiary: T::AccountId,
		pub amount: BalanceOf<T>,
		pub conditions_hash: T::Hash,
		pub released: bool,
		pub created_at: BlockNumberFor<T>,
		pub expires_at: BlockNumberFor<T>,
	}

	// ---------------------------------------------------------------------------
	// Config
	// ---------------------------------------------------------------------------

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency mechanism.
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

		/// Maximum length for a service ID.
		#[pallet::constant]
		type MaxServiceIdLength: Get<u32>;
	}

	// ---------------------------------------------------------------------------
	// Pallet
	// ---------------------------------------------------------------------------

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ---------------------------------------------------------------------------
	// Storage
	// ---------------------------------------------------------------------------

	#[pallet::storage]
	#[pallet::getter(fn payment_intents)]
	pub type PaymentIntents<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, PaymentIntent<T>, OptionQuery>;

	#[pallet::storage]
	pub type Payments<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, PaymentIntent<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn escrow_accounts)]
	pub type EscrowAccounts<T: Config> =
		StorageMap<_, Blake2_128Concat, T::Hash, EscrowInfo<T>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn payment_nonce)]
	pub type PaymentNonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	// ---------------------------------------------------------------------------
	// Events
	// ---------------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A payment intent was created.
		IntentCreated {
			id: T::Hash,
			sender: T::AccountId,
			receiver: T::AccountId,
			amount: BalanceOf<T>,
			flow_type: FlowType,
		},
		/// A payment was executed successfully.
		PaymentExecuted {
			id: T::Hash,
			sender: T::AccountId,
			receiver: T::AccountId,
			amount: BalanceOf<T>,
		},
		/// A payment execution failed.
		PaymentFailed {
			id: T::Hash,
			reason: BoundedVec<u8, ConstU32<128>>,
		},
		/// An escrow was created.
		EscrowCreated {
			id: T::Hash,
			depositor: T::AccountId,
			beneficiary: T::AccountId,
			amount: BalanceOf<T>,
			expires_at: BlockNumberFor<T>,
		},
		/// An escrow was released to the beneficiary.
		EscrowReleased {
			id: T::Hash,
			beneficiary: T::AccountId,
			amount: BalanceOf<T>,
		},
		/// An escrow was refunded to the depositor.
		EscrowRefunded {
			id: T::Hash,
			depositor: T::AccountId,
			amount: BalanceOf<T>,
		},
	}

	// ---------------------------------------------------------------------------
	// Errors
	// ---------------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// The payment intent was not found.
		IntentNotFound,
		/// The payment is not in a valid status for this operation.
		InvalidPaymentStatus,
		/// The escrow was not found.
		EscrowNotFound,
		/// The escrow has already been released.
		EscrowAlreadyReleased,
		/// The caller is not authorized to perform this action.
		NotAuthorized,
		/// Amount must be greater than zero.
		ZeroAmount,
		/// Nonce overflow.
		NonceOverflow,
		/// Duration must be greater than zero.
		ZeroDuration,
		/// The escrow has expired.
		EscrowExpired,
		/// The escrow has not yet expired (cannot refund).
		EscrowNotExpired,
		/// Insufficient balance.
		InsufficientBalance,
		/// Service ID is empty.
		EmptyServiceId,
	}

	// ---------------------------------------------------------------------------
	// Extrinsics
	// ---------------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new payment intent.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 2))]
		pub fn create_intent(
			origin: OriginFor<T>,
			to: T::AccountId,
			amount: BalanceOf<T>,
			service_id: BoundedVec<u8, T::MaxServiceIdLength>,
			flow_type: FlowType,
			mandate_ref: Option<T::Hash>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(amount > BalanceOf::<T>::default(), Error::<T>::ZeroAmount);
			ensure!(!service_id.is_empty(), Error::<T>::EmptyServiceId);

			let nonce = PaymentNonce::<T>::get();
			let new_nonce = nonce.checked_add(1).ok_or(Error::<T>::NonceOverflow)?;

			let id = Self::generate_id(&sender, nonce);
			let now = <frame_system::Pallet<T>>::block_number();

			let intent = PaymentIntent::<T> {
				id,
				flow_type: flow_type.clone(),
				sender: sender.clone(),
				receiver: to.clone(),
				amount,
				service_id,
				mandate_ref,
				channel_id: None,
				compliance_level: ComplianceLevel::Basic,
				status: PaymentStatus::Pending,
				created_at: now,
			};

			PaymentIntents::<T>::insert(id, intent);
			PaymentNonce::<T>::put(new_nonce);

			Self::deposit_event(Event::IntentCreated {
				id,
				sender,
				receiver: to,
				amount,
				flow_type,
			});

			Ok(())
		}

		/// Execute a pending payment intent — transfers funds from sender to receiver.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(20_000, 0) + T::DbWeight::get().reads_writes(1, 2))]
		pub fn execute_payment(
			origin: OriginFor<T>,
			payment_id: T::Hash,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			PaymentIntents::<T>::try_mutate(payment_id, |maybe_intent| -> DispatchResult {
				let intent = maybe_intent.as_mut().ok_or(Error::<T>::IntentNotFound)?;
				ensure!(caller == intent.sender, Error::<T>::NotAuthorized);
				ensure!(
					intent.status == PaymentStatus::Pending
						|| intent.status == PaymentStatus::Authorized,
					Error::<T>::InvalidPaymentStatus
				);

				intent.status = PaymentStatus::Executing;

				let transfer_result = T::Currency::transfer(
					&intent.sender,
					&intent.receiver,
					intent.amount,
					ExistenceRequirement::KeepAlive,
				);

				match transfer_result {
					Ok(_) => {
						intent.status = PaymentStatus::Settled;

						// Copy settled intent into Payments storage.
						Payments::<T>::insert(payment_id, intent.clone());

						Self::deposit_event(Event::PaymentExecuted {
							id: payment_id,
							sender: intent.sender.clone(),
							receiver: intent.receiver.clone(),
							amount: intent.amount,
						});
					},
					Err(_) => {
						intent.status = PaymentStatus::Failed;

						let reason: BoundedVec<u8, ConstU32<128>> =
							BoundedVec::try_from(b"transfer_failed".to_vec())
								.expect("reason fits in 128 bytes");

						Self::deposit_event(Event::PaymentFailed {
							id: payment_id,
							reason,
						});
					},
				}

				Ok(())
			})
		}

		/// Create an escrow: locks the depositor's funds until conditions are met.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(15_000, 0) + T::DbWeight::get().reads_writes(1, 2))]
		pub fn create_escrow(
			origin: OriginFor<T>,
			beneficiary: T::AccountId,
			amount: BalanceOf<T>,
			conditions_hash: T::Hash,
			duration_blocks: BlockNumberFor<T>,
		) -> DispatchResult {
			let depositor = ensure_signed(origin)?;
			ensure!(amount > BalanceOf::<T>::default(), Error::<T>::ZeroAmount);
			ensure!(
				duration_blocks > BlockNumberFor::<T>::default(),
				Error::<T>::ZeroDuration
			);

			// Reserve (lock) the funds.
			T::Currency::reserve(&depositor, amount)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			let nonce = PaymentNonce::<T>::get();
			let new_nonce = nonce.checked_add(1).ok_or(Error::<T>::NonceOverflow)?;
			let id = Self::generate_id(&depositor, nonce);
			let now = <frame_system::Pallet<T>>::block_number();
			let expires_at = now + duration_blocks;

			let escrow = EscrowInfo::<T> {
				id,
				depositor: depositor.clone(),
				beneficiary: beneficiary.clone(),
				amount,
				conditions_hash,
				released: false,
				created_at: now,
				expires_at,
			};

			EscrowAccounts::<T>::insert(id, escrow);
			PaymentNonce::<T>::put(new_nonce);

			Self::deposit_event(Event::EscrowCreated {
				id,
				depositor,
				beneficiary,
				amount,
				expires_at,
			});

			Ok(())
		}

		/// Release escrow funds to the beneficiary. Only the depositor can release.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(15_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn release_escrow(
			origin: OriginFor<T>,
			escrow_id: T::Hash,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			EscrowAccounts::<T>::try_mutate(escrow_id, |maybe_escrow| -> DispatchResult {
				let escrow = maybe_escrow.as_mut().ok_or(Error::<T>::EscrowNotFound)?;
				ensure!(caller == escrow.depositor, Error::<T>::NotAuthorized);
				ensure!(!escrow.released, Error::<T>::EscrowAlreadyReleased);

				let now = <frame_system::Pallet<T>>::block_number();
				ensure!(now <= escrow.expires_at, Error::<T>::EscrowExpired);

				// Unreserve from depositor and transfer to beneficiary.
				T::Currency::unreserve(&escrow.depositor, escrow.amount);
				T::Currency::transfer(
					&escrow.depositor,
					&escrow.beneficiary,
					escrow.amount,
					ExistenceRequirement::AllowDeath,
				)?;

				escrow.released = true;

				Self::deposit_event(Event::EscrowReleased {
					id: escrow_id,
					beneficiary: escrow.beneficiary.clone(),
					amount: escrow.amount,
				});

				Ok(())
			})
		}

		/// Refund a payment. The payment must not yet be settled.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(15_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn refund_payment(
			origin: OriginFor<T>,
			payment_id: T::Hash,
			_reason: BoundedVec<u8, ConstU32<128>>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			PaymentIntents::<T>::try_mutate(payment_id, |maybe_intent| -> DispatchResult {
				let intent = maybe_intent.as_mut().ok_or(Error::<T>::IntentNotFound)?;
				ensure!(
					caller == intent.sender || caller == intent.receiver,
					Error::<T>::NotAuthorized
				);
				ensure!(
					intent.status != PaymentStatus::Settled
						&& intent.status != PaymentStatus::Refunded,
					Error::<T>::InvalidPaymentStatus
				);

				intent.status = PaymentStatus::Refunded;

				Self::deposit_event(Event::EscrowRefunded {
					id: payment_id,
					depositor: intent.sender.clone(),
					amount: intent.amount,
				});

				Ok(())
			})
		}
	}

	// ---------------------------------------------------------------------------
	// Internal helpers
	// ---------------------------------------------------------------------------

	impl<T: Config> Pallet<T> {
		/// Deterministic ID from account + nonce.
		fn generate_id(who: &T::AccountId, nonce: u64) -> T::Hash {
			let payload = (who, nonce);
			T::Hashing::hash_of(&payload)
		}
	}
}
