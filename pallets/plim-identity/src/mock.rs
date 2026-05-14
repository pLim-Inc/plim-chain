//! Mock runtime for `pallet-plim-identity` tests.

#![cfg(test)]

use frame_support::{derive_impl, traits::ConstU32};
use frame_system::EnsureRoot;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

pub type AccountId = u64;
pub type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		PlimIdentity: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type Hash = H256;
	type Hashing = BlakeTwo256;
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxNameLen = ConstU32<64>;
	type VerifierOrigin = EnsureRoot<AccountId>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
