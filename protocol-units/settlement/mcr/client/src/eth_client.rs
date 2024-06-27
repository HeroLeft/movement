use crate::send_eth_tx::InsufficentFunds;
use crate::send_eth_tx::SendTxErrorRule;
use crate::send_eth_tx::UnderPriced;
use crate::send_eth_tx::VerifyRule;
use crate::{CommitmentStream, McrSettlementClientOperations};
use alloy::pubsub::PubSubFrontend;
use alloy_network::Ethereum;
use alloy_network::EthereumSigner;
use alloy_primitives::Address;
use alloy_primitives::U256;
use alloy_provider::fillers::ChainIdFiller;
use alloy_provider::fillers::FillProvider;
use alloy_provider::fillers::GasFiller;
use alloy_provider::fillers::JoinFill;
use alloy_provider::fillers::NonceFiller;
use alloy_provider::fillers::SignerFiller;
use alloy_provider::Provider;
use alloy_provider::{ProviderBuilder, RootProvider};
use alloy_signer_wallet::LocalWallet;
use alloy_sol_types::sol;
use alloy_transport::BoxTransport;
use alloy_transport_ws::WsConnect;
use anyhow::Context;
use mcr_settlement_config::Config;
use movement_types::BlockCommitment;
use movement_types::{Commitment, Id};
use std::array::TryFromSliceError;
use thiserror::Error;
use tokio_stream::StreamExt;

