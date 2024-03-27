mod aptos;
mod call;
mod event;
mod genesis;
mod helpers;
mod rpc;
mod signer;

pub use signer::DevSigner;

mod experimental {
	use super::aptos::db::SovAptosDb;
	use super::aptos::{AptosChainConfig, DbAccount};
	use super::event::Event;
	use super::genesis::AptosConfig;
	use crate::aptos::primitive_types::{
		BlockEnv, Receipt, SealedBlock, TransactionSignedAndRecovered,
	};
	use aptos_api_types::{Address, HexEncodedBytes, MoveModuleBytecode, MoveResource};
	use aptos_consensus_types::block::Block;
	use aptos_crypto::bls12381::Signature;
	use aptos_db::AptosDB;
	use aptos_types::state_store::state_key::StateKey;
	use aptos_types::state_store::state_value::StateValue as AptosStateValue;
	use aptos_types::transaction::Version;
	use sov_modules_api::{Context, DaSpec, Error, ModuleInfo, StateValue, WorkingSet};
	use sov_state::codec::BcsCodec;

	// @TODO: Check these vals. Make tracking issue.
	#[cfg(feature = "native")]
	pub(crate) const MIN_TRANSACTION_GAS: u64 = 21_000u64;
	#[cfg(feature = "native")]
	pub(crate) const MIN_CREATE_GAS: u64 = 53_000u64;

	#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
	pub(crate) struct PendingTransaction {
		pub(crate) transaction: TransactionSignedAndRecovered,
		pub(crate) receipt: Receipt,
	}

	/// The sov-aptos module provides compatibility with the Aptos VM and Sovereign Labs
	#[allow(dead_code)]
	// #[cfg_attr(feature = "native", derive(sov_modules_api::ModuleCallJsonSchema))]
	#[derive(ModuleInfo, Clone)]
	pub struct SovAptosVM<S: sov_modules_api::Spec, Da: DaSpec> {
		/// The address of the aptos module.
		#[address]
		pub(crate) address: S::Address,

		/// Mapping of `Version` to `AptosStateValue`.
		/// By default, `StateMap` uses the `Borsh` codec which is faster and more
		/// performant that `BcsCodec`.
		#[state]
		pub(crate) state_data: sov_modules_api::StateMap<Version, AptosStateValue>,

		/// Mapping from account address to account state.
		#[state]
		pub(crate) accounts: sov_modules_api::StateMap<Address, DbAccount, BcsCodec>,

		/// Mapping from code hash to owend resources. Used for lazy-loading code into a contract account.
		#[state]
		pub(crate) resources:
			sov_modules_api::StateMap<HexEncodedBytes, Vec<MoveResource>, BcsCodec>,

		#[state]
		pub(crate) modules: sov_modules_api::StateMap<HexEncodedBytes, Vec<MoveModuleBytecode>>,

		/// Chain configuration. This field is set in genesis.
		#[state]
		pub(crate) cfg: sov_modules_api::StateValue<AptosChainConfig, BcsCodec>,

		/// Block environment used by the aptos. This field is set in `begin_slot_hook`.
		#[state]
		pub(crate) block_env: sov_modules_api::StateValue<BlockEnv, BcsCodec>,

		/// Transactions that will be added to the current block.
		/// A valid transaction is added to the vec on every call message.
		#[state]
		pub(crate) pending_transactions: sov_modules_api::StateVec<PendingTransaction, BcsCodec>,

		/// Head of the chain. The new head is set in `end_slot_hook` but without the inclusion of the `state_root` field.
		/// The `state_root` is added in `begin_slot_hook` of the next block because its calculation occurs after the `end_slot_hook`.
		#[state]
		pub(crate) head: sov_modules_api::StateValue<Block, BcsCodec>,

		/// Used only by the RPC: This represents the head of the chain and is set in two distinct stages:
		/// 1. `end_slot_hook`: the pending head is populated with data from pending_transactions.
		/// 2. `finalize_hook` the `root_hash` is populated.
		/// Since this value is not authenticated, it can be modified in the `finalize_hook` with the correct `state_root`.
		#[state]
		pub(crate) pending_head: sov_modules_api::AccessoryStateValue<Block, BcsCodec>,

		/// Used only by the RPC: The vec is extended with `pending_head` in `finalize_hook`.
		#[state]
		pub(crate) blocks: sov_modules_api::AccessoryStateVec<Block, BcsCodec>,

		/// Used only by the RPC: block.signature => block_number mapping.
		#[state]
		pub(crate) block_hashes: sov_modules_api::AccessoryStateMap<Signature, u64, BcsCodec>,

		/// Used only by the RPC: List of processed transactions.
		#[state]
		pub(crate) transactions:
			sov_modules_api::AccessoryStateVec<TransactionSignedAndRecovered, BcsCodec>,

		/// Used only by the RPC: transaction_hash => transaction_index mapping.
		#[state]
		pub(crate) transaction_hashes:
			sov_modules_api::AccessoryStateMap<revm::primitives::B256, u64, BcsCodec>,

		/// Used only by the RPC: Receipts.
		#[state]
		pub(crate) receipts: sov_modules_api::AccessoryStateVec<Receipt, BcsCodec>,

		#[kernel_module]
		pub(crate) chain_state: sov_chain_state::ChainState<S, Da>,
	}

	impl<S: sov_modules_api::Spec, Da: DaSpec> sov_modules_api::Module for SovAptosVM<S, Da> {
		type Spec = S;

		type Config = AptosConfig;

		type CallMessage = super::call::CallMessage;

		type Event = Event;

		fn genesis(
			&self,
			config: &Self::Config,
			working_set: &mut WorkingSet<S>,
		) -> Result<(), Error> {
			Ok(self.init_module(config, working_set)?)
		}

		fn call(
			&self,
			msg: Self::CallMessage,
			context: &Context<Self::Spec>,
			working_set: &mut WorkingSet<S>,
		) -> Result<sov_modules_api::CallResponse, Error> {
			Ok(self.execute_call(msg.tx, context, working_set)?)
		}
	}

	impl<S: sov_modules_api::Spec, Da: DaSpec> SovAptosVM<S, Da> {
		pub(crate) fn get_db<'a>(&self, working_set: &'a mut WorkingSet<S>) -> SovAptosDb<'a, S> {
			SovAptosDb::new(self.state_data.clone(), working_set)
		}
	}
}
