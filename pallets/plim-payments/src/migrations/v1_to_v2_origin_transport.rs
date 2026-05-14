//! L99 Workstream A — Mandate v1 -> v2 migration.
//!
//! Adds the `payment_origin_transport_code` field to every existing
//! `MandateInfo` row, defaulting to `PaymentOriginTransportCode::Https`.
//!
//! Runs at runtime upgrade from spec_version 301 -> 302.

use crate::{Config, MandateInfo, MandateInfoV1, Mandates, PaymentOriginTransportCode};
use frame_support::traits::{Get, OnRuntimeUpgrade};
use frame_support::weights::Weight;

pub struct Migration<T>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
    fn on_runtime_upgrade() -> Weight {
        let mut count: u64 = 0;
        Mandates::<T>::translate::<MandateInfoV1<T>, _>(|_key, v1| {
            count += 1;
            Some(MandateInfo::<T> {
                payer: v1.payer,
                payee: v1.payee,
                asset_id: v1.asset_id,
                allowance: v1.allowance,
                expires_at: v1.expires_at,
                payment_origin_transport_code: PaymentOriginTransportCode::Https,
            })
        });
        frame_support::__private::log::info!(
            target: "runtime::plim-payments",
            "L99 v1->v2 origin_transport: backfilled {} mandates with Https",
            count,
        );
        T::DbWeight::get().reads_writes(count, count)
    }
}
