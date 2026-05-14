//! Storage migrations for `pallet-plim-identity`.
//!
//! - `v1_to_v2_device_attestation`: backfills `device_attestation_hash` on
//!   existing `Identities` rows with `None` when upgrading from
//!   spec_version 301 -> 302 (L99 Workstream A).

pub mod v1_to_v2_device_attestation;
