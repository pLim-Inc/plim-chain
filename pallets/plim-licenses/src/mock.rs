use crate as pallet_plim_licenses;
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
	pub type PlimLicenses = pallet_plim_licenses::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub const LicenseCollectionId: u32 = 1;
	pub const MaxGeoRestrictions: u32 = 10;
	pub const MaxPlatformRestrictions: u32 = 5;
}

impl pallet_plim_licenses::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	// In tests, any signed origin acts as marketplace (EnsureSigned<u64>).
	// We use EnsureRoot for marketplace + admin to simplify test setup.
	type MarketplaceOrigin = frame_system::EnsureRoot<u64>;
	type AdminOrigin = frame_system::EnsureRoot<u64>;
	type LicenseCollectionId = LicenseCollectionId;
	type MaxGeoRestrictions = MaxGeoRestrictions;
	type MaxPlatformRestrictions = MaxPlatformRestrictions;
	type WeightInfo = ();
}

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
