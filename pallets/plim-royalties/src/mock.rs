//! Mock runtime for pallet-plim-royalties tests.
//!
//! Wires `frame_system`, `pallet_balances`, and `pallet_plim_royalties` into a
//! minimal runtime so we can drive real extrinsics in unit tests.

#![cfg(test)]

use frame_support::{
	derive_impl,
	parameter_types,
	traits::ConstU32,
};
use frame_system::EnsureRoot;
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
		PlimRoyalties: crate,
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

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AdminOrigin = EnsureRoot<AccountId>;
	type NativeCurrency = Balances;
	type WeightInfo = ();
}

/// Build test externalities with funded accounts.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("frame_system genesis builds");
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 1_000_000), (2, 1_000_000), (3, 1_000_000), (99, 10_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.expect("pallet_balances genesis assimilates");
	t.into()
}
