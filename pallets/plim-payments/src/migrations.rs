//! Storage migrations for `pallet-plim-payments`.
//!
//! - `v1_to_v2_origin_transport`: backfills `payment_origin_transport_code`
//!   on existing `Mandates` rows with `PaymentOriginTransportCode::Https`
//!   when upgrading from spec_version 301 -> 302 (L99 Workstream A).

pub mod v1_to_v2_origin_transport;
