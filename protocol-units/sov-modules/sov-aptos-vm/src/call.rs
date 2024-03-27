use anyhow::Result;
use aptos_types::block_executor::config::BlockExecutorConfigFromOnchain;
use aptos_types::transaction::signature_verified_transaction::SignatureVerifiedTransaction;
use reth_primitives::{Log as RethLog, TransactionSignedEcRecovered};
use revm::primitives::{Address, CfgEnv, CfgEnvWithHandlerCfg, EVMError, Log, SpecId};
use sov_modules_api::{
	CallResponse, Context, DaSpec, StateValueAccessor, StateVecAccessor, WorkingSet,
};
use tracing::log;

use crate::aptos::db::AptosDb;
use crate::aptos::executor::{self};
use crate::aptos::primitive_types::{BlockEnv, Receipt, TransactionSignedAndRecovered};
use crate::aptos::{AptosChainConfig, RlpEvmTransaction};
use crate::experimental::{AptosVM, PendingTransaction, SovAptosVM};

/// aptos call message.
#[derive(
	borsh::BorshDeserialize,
	borsh::BorshSerialize,
	serde::Serialize,
	serde::Deserialize,
	Debug,
	PartialEq,
	Clone,
)]
pub struct CallMessage {
	/// RLP encoded transaction.
	pub tx: RlpEvmTransaction,
}

impl<S: sov_modules_api::Spec, Da: DaSpec> SovAptosVM<S, Da> {
	pub(crate) fn execute_call(
		&self,
		txs: &[SignatureVerifiedTransaction],
		_context: &Context<S>,
		working_set: &mut WorkingSet<S>,
	) -> Result<CallResponse> {
		let state = self.get_db(working_set).state_view_at_version(None)?;
		let result = executor::execute_block_no_limit(&state, txs)?;
		log::info!("execute_call: result: {:?}", result);
		Ok(CallResponse {})
	}
}

/// builds CfgEnvWithHandlerCfg
/// Returns correct config depending on spec for given block number
// Copies context-dependent values from template_cfg or default if not provided
pub(crate) fn get_cfg_env_with_handler(
	block_env: &BlockEnv,
	cfg: AptosChainConfig,
	template_cfg: Option<CfgEnv>,
) -> CfgEnvWithHandlerCfg {
	todo!()
}

/// Get spec id for a given block number
/// Returns the first spec id defined for block >= block_number
pub(crate) fn get_spec_id(spec: Vec<(u64, SpecId)>, block_number: u64) -> u64 {
	// not sure we need this for sov-aptos, the values can be hardcoded
	todo!()
}

/// Copied from <https://github.com/paradigmxyz/reth/blob/e83d3aa704f87825ca8cab6f593ab4d4adbf6792/crates/revm/revm-primitives/src/compat.rs#L17-L23>.
/// All rights reserved.
///
/// By copying the code, we can avoid depending on the whole crate.
pub fn into_reth_log(log: Log) -> RethLog {
	RethLog { address: Address(log.address.0), topics: log.topics().to_vec(), data: log.data.data }
}
