//! Mock runtime for pallet-plim-kyc tests.
//!
//! Wires `frame_system`, `pallet_balances`, and `pallet_plim_kyc` into a
//! minimal runtime so we can drive real extrinsics in unit tests.

#![cfg(test)]

use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

pub type AccountId = u64;
pub type Balance = u128;
pub type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		PlimKyc: crate,
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
	pub const MaxAttestors: u32 = 10;
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

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxAttestors = MaxAttestors;
	type WeightInfo = ();
}

pub const ATTESTOR1: AccountId = 100;
pub const ATTESTOR2: AccountId = 101;
pub const NON_ATTESTOR: AccountId = 200;
pub const SUBJECT_A: AccountId = 1;
pub const SUBJECT_B: AccountId = 2;

/// Build test externalities with `ATTESTOR1` and `ATTESTOR2` pre-registered
/// in the attestor set via genesis.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("frame_system genesis builds");

	crate::GenesisConfig::<Test> {
		initial_attestors: vec![ATTESTOR1, ATTESTOR2],
	}
	.assimilate_storage(&mut t)
	.expect("plim-kyc genesis assimilates");

	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
