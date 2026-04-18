//! Mock runtime for `pallet-rwa` tests.
//!
//! Wires `frame_system`, `pallet_balances`, and the pallet under test.
//! Provides `MockKyc`: a per-account, mutable KYC oracle controlled from
//! tests via thread-local maps (KYC level, sanctions, expiry override).

#![cfg(test)]

use crate::{self as pallet_rwa, KycLevel, KycProvider};
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64},
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, DispatchError,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

pub type AccountId = u64;
pub type Balance = u128;
pub type Block = frame_system::mocking::MockBlock<Test>;
pub type BlockNumber = u64;

pub const ALICE: AccountId = 1; // manager
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;
pub const DAVE: AccountId = 4;
pub const EVE: AccountId = 5; // unKYCed

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Rwa: pallet_rwa,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Hash = H256;
	type Hashing = BlakeTwo256;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = ConstU32<0>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

// ---------------------------------------------------------------------------
// MockKyc — granular per-test KYC oracle.
// ---------------------------------------------------------------------------

thread_local! {
	pub static KYC_LEVELS: RefCell<BTreeMap<AccountId, KycLevel>> = RefCell::new(BTreeMap::new());
	pub static KYC_SANCTIONED: RefCell<BTreeSet<AccountId>> = RefCell::new(BTreeSet::new());
	pub static KYC_EXPIRED: RefCell<BTreeSet<AccountId>> = RefCell::new(BTreeSet::new());
}

pub struct MockKyc;

impl MockKyc {
	pub fn set_level(account: AccountId, level: KycLevel) {
		KYC_LEVELS.with(|m| {
			m.borrow_mut().insert(account, level);
		});
	}

	pub fn sanction(account: AccountId) {
		KYC_SANCTIONED.with(|s| {
			s.borrow_mut().insert(account);
		});
	}

	pub fn mark_expired(account: AccountId) {
		KYC_EXPIRED.with(|s| {
			s.borrow_mut().insert(account);
		});
	}

	pub fn reset() {
		KYC_LEVELS.with(|m| m.borrow_mut().clear());
		KYC_SANCTIONED.with(|s| s.borrow_mut().clear());
		KYC_EXPIRED.with(|s| s.borrow_mut().clear());
	}
}

impl KycProvider<AccountId, BlockNumber> for MockKyc {
	fn level_of(account: &AccountId) -> KycLevel {
		KYC_LEVELS.with(|m| m.borrow().get(account).copied().unwrap_or(KycLevel::None))
	}

	fn is_sanctioned(account: &AccountId) -> bool {
		KYC_SANCTIONED.with(|s| s.borrow().contains(account))
	}

	fn is_expired(account: &AccountId, _now: BlockNumber) -> bool {
		KYC_EXPIRED.with(|s| s.borrow().contains(account))
	}

	fn require_at_least(
		account: &AccountId,
		required: KycLevel,
		now: BlockNumber,
	) -> Result<(), DispatchError> {
		if Self::is_sanctioned(account) {
			return Err(DispatchError::Other("kyc: sanctioned"));
		}
		if Self::is_expired(account, now) {
			return Err(DispatchError::Other("kyc: expired"));
		}
		if Self::level_of(account) < required {
			return Err(DispatchError::Other("kyc: level too low"));
		}
		Ok(())
	}
}

// ---------------------------------------------------------------------------
// Pallet under test.
// ---------------------------------------------------------------------------

impl pallet_rwa::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RwaAssetId = u32;
	type DistributionId = u64;
	type Kyc = MockKyc;
	type MaxDistributionsPerClaim = ConstU32<50>;
	type MaxShareholdersPerDistribution = ConstU32<10_000>;
	type WeightInfo = ();
}

#[allow(dead_code)]
const _BLOCK_NUMBER_HINT: ConstU64<0> = ConstU64::<0>;

pub fn new_test_ext() -> sp_io::TestExternalities {
	MockKyc::reset();
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, 1_000_000_000),
			(BOB, 1_000_000),
			(CHARLIE, 1_000_000),
			(DAVE, 1_000_000),
			(EVE, 1_000_000),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		// Default: ALICE/BOB/CHARLIE/DAVE = Institutional; EVE intentionally unKYCed.
		MockKyc::set_level(ALICE, KycLevel::Institutional);
		MockKyc::set_level(BOB, KycLevel::Institutional);
		MockKyc::set_level(CHARLIE, KycLevel::Institutional);
		MockKyc::set_level(DAVE, KycLevel::Institutional);
	});
	ext
}
