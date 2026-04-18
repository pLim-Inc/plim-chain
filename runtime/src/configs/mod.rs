// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use frame_support::{
	derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstBool, ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, VariantCountOf},
	weights::{
		constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
		IdentityFee, Weight,
	},
	PalletId,
};
use frame_system::{limits::{BlockLength, BlockWeights}, EnsureRoot, EnsureSigned};
use pallet_transaction_payment::{ConstFeeMultiplier, FungibleAdapter, Multiplier};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{traits::One, Perbill, Permill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
	AccountId, Aura, Balance, Balances, Block, BlockNumber, Hash, Nonce, PalletInfo, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask,
	PlimLicenses, PlimRoyalties,
	Signature, System, DAYS, EXISTENTIAL_DEPOSIT, SLOT_DURATION, UNIT, VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;

	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub RuntimeBlockLength: BlockLength = BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	// Base deposit held while a multisig call is pending, per polkadot-sdk reference values.
	// ~1 UNIT base + ~0.05 UNIT per additional signatory keeps the floor cheap enough for
	// a 3-of-5 council operation but still discourages spam.
	pub const MultisigDepositBase: Balance = UNIT;
	pub const MultisigDepositFactor: Balance = UNIT / 20;
	pub const MaxMultisigSignatories: u32 = 16;
}

/// pallet-multisig — enables N-of-M threshold accounts (the 3-of-5 council
/// that will take over `Sudo::set_key` in runtime spec_version 102).
/// The multi_account_id derivation is deterministic and chain-agnostic, so
/// the 3-of-5 SS58 address can be computed off-chain and injected via
/// `Sudo::set_key(multisig)` as soon as this runtime upgrade lands.
impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type DepositBase = MultisigDepositBase;
	type DepositFactor = MultisigDepositFactor;
	type MaxSignatories = MaxMultisigSignatories;
	type WeightInfo = pallet_multisig::weights::SubstrateWeight<Runtime>;
	type BlockNumberProvider = System;
}

/// Configure pallet-assets for the multi-token catalog
/// (ePL / gPLIM / pEUR / pUSD — PLIM native stays in pallet-balances).
impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = u32;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = ConstU128<{ 100 * UNIT }>;
	type AssetAccountDeposit = ConstU128<{ UNIT }>;
	type MetadataDepositBase = ConstU128<{ 10 * UNIT }>;
	type MetadataDepositPerByte = ConstU128<{ UNIT }>;
	type ApprovalDeposit = ConstU128<{ UNIT }>;
	type StringLimit = ConstU32<50>;
	type Freezer = ();
	type Holder = ();
	type Extra = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// Configure pallet-nfts for the P:L:I:M:/Chain Artbook (250 NFTs)
parameter_types! {
	pub const NftCollectionDeposit: Balance = 100 * UNIT;
	pub const NftItemDeposit: Balance = UNIT;
	pub const NftMetadataDepositBase: Balance = 10 * UNIT;
	pub const NftMetadataDepositPerByte: Balance = UNIT;
	pub const NftAttributeDepositBase: Balance = UNIT;
	pub const NftKeyLimit: u32 = 64;
	pub const NftValueLimit: u32 = 256;
	pub const NftApprovalsLimit: u32 = 20;
	pub const NftItemAttributesApprovalsLimit: u32 = 20;
	pub const NftMaxTips: u32 = 10;
	pub const NftMaxDeadlineDuration: BlockNumber = 12 * 30 * DAYS;
	pub NftFeatures: pallet_nfts::PalletFeatures = pallet_nfts::PalletFeatures::all_enabled();
}

