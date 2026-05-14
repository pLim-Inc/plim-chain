//! Mock runtime for `pallet-plim-mesh-relay` tests.

#![cfg(test)]

use frame_support::{
    derive_impl,
    traits::{ConstU128, ConstU32},
};
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
        PlimMeshRelay: crate,
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
    type MaxRelayedQueue = ConstU32<10_000>;
    type MaxPayloadLen = ConstU32<1024>;
    // 10 EUR @ 12 decimals.
    type DefaultOfflineTxValueCap = ConstU128<10_000_000_000_000>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext: sp_io::TestExternalities = t.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}
