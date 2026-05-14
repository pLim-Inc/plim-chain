//! L99 Workstream A — Identity v1 -> v2 migration.
//!
//! Adds the `device_attestation_hash` field (defaulted to `None`) to every
//! existing `IdentityInfo` row. Runs at runtime upgrade from spec_version
//! 301 -> 302.

use crate::{Config, Identities, IdentityInfo, IdentityInfoV1};
use frame_support::traits::{Get, OnRuntimeUpgrade};
use frame_support::weights::Weight;

pub struct Migration<T>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut count: u64 = 0;
		Identities::<T>::translate::<IdentityInfoV1<T>, _>(|_key, v1| {
			count += 1;
			Some(IdentityInfo::<T> {
				display_name: v1.display_name,
				country: v1.country,
				verified_at: v1.verified_at,
				verifier: v1.verifier,
				device_attestation_hash: None,
			})
		});
		frame_support::__private::log::info!(
			target: "runtime::plim-identity",
			"L99 v1->v2 device_attestation: defaulted {} identities to None",
			count,
		);
		T::DbWeight::get().reads_writes(count, count)
	}
}
