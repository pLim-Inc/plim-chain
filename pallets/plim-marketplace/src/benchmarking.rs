//! Benchmarking scaffolding for pallet-plim-marketplace.
//!
//! This file provides the skeleton for `frame-benchmarking` integration.
//! Actual benchmark bodies will be implemented once the runtime is wired
//! and we have concrete worst-case scenarios profiled.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn list_for_sale() {
		// TODO: set up worst-case state and call extrinsic
		#[block]
		{}
	}

	#[benchmark]
	fn cancel_listing() {
		#[block]
		{}
	}

	#[benchmark]
	fn buy_now() {
		#[block]
		{}
	}

	#[benchmark]
	fn buy_now_with_fiat_proof() {
		#[block]
		{}
	}

	#[benchmark]
	fn make_offer() {
		#[block]
		{}
	}

	#[benchmark]
	fn accept_offer() {
		#[block]
		{}
	}

	#[benchmark]
	fn reject_offer() {
		#[block]
		{}
	}

	#[benchmark]
	fn withdraw_offer() {
		#[block]
		{}
	}

	#[benchmark]
	fn update_platform_fee() {
		#[block]
		{}
	}

	#[benchmark]
	fn create_auction() {
		#[block]
		{}
	}

	#[benchmark]
	fn bid_auction() {
		#[block]
		{}
	}

	#[benchmark]
	fn settle_auction() {
		#[block]
		{}
	}

	#[benchmark]
	fn cancel_auction() {
		#[block]
		{}
	}
}