impl pallet_nfts::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CollectionId = u32;
	type ItemId = u32;
	type Currency = Balances;
	type ForceOrigin = EnsureRoot<AccountId>;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type Locker = ();
	type CollectionDeposit = NftCollectionDeposit;
	type ItemDeposit = NftItemDeposit;
	type MetadataDepositBase = NftMetadataDepositBase;
	type AttributeDepositBase = NftAttributeDepositBase;
	type DepositPerByte = NftMetadataDepositPerByte;
	type StringLimit = ConstU32<256>;
	type KeyLimit = NftKeyLimit;
	type ValueLimit = NftValueLimit;
	type ApprovalsLimit = NftApprovalsLimit;
	type ItemAttributesApprovalsLimit = NftItemAttributesApprovalsLimit;
	type MaxTips = NftMaxTips;
	type MaxDeadlineDuration = NftMaxDeadlineDuration;
	type MaxAttributesPerCall = ConstU32<10>;
	type Features = NftFeatures;
	type OffchainSignature = Signature;
	type OffchainPublic = <Signature as sp_runtime::traits::Verify>::Signer;
	type BlockNumberProvider = System;
	type WeightInfo = pallet_nfts::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
}

/// P:L:I:M: custom pallets configuration
///
/// Updated 2026-04-14T15:00 — concrete genesis allocations + 7 pallet implementations
/// All custom-pallet origins are temporarily wired to `EnsureRoot` until the
/// governance council goes live in v3.

impl pallet_plim_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxNameLen = ConstU32<64>;
	type VerifierOrigin = EnsureRoot<AccountId>;
}

impl pallet_plim_payments::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type MaxMandatesPerAccount = ConstU32<64>;
}

impl pallet_plim_mandates::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_plim_channels::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_plim_delegation::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// 14_400 blocks ≈ 1 day at 6s blocks.
	type BlocksPerDay = ConstU32<14_400>;
}

impl pallet_plim_compliance::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ComplianceOrigin = EnsureRoot<AccountId>;
}

impl pallet_plim_reputation::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AdminOrigin = EnsureRoot<AccountId>;
	// 100_800 blocks ≈ 1 week at 6s blocks.
	type AttestCooldown = ConstU32<100_800>;
}

impl pallet_plim_timestamps::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

// ---------------------------------------------------------------------------
// 3dplim Marketplace pallets (spec_version 200)
// ---------------------------------------------------------------------------

parameter_types! {
	pub const LicenseCollectionId: u32 = 1;
	pub const MaxGeoRestrictions: u32 = 32;
	pub const MaxPlatformRestrictions: u32 = 16;
	pub const DefaultPlatformFeeBp: u16 = 1500;         // 15%
	pub const MaxActiveListingsPerAccount: u32 = 100;
	pub const PEURAssetId: u32 = 3;                     // pEUR in pallet-assets
	pub const MarketplaceTreasuryPalletId: PalletId = PalletId(*b"py/trsry");
}

impl pallet_plim_licenses::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MarketplaceOrigin = EnsureRoot<AccountId>;
	type AdminOrigin = EnsureRoot<AccountId>;
	type LicenseCollectionId = LicenseCollectionId;
	type MaxGeoRestrictions = MaxGeoRestrictions;
	type MaxPlatformRestrictions = MaxPlatformRestrictions;
	type WeightInfo = ();
}

/// Adapter: bridges pallet-plim-licenses helpers → pallet-plim-marketplace::LicenseInspect.
pub struct LicenseBridge;
impl pallet_plim_marketplace::LicenseInspect<u32, AccountId> for LicenseBridge {
	fn is_transferable(item_id: &u32) -> bool {
		pallet_plim_licenses::Pallet::<Runtime>::is_transferable(*item_id)
	}
	fn royalty_info(item_id: &u32) -> Option<(AccountId, u16)> {
		pallet_plim_licenses::Pallet::<Runtime>::royalty_info(*item_id)
	}
}

