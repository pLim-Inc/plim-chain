use crate as pallet_plim_oracle;
use frame_support::derive_impl;
use frame_support::parameter_types;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type PlimOracle = pallet_plim_oracle::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub const MaxUpdaters: u32 = 5;
	pub const StalenessWindow: u64 = 100;
}

impl pallet_plim_oracle::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxUpdaters = MaxUpdaters;
	type StalenessWindow = StalenessWindow;
	type WeightInfo = ();
}

/// Build a fresh externalities backed by an empty oracle (no updaters, quorum = 0).
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}

/// Build externalities with a custom (initial_updaters, initial_quorum).
pub fn new_test_ext_with(updaters: alloc::vec::Vec<u64>, quorum: u32) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_plim_oracle::GenesisConfig::<Test> {
		initial_updaters: updaters,
		initial_quorum: quorum,
	}
	.assimilate_storage(&mut t)
	.unwrap();
	t.into()
}

extern crate alloc;
