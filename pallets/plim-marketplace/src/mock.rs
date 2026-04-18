//! Mock runtime for pallet-plim-marketplace tests.
//!
//! Wires `frame_system`, `pallet_balances`, and `pallet_plim_marketplace`
//! into a minimal runtime. License inspection, royalty callback and item
//! ownership are provided via configurable mocks.

#![cfg(test)]

use crate as pallet_plim_marketplace;
use crate::{ItemOwner, LicenseInspect, ListingCurrency, OnRoyaltyPayment};

use frame_support::{
	derive_impl,
	parameter_types,
	traits::ConstU32,
	PalletId,
};
use frame_system::EnsureSigned;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, Permill,
};

pub type AccountId = u64;
pub type Balance = u128;
pub type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Marketplace: pallet_plim_marketplace,
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
// Mock LicenseInspect / ItemOwner — controllable via thread-local storage
// ---------------------------------------------------------------------------

use core::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
	static TRANSFERABLE: RefCell<bool> = RefCell::new(true);
	static ROYALTY: RefCell<Option<(AccountId, u16)>> = RefCell::new(None);
	static ROYALTY_PAID_LOG: RefCell<Vec<(AccountId, u32, Balance, ListingCurrency)>> = RefCell::new(Vec::new());
	static OWNERSHIP: RefCell<BTreeMap<u32, AccountId>> = RefCell::new(BTreeMap::new());
}

pub struct MockLicenseInspect;

impl LicenseInspect<u32, AccountId> for MockLicenseInspect {
	fn is_transferable(_item_id: &u32) -> bool {
		TRANSFERABLE.with(|t| *t.borrow())
	}
	fn royalty_info(_item_id: &u32) -> Option<(AccountId, u16)> {
		ROYALTY.with(|r| r.borrow().clone())
	}
}

pub fn set_transferable(val: bool) {
	TRANSFERABLE.with(|t| *t.borrow_mut() = val);
}

pub fn set_royalty(val: Option<(AccountId, u16)>) {
	ROYALTY.with(|r| *r.borrow_mut() = val);
}

pub fn royalty_paid_log() -> Vec<(AccountId, u32, Balance, ListingCurrency)> {
	ROYALTY_PAID_LOG.with(|l| l.borrow().clone())
}

pub struct MockRoyaltyCallback;

impl OnRoyaltyPayment<AccountId, u32, Balance> for MockRoyaltyCallback {
	fn on_royalty_paid(
		creator: &AccountId,
		item_id: &u32,
		amount: Balance,
		currency: ListingCurrency,
	) {
		ROYALTY_PAID_LOG.with(|l| l.borrow_mut().push((*creator, *item_id, amount, currency)));
	}
}

pub struct MockItemOwner;

impl ItemOwner<u32, AccountId> for MockItemOwner {
	fn owner_of(item_id: &u32) -> Option<AccountId> {
		OWNERSHIP.with(|m| m.borrow().get(item_id).copied())
	}
	fn transfer(item_id: &u32, _from: &AccountId, to: &AccountId) -> Result<(), ()> {
		OWNERSHIP.with(|m| {
			m.borrow_mut().insert(*item_id, *to);
		});
		Ok(())
	}
}

pub fn set_item_owner(item_id: u32, owner: AccountId) {
	OWNERSHIP.with(|m| {
		m.borrow_mut().insert(item_id, owner);
	});
}

pub fn item_owner(item_id: u32) -> Option<AccountId> {
	OWNERSHIP.with(|m| m.borrow().get(&item_id).copied())
}

// ---------------------------------------------------------------------------
// Marketplace config
// ---------------------------------------------------------------------------

parameter_types! {
	pub const PEURAssetId: u32 = 100;
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const DefaultPlatformFeeBp: u16 = 1500; // 15%
	pub const MaxActiveListingsPerAccount: u32 = 5;
	pub const MaxBidsPerAuction: u32 = 64;
	pub const MaxAuctionsPerBlock: u32 = 32;
	pub const MinAuctionDuration: u32 = 10;
	pub MinBidIncrement: Permill = Permill::from_percent(2);
}

impl pallet_plim_marketplace::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MarketplaceOrigin = EnsureSigned<AccountId>;
	type NativeCurrency = Balances;
	type PEURAssetId = PEURAssetId;
	type TreasuryPalletId = TreasuryPalletId;
	type DefaultPlatformFeeBp = DefaultPlatformFeeBp;
	type MaxActiveListingsPerAccount = MaxActiveListingsPerAccount;
	type OnRoyaltyPayment = MockRoyaltyCallback;
	type LicenseInspect = MockLicenseInspect;
	type ItemOwner = MockItemOwner;
	type AuctionId = u64;
	type MaxBidsPerAuction = MaxBidsPerAuction;
	type MaxAuctionsPerBlock = MaxAuctionsPerBlock;
	type MinAuctionDuration = MinAuctionDuration;
	type MinBidIncrement = MinBidIncrement;
	type WeightInfo = ();
}

// ---------------------------------------------------------------------------
// Test externalities builder
// ---------------------------------------------------------------------------

/// Accounts:
/// - 1: seller (1_000_000 balance)
/// - 2: buyer  (1_000_000 balance)
/// - 3: creator / royalty recipient (1_000_000 balance)
/// - 4: secondary bidder (1_000_000 balance)
/// - 10: admin / marketplace origin
pub fn new_test_ext() -> sp_io::TestExternalities {
	// Reset thread-locals for each test
	set_transferable(true);
	set_royalty(None);
	ROYALTY_PAID_LOG.with(|l| l.borrow_mut().clear());
	OWNERSHIP.with(|m| m.borrow_mut().clear());

	let mut t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(1, 1_000_000),
			(2, 1_000_000),
			(3, 1_000_000),
			(4, 1_000_000),
			(10, 1_000_000),
			// Treasury and marketplace pallet account need ED to exist
			(Marketplace::treasury_account(), 1_000),
			(Marketplace::pallet_account(), 1_000),
		],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}