/// Adapter: bridges pallet-plim-royalties → pallet-plim-marketplace::OnRoyaltyPayment.
pub struct RoyaltyBridge;
impl pallet_plim_marketplace::OnRoyaltyPayment<AccountId, u32, Balance> for RoyaltyBridge {
	fn on_royalty_paid(
		creator: &AccountId,
		item_id: &u32,
		amount: Balance,
		currency: pallet_plim_marketplace::types::ListingCurrency,
	) {
		let royalty_currency = match currency {
			pallet_plim_marketplace::types::ListingCurrency::PLIM => pallet_plim_royalties::types::RoyaltyCurrency::PLIM,
			pallet_plim_marketplace::types::ListingCurrency::PEUR => pallet_plim_royalties::types::RoyaltyCurrency::PEUR,
			pallet_plim_marketplace::types::ListingCurrency::EURFiat => pallet_plim_royalties::types::RoyaltyCurrency::EURFiat,
		};
		pallet_plim_royalties::Pallet::<Runtime>::record_royalty_payment(creator, item_id, amount, royalty_currency);
	}
}

parameter_types! {
	pub const MarketplaceMaxBidsPerAuction: u32 = 256;
	pub const MarketplaceMaxAuctionsPerBlock: u32 = 64;
	pub const MarketplaceMinAuctionDuration: u32 = 100;       // ~10 min @ 6s
	pub const MarketplaceMinBidIncrement: Permill = Permill::from_percent(2);
}

impl pallet_plim_marketplace::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MarketplaceOrigin = EnsureRoot<AccountId>;
	type NativeCurrency = Balances;
	type PEURAssetId = PEURAssetId;
	type TreasuryPalletId = MarketplaceTreasuryPalletId;
	type DefaultPlatformFeeBp = DefaultPlatformFeeBp;
	type MaxActiveListingsPerAccount = MaxActiveListingsPerAccount;
	type OnRoyaltyPayment = RoyaltyBridge;
	type LicenseInspect = LicenseBridge;
	// TODO(spec 400): wire `ItemOwner` to a `pallet-nfts` adapter so auctions
	// actually move on-chain NFT items. For spec 300 the unit `()` impl is a
	// no-op (`owner_of => None`, `transfer => Ok(())`) which keeps auction
	// extrinsics dispatchable without coupling marketplace to pallet-nfts yet.
	type ItemOwner = ();
	type AuctionId = u64;
	type MaxBidsPerAuction = MarketplaceMaxBidsPerAuction;
	type MaxAuctionsPerBlock = MarketplaceMaxAuctionsPerBlock;
	type MinAuctionDuration = MarketplaceMinAuctionDuration;
	type MinBidIncrement = MarketplaceMinBidIncrement;
	type WeightInfo = ();
}

impl pallet_plim_royalties::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AdminOrigin = EnsureRoot<AccountId>;
	type NativeCurrency = Balances;
	type WeightInfo = ();
}

// ---------------------------------------------------------------------------
// P:L:I:M:/Protocol — Oracle, KYC & RWA tokenization (spec_version 300)
// ---------------------------------------------------------------------------

parameter_types! {
	pub const MaxOracleUpdaters: u32 = 10;
	pub const OracleStalenessWindow: BlockNumber = 100;       // ~10 min @ 6s
	pub const MaxKycAttestors: u32 = 20;
	pub const MaxRwaDistributionsPerClaim: u32 = 50;
	pub const MaxRwaShareholdersPerDistribution: u32 = 10_000;
}

impl pallet_plim_oracle::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxUpdaters = MaxOracleUpdaters;
	type StalenessWindow = OracleStalenessWindow;
	type WeightInfo = ();
}

impl pallet_plim_kyc::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxAttestors = MaxKycAttestors;
	type WeightInfo = ();
}

impl pallet_rwa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RwaAssetId = u32;
	type DistributionId = u64;
	// TODO(spec 400): wire `Kyc = PlimKyc` once a cross-pallet adapter
	// bridges `pallet_plim_kyc::KycProvider` → `pallet_rwa::KycProvider`
	// (the two pallets define mirror traits to stay decoupled).
	// For spec 300 we use the default permissive `()` impl so RWA flows
	// can be exercised without an attestor-set bootstrap.
	type Kyc = ();
	type MaxDistributionsPerClaim = MaxRwaDistributionsPerClaim;
	type MaxShareholdersPerDistribution = MaxRwaShareholdersPerDistribution;
	type WeightInfo = ();
}