#[derive(Error, Debug)]
pub enum McrEthConnectorError {
	#[error(
		"MCR Settlement Tx fail because gas estimation is to high. Estimated gas:{0} gas limit:{1}"
	)]
	GasLimitExceed(u128, u128),
	#[error("MCR Settlement Tx fail because account funds are insufficient. error:{0}")]
	InsufficientFunds(String),
	#[error("MCR Settlement Tx send fail because :{0}")]
	SendTxError(#[from] alloy_contract::Error),
	#[error("MCR Settlement Tx send fail during its execution :{0}")]
	RpcTxExecution(String),
	#[error("MCR Settlement BlockAccepted event notification error :{0}")]
	EventNotificationError(#[from] alloy_sol_types::Error),
	#[error("MCR Settlement BlockAccepted event notification stream close")]
	EventNotificationStreamClosed,
}

// Codegen from artifact.
sol!(
	#[allow(missing_docs)]
	#[sol(rpc)]
	MCR,
	"abis/MCRLegacy.json"
);

// When created, kill the pid when dropped.
// Use to kill Anvil process when Suzuka Node end.
// TODO should be removed by new config.
struct AnvilKillAtDrop {
	pid: u32,
}

impl Drop for AnvilKillAtDrop {
	fn drop(&mut self) {
		tracing::info!("Killing Anvil process pid:{}", self.pid);
		if let Err(err) = std::process::Command::new("kill").args(&[&self.pid.to_string()]).spawn()
		{
			tracing::info!("warn, an error occurs during Anvil process kill : {err}");
		}
	}
}

pub struct Client<P> {
	rpc_provider: P,
	ws_provider: RootProvider<PubSubFrontend>,
	signer_address: Address,
	contract_address: Address,
	send_tx_error_rules: Vec<Box<dyn VerifyRule>>,
	gas_limit: u64,
	num_tx_send_retries: u32,
	kill_anvil_process: Option<AnvilKillAtDrop>,
}

impl
	Client<
		FillProvider<
			JoinFill<
				JoinFill<
					JoinFill<JoinFill<alloy_provider::Identity, GasFiller>, NonceFiller>,
					ChainIdFiller,
				>,
				SignerFiller<EthereumSigner>,
			>,
			RootProvider<BoxTransport>,
			BoxTransport,
			Ethereum,
		>,
	>
{
	pub async fn build_with_config(config: Config) -> Result<Self, anyhow::Error> {
		let signer_private_key =
			config.signer_private_key.context("Signer private key is not set")?;
		let signer: LocalWallet = signer_private_key.parse()?;
		let signer_address = signer.address();

		tracing::info!("MCR Client sender address:{signer_address}");

		let contract_address = config.mcr_contract_address.parse()?;
		let rpc_url = config.rpc_url.context("Ethereum RPC URL is not set")?;
		let ws_url = config.ws_url.context("Ethereum WebSocket URL is not set")?;
		let rpc_provider = ProviderBuilder::new()
			.with_recommended_fillers()
			.signer(EthereumSigner::from(signer))
			.on_builtin(&rpc_url)
			.await?;

		let mut client = Client::build_with_provider(
			rpc_provider,
			ws_url,
			signer_address,
			contract_address,
			config.gas_limit,
			config.tx_send_retries,
		)
		.await?;
		if let Some(pid) = config.anvil_process_pid {
			client.kill_anvil_process = Some(AnvilKillAtDrop { pid })
		}
		Ok(client)
	}
}

impl<P> Client<P> {
	async fn build_with_provider<S>(
		rpc_provider: P,
		ws_url: S,
		signer_address: Address,
		contract_address: Address,
		gas_limit: u64,
		num_tx_send_retries: u32,
	) -> Result<Self, anyhow::Error>
	where
		P: Provider + Clone,
		S: Into<String>,
	{
		let ws = WsConnect::new(ws_url);

		let ws_provider = ProviderBuilder::new().on_ws(ws).await?;

		let rule1: Box<dyn VerifyRule> = Box::new(SendTxErrorRule::<UnderPriced>::new());
		let rule2: Box<dyn VerifyRule> = Box::new(SendTxErrorRule::<InsufficentFunds>::new());
		let send_tx_error_rules = vec![rule1, rule2];

		Ok(Client {
			rpc_provider,
			ws_provider,
			signer_address,
			contract_address,
			send_tx_error_rules,
			gas_limit,
			num_tx_send_retries,
			kill_anvil_process: None,
		})
	}
}

#[async_trait::async_trait]
impl<P> McrSettlementClientOperations for Client<P>
where
	P: Provider + Clone,
{
	async fn post_block_commitment(
		&self,
		block_commitment: BlockCommitment,
	) -> Result<(), anyhow::Error> {
		let contract = MCR::new(self.contract_address, &self.rpc_provider);

		let eth_block_commitment = MCR::BlockCommitment {
			// currently, to simplify the api, we'll say 0 is uncommitted all other numbers are legitimate heights
			height: U256::from(block_commitment.height),
			commitment: alloy_primitives::FixedBytes(block_commitment.commitment.0),
			blockId: alloy_primitives::FixedBytes(block_commitment.block_id.0),
		};

		let call_builder = contract.submitBlockCommitment(eth_block_commitment);

		crate::send_eth_tx::send_tx(
			call_builder,
			&self.send_tx_error_rules,
			self.num_tx_send_retries,
			self.gas_limit as u128,
		)
		.await
	}

	async fn post_block_commitment_batch(
		&self,
		block_commitments: Vec<BlockCommitment>,
	) -> Result<(), anyhow::Error> {
		let contract = MCR::new(self.contract_address, &self.rpc_provider);

		let eth_block_commitment: Vec<_> = block_commitments
			.into_iter()
			.map(|block_commitment| {
				tracing::info!(
					"settlement client post_block_commitment_batch height:{}",
					block_commitment.height
				);
				Ok(MCR::BlockCommitment {
					// currently, to simplify the api, we'll say 0 is uncommitted all other numbers are legitimate heights
					height: U256::from(block_commitment.height),
					commitment: alloy_primitives::FixedBytes(block_commitment.commitment.0),
					blockId: alloy_primitives::FixedBytes(block_commitment.block_id.0),
				})
			})
			.collect::<Result<Vec<_>, TryFromSliceError>>()?;

		let call_builder = contract.submitBatchBlockCommitment(eth_block_commitment);

		crate::send_eth_tx::send_tx(
			call_builder,
			&self.send_tx_error_rules,
			self.num_tx_send_retries,
			self.gas_limit as u128,
		)
		.await
	}

	async fn stream_block_commitments(&self) -> Result<CommitmentStream, anyhow::Error> {
		//register to contract BlockCommitmentSubmitted event

		let contract = MCR::new(self.contract_address, &self.ws_provider);
		let event_filter = contract.BlockAccepted_filter().watch().await?;

		let stream = event_filter.into_stream().map(|event| {
			event
				.and_then(|(commitment, _)| {
					let height = commitment.height.try_into().map_err(
						|err: alloy::primitives::ruint::FromUintError<u64>| {
							alloy_sol_types::Error::Other(err.to_string().into())
						},
					)?;
					tracing::info!(
						"settlement client stream_block_commitments received for height:{height}",
					);
					Ok(BlockCommitment {
						height,
						block_id: Id(commitment.blockHash.0),
						commitment: Commitment(commitment.stateCommitment.0),
					})
				})
				.map_err(|err| McrEthConnectorError::EventNotificationError(err).into())
		});
		Ok(Box::pin(stream) as CommitmentStream)
	}

	async fn get_commitment_at_height(
		&self,
		height: u64,
	) -> Result<Option<BlockCommitment>, anyhow::Error> {
		let contract = MCR::new(self.contract_address, &self.ws_provider);
		let MCR::getValidatorCommitmentAtBlockHeightReturn { _0: commitment } = contract
			.getValidatorCommitmentAtBlockHeight(U256::from(height), self.signer_address)
			.call()
			.await?;
		let return_height: u64 = commitment.height.try_into()?;
		// Commitment with height 0 mean not found
		Ok((return_height != 0).then_some(BlockCommitment {
			height: commitment.height.try_into()?,
			block_id: Id(commitment.blockId.into()),
			commitment: Commitment(commitment.commitment.into()),
		}))
	}

	async fn get_max_tolerable_block_height(&self) -> Result<u64, anyhow::Error> {
		let contract = MCR::new(self.contract_address, &self.ws_provider);
		let MCR::getMaxTolerableBlockHeightReturn { _0: block_height } =
			contract.getMaxTolerableBlockHeight().call().await?;
		let return_height: u64 = block_height.try_into()?;
		Ok(return_height)
	}
}

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod tests {
	use super::*;
	use alloy_primitives::Bytes;
	use alloy_provider::ProviderBuilder;
	use alloy_rpc_types::TransactionRequest;
	use alloy_signer_wallet::LocalWallet;
	use alloy_transport::Transport;
	use movement_types::Commitment;

	// Define 2 validators (signer1 and signer2) with each a little more than 50% of stake.
	// After genesis ceremony, 2 validator send the commitment for height 1.
	// Validator2 send a commitment for height 2 to trigger next epoch and fire event.
	// Wait the commitment accepted event.
	#[tokio::test]
	async fn test_send_commitment() -> Result<(), anyhow::Error> {
		//Activate to debug the test.
		tracing_subscriber::fmt()
			.with_env_filter(
				tracing_subscriber::EnvFilter::try_from_default_env()
					.unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
			)
			.init();

		//load local env.
		let dot_movement = dot_movement::DotMovement::try_from_env()?;
		let suzuka_config = dot_movement.try_get_config_from_json::<suzuka_config::Config>()?;

		let rpc_url = suzuka_config.mcr.rpc_url.ok_or_else(|| {
			anyhow::anyhow!(format!("Anvil rpc Url not defined in config. Aborting."))
		})?;

		let ws_url = suzuka_config.mcr.ws_url.ok_or_else(|| {
			anyhow::anyhow!(format!("Anvil rpc Url not defined in config. Aborting."))
		})?;

		let anvil_config = suzuka_config
			.mcr
			.test_local
			.ok_or_else(|| anyhow::anyhow!("Test local anvil configuration not intialized?"))?;

		println!("test anvil_address");

		let mcr_address: Address = suzuka_config.mcr.mcr_contract_address.trim().parse()?;

		//Do SC ceremony init stake calls.
		do_genesis_ceremonial(mcr_address, &anvil_config.anvil_keys, &rpc_url).await?;

		let config = Config {
			mcr_contract_address: mcr_address.to_string(),
			rpc_url: Some(rpc_url),
			ws_url: Some(ws_url),
			..Default::default()
		};

		//Build client 1 and send first commitment.
		let config1 = Config {
			signer_private_key: Some(anvil_config.anvil_keys[0].private_key.clone()),
			..config.clone()
		};
		let client1 = Client::build_with_config(config1).await.unwrap();

		let mut client1_stream = client1.stream_block_commitments().await.unwrap();

		//client post a new commitment
		let commitment =
			BlockCommitment { height: 1, block_id: Id([2; 32]), commitment: Commitment([3; 32]) };

		let res = client1.post_block_commitment(commitment.clone()).await;
		assert!(res.is_ok());

		//no notification quorum is not reach
		let res =
			tokio::time::timeout(tokio::time::Duration::from_secs(5), client1_stream.next()).await;
		assert!(res.is_err());

		//Build client 2 and send the second commitment.
		let config2 = Config {
			signer_private_key: Some(anvil_config.anvil_keys[1].private_key.clone()),
			..config.clone()
		};
		let client2 = Client::build_with_config(config2).await.unwrap();

		let mut client2_stream = client2.stream_block_commitments().await.unwrap();

		//client post a new commitment
		let res = client2.post_block_commitment(commitment).await;
		assert!(res.is_ok());

		// now we move to block 2 and make some commitment just to trigger the epochRollover
		let commitment2 =
			BlockCommitment { height: 2, block_id: Id([4; 32]), commitment: Commitment([5; 32]) };

		let res = client2.post_block_commitment(commitment2.clone()).await;
		assert!(res.is_ok());

		//validate that the accept commitment stream get the event.
		let event =
			tokio::time::timeout(tokio::time::Duration::from_secs(5), client1_stream.next())
				.await
				.unwrap()
				.unwrap()
				.unwrap();
		assert_eq!(event.commitment.0[0], 3);
		assert_eq!(event.block_id.0[0], 2);
		let event =
			tokio::time::timeout(tokio::time::Duration::from_secs(5), client2_stream.next())
				.await
				.unwrap()
				.unwrap()
				.unwrap();
		assert_eq!(event.commitment.0[0], 3);
		assert_eq!(event.block_id.0[0], 2);

		//test post batch commitment
		// post the complementary batch on height 2 and one on height 3
		let commitment3 =
			BlockCommitment { height: 3, block_id: Id([6; 32]), commitment: Commitment([7; 32]) };
		let res = client1.post_block_commitment_batch(vec![commitment2, commitment3]).await;
		assert!(res.is_ok());
		//validate that the accept commitment stream get the event.
		let event =
			tokio::time::timeout(tokio::time::Duration::from_secs(5), client1_stream.next())
				.await
				.unwrap()
				.unwrap()
				.unwrap();
		assert_eq!(event.commitment.0[0], 5);
		assert_eq!(event.block_id.0[0], 4);
		let event =
			tokio::time::timeout(tokio::time::Duration::from_secs(5), client2_stream.next())
				.await
				.unwrap()
				.unwrap()
				.unwrap();
		assert_eq!(event.commitment.0[0], 5);
		assert_eq!(event.block_id.0[0], 4);

		//test get_commitment_at_height
		let commitment = client1.get_commitment_at_height(1).await?;
		assert!(commitment.is_some());
		let commitment = commitment.unwrap();
		assert_eq!(commitment.commitment.0[0], 3);
		assert_eq!(commitment.block_id.0[0], 2);
		let commitment = client1.get_commitment_at_height(10).await?;
		assert_eq!(commitment, None);

		Ok(())
	}

	// Do the Genesis ceremony in Rust because if node by forge script,
	// it's never done from Rust call.
	async fn do_genesis_ceremonial(
		mcr_address: Address,
		anvil_address: &[mcr_settlement_config::anvil::AnvilAddressEntry],
		rpc_url: &str,
	) -> Result<(), anyhow::Error> {
		//Define Signer. Signer1 is the MCRSettelement client
		let signer1: LocalWallet = anvil_address[0].private_key.parse()?;
		let signer1_addr: Address = anvil_address[0].address.parse()?;
		let signer1_rpc_provider = ProviderBuilder::new()
			.with_recommended_fillers()
			.signer(EthereumSigner::from(signer1))
			.on_http(rpc_url.parse()?);
		let signer1_contract = MCR::new(mcr_address, &signer1_rpc_provider);

		stake_genesis(
			&signer1_rpc_provider,
			&signer1_contract,
			mcr_address,
			signer1_addr,
			55_000_000_000_000_000_000,
		)
		.await?;

		let signer2: LocalWallet = anvil_address[1].private_key.parse()?;
		let signer2_addr: Address = anvil_address[1].address.parse()?;
		let signer2_rpc_provider = ProviderBuilder::new()
			.with_recommended_fillers()
			.signer(EthereumSigner::from(signer2))
			.on_http(rpc_url.parse()?);
		let signer2_contract = MCR::new(mcr_address, &signer2_rpc_provider);

		//init staking
		// Build a transaction to set the values.
		stake_genesis(
			&signer2_rpc_provider,
			&signer2_contract,
			mcr_address,
			signer2_addr,
			54_000_000_000_000_000_000,
		)
		.await?;

		let MCR::hasGenesisCeremonyEndedReturn { _0: has_genesis_ceremony_ended } =
			signer2_contract.hasGenesisCeremonyEnded().call().await?;
		let ceremony: bool = has_genesis_ceremony_ended.try_into().unwrap();
		assert!(ceremony);
		Ok(())
	}

	async fn stake_genesis<P: Provider<T, Ethereum>, T: Transport + Clone>(
		provider: &P,
		contract: &MCR::MCRInstance<T, &P, Ethereum>,
		contract_address: Address,
		signer: Address,
		amount: u128,
	) -> Result<(), anyhow::Error> {
		let stake_genesis_call = contract.stakeGenesis();
		let calldata = stake_genesis_call.calldata().to_owned();
		send_tx(provider, calldata, contract_address, signer, amount).await
	}

	async fn send_tx<P: Provider<T, Ethereum>, T: Transport + Clone>(
		provider: &P,
		call_data: Bytes,
		contract_address: Address,
		signer: Address,
		amount: u128,
	) -> Result<(), anyhow::Error> {
		let eip1559_fees = provider.estimate_eip1559_fees(None).await?;
		let tx = TransactionRequest::default()
			.from(signer)
			.to(contract_address)
			.value(U256::from(amount))
			.input(call_data.into())
			.max_fee_per_gas(eip1559_fees.max_fee_per_gas)
			.max_priority_fee_per_gas(eip1559_fees.max_priority_fee_per_gas);

		provider.send_transaction(tx).await?.get_receipt().await?;
		Ok(())
	}
}
