use sc_service::ChainType;
use plim_runtime::WASM_BINARY;
use serde_json::json;
use sp_core::crypto::Ss58Codec;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

/// Token properties for the Plim Chain.
fn plim_properties() -> serde_json::Map<String, serde_json::Value> {
	let mut properties = serde_json::Map::<String, serde_json::Value>::new();
	properties.insert("tokenSymbol".into(), json!("PLIM"));
	properties.insert("tokenDecimals".into(), json!(12));
	properties.insert("ss58Format".into(), json!(42));
	properties
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Plim Chain Development")
	.with_id("plim_dev")
	.with_protocol_id("plim")
	.with_chain_type(ChainType::Development)
	.with_properties(plim_properties())
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Plim Chain Testnet")
	.with_id("plim_testnet")
	.with_protocol_id("plim")
	.with_chain_type(ChainType::Local)
	.with_properties(plim_properties())
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.build())
}
