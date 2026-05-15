# Security Audit — plim-chain — 2026-05-15

Scope: `plim-chain` substrate node — every pallet under `pallets/` and the runtime config (`runtime/src/lib.rs`, `runtime/src/configs/mod.rs`).
Branch audited: `feature/l99-pallet-extensions` @ `01b19d0` (matches parent `plim-protocol@develop` submodule pointer).
Auditor: Claude (Opus 4.7 · 1M context) under `chore/security-audit-chain`.

Methodology: line-by-line read of every `#[pallet::call]` extrinsic, runtime `Config` impl, and `#[pallet::storage]` map; checked origin gates, storage growth bounds, weight wiring, BoundedVec caps, sudo/root surface, cryptographic operations, and hardcoded seeds. `cargo clippy --workspace` was attempted but skipped because the worktree has no compiled target cache and a clean build would take ~30 min on this host (see Notes below).

## Severity counts

| Severity  | Count |
|-----------|------:|
| Critical  | 1     |
| High      | 6     |
| Medium    | 9     |
| Low       | 4     |
| Total     | 20    |

## Findings

| ID | Severity | File:line | Issue | Status | Notes |
|----|----------|-----------|-------|--------|-------|
| C-01 | Critical | `pallets/plim-royalties/src/lib.rs:214-241` | `claim_accumulated_royalties` for `RoyaltyCurrency::PLIM` calls `NativeCurrency::deposit_into_existing(&creator, amount)` — this **mints PLIM out of thin air**. Marketplace's `do_split_payout` (`pallets/plim-marketplace/src/lib.rs:1124-1147`) already transfers `royalty_amount` directly from buyer → creator at sale time, and then the `RoyaltyBridge` (`runtime/src/configs/mod.rs:352-368`) re-records the SAME amount into `AccumulatedRoyalties`. A creator who later claims the PLIM bucket receives the royalty a **second** time, freshly minted. | DEFERRED | Cannot patch safely under the task's "no extrinsic signature change, no pallet-rwa math change" guardrails — but the fix is small: drop the `deposit_into_existing` arm (treat PLIM like PEUR/EURFiat — event-only because marketplace already paid on-chain) **or** reroute marketplace PLIM royalty into a royalties-pallet pot account before the bridge accumulates. Track in a follow-up PR; meanwhile recommend pausing `claim_accumulated_royalties` on mainnet via a runtime call-filter until fixed. |
| H-01 | High | `pallets/plim-timestamps/src/lib.rs:60-72` | `anchor` accepts any caller-controlled `[u8; 32]` and inserts one storage row per call. No per-account cap, no deposit, no expiry sweep — unbounded `Anchors` growth at the cost of one signed extrinsic per row. | DEFERRED | Fix requires adding a `Config` constant + counter doublemap (extrinsic signature stays the same but storage layout adds an account→count map). Slightly larger than the safe-fix budget. Mitigation in interim: rely on transaction-fee floor + future per-account `MaxAnchorsPerAccount` cap. |
| H-02 | High | `pallets/plim-mandates/src/lib.rs:80-108` | `create` accepts caller-controlled `mandate_ref: [u8; 32]` with no per-payer cap (unlike its sibling `pallet-plim-payments` which does enforce `MaxMandatesPerAccount`). A signed account can spam unique `mandate_ref` values indefinitely. | DEFERRED | Same shape of fix as `pallet-plim-payments`: add `MaxMandatesPerAccount` constant + `MandateCount` doublemap. Punt to a follow-up. |
| H-03 | High | `pallets/plim-channels/src/lib.rs:79-108` | `open` accepts caller-controlled `channel_id: [u8; 32]` with no cap, no deposit, no fund movement. A signed account can spam unique channel ids → unbounded `Channels` storage. The `close` extrinsic accepts and stores a signature blob but **does not verify it** (acknowledged in the pallet docstring as v2-deferred). | DEFERRED | Documented design gap. Add a `MaxOpenChannelsPerAccount` cap and either (a) actually reserve `deposit` via `Currency::reserve` or (b) gate `open` behind a `ChannelOriginatorOrigin`. The signature-verify TODO blocks any production use; recommend a hard `Disabled` call-filter on mainnet until v3. |
| H-04 | High | `pallets/plim-delegation/src/lib.rs:97-122` | `delegate` accepts arbitrary `(delegator, delegate)` rows with no cap on number of delegates per delegator. Combined with the doublemap layout, a single signed delegator can mint storage rows against many delegate accounts. | DEFERRED | Add `MaxDelegatesPerDelegator` constant + counter map; same shape of fix as `H-02`. |
| H-05 | High | `pallets/plim-licenses/src/lib.rs:230-314` | `create_license_template` is signed-only (any account can call). Each call stores a `LicenseDataOf<T>` keyed by the SCALE-hash of the template — caller can pick deeply distinct templates and spam the `LicenseTemplates` map. No cap, no deposit. | DEFERRED | Either gate behind `MarketplaceOrigin` (the same origin that mints licenses) or charge a deposit. Frontend impact: any UX that currently lets unauthenticated creators call this extrinsic must switch to the gateway path. |
| H-06 | High | `pallets/plim-licenses/src/lib.rs:512-543` | `claim_custody_license` authorises the caller as the new owner whenever `claim_nonce == 0` (the storage default) and `custody.claimed == false`. The stored `buyer_email_hash` is **never** consumed by the check, so anybody who can read `CustodyQueue` (public on-chain) can front-run the legitimate buyer and steal the license. | DEFERRED | Comment hints at a future signature/nonce protocol. Until then, the only protection is "off-chain side never publishes the item-id before fronting the claim from the buyer's wallet" — fragile. Recommend either gating this extrinsic behind `MarketplaceOrigin` (backend dispatches on behalf of the buyer after Stripe webhook) or making it consume a signed claim payload that proves knowledge of the email pre-image. |
| M-01 | Medium | `pallets/plim-mesh-relay/src/lib.rs:186` | Extrinsic uses `Weight::from_parts(50_000_000, 4096)` directly even though `pallets/plim-mesh-relay/src/weights.rs` exists with a benchmarked `submit_relayed_transaction()` weight (`57_340_000 ps + 3517 PoV + 3R3W`). The pallet's `Config` trait does **not** declare a `WeightInfo` associated type, so the benchmark output is orphaned. | DEFERRED | The fix requires adding `type WeightInfo: WeightInfo;` to `Config`, a local `pub trait WeightInfo { fn submit_relayed_transaction() -> Weight; }`, and wiring `type WeightInfo = pallet_plim_mesh_relay::weights::WeightInfo<Runtime>;` in the runtime config. Three-file change but mechanical; deferred to keep this PR audit-only. |
| M-02 | Medium | `pallets/plim-payments/src/lib.rs:175,214,230,279` | All four extrinsics hardcode `Weight::from_parts(10_000, 0)` (PoV = 0). No `WeightInfo` trait or `Config::WeightInfo`. Block production has no idea of the actual cost; mainnet block weight accounting is currently a guess for this pallet. | DEFERRED | Same shape as M-01. Add the trait + Config item + benchmark, then wire in `runtime/src/configs/mod.rs:271`. |
| M-03 | Medium | `pallets/plim-mandates/src/lib.rs:79,112` | Same pattern as M-02. | DEFERRED | — |
| M-04 | Medium | `pallets/plim-channels/src/lib.rs:78,117` | Same pattern. | DEFERRED | — |
| M-05 | Medium | `pallets/plim-delegation/src/lib.rs:97,126,140` | Same pattern. | DEFERRED | — |
| M-06 | Medium | `pallets/plim-compliance/src/lib.rs:59,68,77` | Same pattern. Compliance extrinsics are root-only so block-weight accounting is less load-bearing, but the runtime config still wires `WeightInfo = ()` indirectly. | DEFERRED | — |
| M-07 | Medium | `pallets/plim-reputation/src/lib.rs:74,88` | Same pattern. | DEFERRED | — |
| M-08 | Medium | `pallets/plim-timestamps/src/lib.rs:60` | Same pattern (single extrinsic). | DEFERRED | — |
| M-09 | Medium | `pallets/plim-identity/src/lib.rs:103,130,148,168` | Same pattern. | DEFERRED | — |
| L-01 | Low | `pallets/plim-marketplace/src/lib.rs:686-726` | `make_offer` lets any signed account write an `Offers` row keyed by `T::Hashing::hash_of(&(bidder, item_id, now))`. No per-account cap, no deposit, no auto-expiry GC. An attacker who can keep producing fresh `(bidder, item_id, block)` tuples can spam the `Offers` map indefinitely. Per-block uniqueness limits the rate, but over weeks the map grows unboundedly. | DEFERRED | Add per-account cap + `on_initialize` expired-offer sweep (mirror the licenses pallet's 100-block sweep with `MAX_SWEEP_PER_BLOCK = 50`). |
| L-02 | Low | `pallets/plim-marketplace/src/lib.rs:1237-1290` | `process_ended_auctions` (called from `on_idle`) iterates `AuctionsByEndBlock::<T>::iter()` — an unbounded scan over every end-block bucket. The inner loop is correctly bounded by `MAX_PER_BLOCK = 50`, but the outer iter walks every key in the index every block until the bucket is emptied. For a long-running chain with thousands of expired auctions, this becomes a per-block O(n) read pinned to `on_idle` remaining weight. | DEFERRED | Track a `NextEndBlockToScan` cursor that only advances after a bucket is fully drained; or use `drain_prefix` over `block <= now` ranges in chunks. |
| L-03 | Low | `pallets/plim-licenses/src/lib.rs:189-219` | `on_initialize` iterates `Licenses::<T>::iter()` every `SWEEP_INTERVAL` (100 blocks), bounded by `MAX_SWEEP_PER_BLOCK = 50`. As the licenses map grows past tens of thousands, each sweep block still pays O(n) reads to find the first 50 expired rows. | DEFERRED | Maintain a secondary `ExpiringAt: StorageMap<BlockNumber, BoundedVec<u32, _>>` index so the sweep walks the index rather than every license. |
| L-04 | Low | `pallets/plim-kyc/src/lib.rs:250-265` | `revoke_kyc` lets **any** registered attestor revoke **any** account's KYC, not just records they attested. Documented as intentional in the pallet docstring (any attestor can revoke under emergency), but worth flagging because a single rogue attestor key can revoke the entire user base. | ACCEPTED | Operational mitigation: keep the attestor set small (`MaxKycAttestors = 20`) and root-gated; tie attestor admission to council vote in v3. |

## Pass — items explicitly checked and found clean

1. **Dispatchable origin checks.** Every `#[pallet::call]` extrinsic I read calls `ensure_signed`, `ensure_root`, `ensure_signed_or_root`, or a typed `EnsureOrigin::ensure_origin`. No extrinsic accepts a raw `OriginFor<T>` and then mutates storage.
2. **L99 mesh-relay storage bounds (§3.1.5).** The `RelayedTransactions` refactor from `StorageValue<BoundedVec<…>>` → `StorageMap` is in place. `RelayedQueueCount` doubles as the live-queue length and is checked against `T::MaxRelayedQueue = 10_000` (`runtime/src/configs/mod.rs:312`) before every insert. PoV for a single relay is now O(1) (~3.5 KiB), per the orphaned benchmark in `pallets/plim-mesh-relay/src/weights.rs`.
3. **`pallet-rwa::claim_all_yield` cap.** `MaxDistributionsPerClaim = 50` is wired (`runtime/src/configs/mod.rs:415`). The for-loop in `pallets/plim-rwa/src/lib.rs:629-639` short-circuits at `cleared.len() >= limit`. No reentrancy: the loop is read-only over `YieldDistributions`, accumulates into a local `Vec`, then performs a single `transfer()` and N storage writes — no callbacks to external pallets between the read and the write.
4. **`pallet-rwa::distribute_yield` arithmetic.** Uses `checked_add` for `TotalIssued`, `saturating_*` for share math, `checked_div(issued_u128)` for the pro-rata floor. Total is bounded by `MaxShareholdersPerDistribution = 10_000` (line 416 of runtime config), enforced at line 506 of `pallets/plim-rwa/src/lib.rs`.
5. **ed25519 verification in no_std path.** `pallets/plim-mesh-relay/src/lib.rs:227` uses `sp_io::crypto::ed25519_verify`, consistent with the project_protocol_integration_2026-05-14 fix.
6. **BoundedVec bounds.** Every `BoundedVec<u8, ConstU32<N>>` I found uses N ∈ {16, 64, 128, 256, 1024} — sane caps. No `ConstU32<u32::MAX>` or `ConstU32<{ … large }>`.
7. **Sudo / root usage.** `EnsureRoot` is used only for: RWA `register_asset` / `freeze` / `unfreeze` / `wind_down`, KYC attestor + sanction management, oracle updater + quorum management, compliance sanction set, reputation `award`. No user-facing extrinsic requires root by mistake.
8. **No hardcoded secrets / Alice seeds in production runtime.** `runtime/src/genesis_config_presets.rs` keeps the Alice keys behind the dev preset; mainnet-v2 genesis preset embeds only public keys (mnemonics live at `/root/SECURE/mainnet_v2_keys_2026-04-14.txt`). `grep -rE '"0x[0-9a-fA-F]{40,}"' pallets/ runtime/` returns nothing.

## Benchmark registry gap (task §3)

`runtime/src/benchmarks.rs:26-38` registers 5 of the 11 custom pallets via `define_benchmarks!`:

Registered: `pallet_plim_marketplace`, `pallet_plim_oracle`, `pallet_plim_kyc`, `pallet_rwa`, `pallet_plim_mesh_relay`.

Missing: `pallet_plim_identity`, `pallet_plim_payments`, `pallet_plim_mandates`, `pallet_plim_channels`, `pallet_plim_delegation`, `pallet_plim_compliance`, `pallet_plim_reputation`, `pallet_plim_timestamps`, `pallet_plim_licenses`, `pallet_plim_royalties`.

This is consistent with the M-02 — M-09 findings: those pallets have no `WeightInfo` config and therefore nothing to benchmark yet. Recommend bundling the trait-introduction PR with the `define_benchmarks!` registration.

## Notes on the build verification

`cargo clippy --workspace --no-deps --tests` was attempted on the audit worktree (`/opt/plimlab/wt-chain-audit`). The worktree had no compiled `target/` directory and the parent submodule's `target/` is incompatible across worktrees (Cargo locks the build dir). A cold compile of the polkadot-sdk dependency graph on this host (no `plimagent` cargo-cache available locally) was estimated at >30 min, so per the task guardrails (`skip the build verification`) I did not run it. No auto-fixed clippy lints are included in this PR.

The benchmark file `pallets/plim-mesh-relay/src/weights.rs` was generated on `plimagentserver` 2026-05-14, so the cargo cache **is** primed on `plimagent` — a follow-up clippy pass should ride on that host.

## Inline fixes applied in this PR

None. Every candidate fix landed in a category the task explicitly defers:

- Adding `T::WeightInfo` calls — would require introducing the `WeightInfo` trait + `Config::WeightInfo` item in each pallet (extrinsic signatures stay the same, but adding a Config type is a small interface change and would unsettle every downstream test mock).
- Tightening `BoundedVec` bounds — every bound I found was already ≤1024 and load-bearing for downstream type signatures.
- `clippy --fix` for trivial lints — not run for the reason above.
- Adding `ensure_signed` — every extrinsic already has an origin gate; none are missing the check.

The PR therefore consists of this document only. The followup PRs (one per H/M finding) should land before the next mainnet runtime upgrade.

---

🤖 Generated with [Claude Code](https://claude.com/claude-code)
