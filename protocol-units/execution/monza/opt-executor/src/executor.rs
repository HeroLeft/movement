use aptos_db::AptosDB;
use aptos_executor_types::BlockExecutorTrait;
use aptos_mempool::{
	core_mempool::{CoreMempool, TimelineState},
	MempoolClientRequest, MempoolClientSender,
};
use aptos_storage_interface::DbReaderWriter;
use aptos_types::{
	block_executor::{config::BlockExecutorConfigFromOnchain, partitioner::ExecutableBlock}, chain_id::ChainId, transaction::{
		ChangeSet, SignedTransaction, Transaction, WriteSetPayload
	}, validator_signer::ValidatorSigner
};
use aptos_vm::AptosVM;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use aptos_config::config::NodeConfig;
use aptos_executor::{
	block_executor::BlockExecutor,
	db_bootstrapper::{generate_waypoint, maybe_bootstrap},
};
use aptos_api::{get_api_service, runtime::{get_apis, Apis}, Context};
use futures::channel::mpsc as futures_mpsc;
use poem::{listener::TcpListener, Route, Server};
use aptos_sdk::types::mempool_status::{MempoolStatus, MempoolStatusCode};
use aptos_mempool::SubmissionStatus;
use futures::StreamExt;
use aptos_vm_genesis::GENESIS_KEYPAIR;
use aptos_types::{
    aggregate_signature::AggregateSignature,
    block_info::BlockInfo,
    ledger_info::{LedgerInfo, LedgerInfoWithSignatures},
    transaction::Version
};
use aptos_crypto::HashValue;
use aptos_vm_genesis::{TestValidator, Validator, encode_genesis_change_set, GenesisConfiguration, default_gas_schedule};
use aptos_sdk::types::on_chain_config::{
	OnChainConsensusConfig, OnChainExecutionConfig
};
// use aptos_types::test_helpers::transaction_test_helpers::block;

/// The `Executor` is responsible for executing blocks and managing the state of the execution
/// against the `AptosVM`.
#[derive(Clone)]
pub struct Executor {
	/// The executing type.
	pub block_executor: Arc<RwLock<BlockExecutor<AptosVM>>>,
	/// The access to db.
	pub db: Arc<RwLock<DbReaderWriter>>,
	/// The signer of the executor's transactions.
	pub signer: ValidatorSigner,
	/// The core mempool (used for the api to query the mempool).
	pub core_mempool: Arc<RwLock<CoreMempool>>,
	/// The sender for the mempool client.
	pub mempool_client_sender: MempoolClientSender,
	/// The receiver for the mempool client.
	pub mempool_client_receiver: Arc<RwLock<futures_mpsc::Receiver<MempoolClientRequest>>>,
	/// The configuration of the node.
	pub node_config: NodeConfig,
	/// The chain id of the node.
	pub chain_id: ChainId,
	/// Context 
	pub context : Arc<Context>,
}


impl Executor {

	const DB_PATH_ENV_VAR: &'static str = "DB_DIR";

	/// Create a new `Executor` instance.
	pub fn new(
		db_dir : PathBuf,
		block_executor: BlockExecutor<AptosVM>,
		signer: ValidatorSigner,
		mempool_client_sender: MempoolClientSender,
		mempool_client_receiver: futures_mpsc::Receiver<MempoolClientRequest>,
		node_config: NodeConfig,
		chain_id: ChainId,
	) -> Self {

		let (_aptos_db, reader_writer) = DbReaderWriter::wrap(AptosDB::new_for_test(&db_dir));
		let core_mempool = Arc::new(RwLock::new(CoreMempool::new(&node_config)));
		let reader = reader_writer.reader.clone();
		Self {
			block_executor: Arc::new(RwLock::new(block_executor)),
			db: Arc::new(RwLock::new(reader_writer)),
			signer,
			core_mempool,
			mempool_client_sender : mempool_client_sender.clone(),
			node_config : node_config.clone(),
			mempool_client_receiver : Arc::new(RwLock::new(mempool_client_receiver)),
			chain_id : chain_id.clone(),
			context : Arc::new(Context::new(
				chain_id,
				reader,
				mempool_client_sender,
				node_config ,
				None
			))
		}
	}

	pub fn genesis_change_set_and_validators(
		chain_id: ChainId, 
		count : Option<usize>
	) -> (ChangeSet, Vec<TestValidator>) {

		let framework = aptos_cached_packages::head_release_bundle();
		let test_validators = TestValidator::new_test_set(count, Some(100_000_000));
		let validators_: Vec<Validator> = test_validators.iter().map(|t| t.data.clone()).collect();
		let validators = &validators_;

		let genesis = encode_genesis_change_set(
			&GENESIS_KEYPAIR.1,
			validators,
			framework,
			chain_id,
			// todo: get this config from somewhere
			&GenesisConfiguration {
				allow_new_validators: true,
				epoch_duration_secs: 3600,
				is_test: true,
				min_stake: 0,
				min_voting_threshold: 0,
				// 1M APTOS coins (with 8 decimals).
				max_stake: 100_000_000_000_000,
				recurring_lockup_duration_secs: 7200,
				required_proposer_stake: 0,
				rewards_apy_percentage: 10,
				voting_duration_secs: 3600,
				voting_power_increase_limit: 50,
				employee_vesting_start: 1663456089,
				employee_vesting_period_duration: 5 * 60, // 5 minutes
				initial_features_override: None,
				randomness_config_override: None,
				jwk_consensus_config_override: None,
			},
			&OnChainConsensusConfig::default_for_genesis(),
			&OnChainExecutionConfig::default_for_genesis(),
			&default_gas_schedule(),
		);
		(genesis, test_validators)
	
	}

	pub fn bootstrap_empty_db(
		db_dir : PathBuf,
		chain_id: ChainId
	) -> Result<(
		DbReaderWriter,
		ValidatorSigner
	), anyhow::Error> {
		
		let (genesis, validators) = Self::genesis_change_set_and_validators(chain_id, Some(1));
		let genesis_txn = Transaction::GenesisTransaction(WriteSetPayload::Direct(genesis));
		let db_rw = DbReaderWriter::new(AptosDB::new_for_test(&db_dir));
		
		assert!(db_rw.reader.get_latest_ledger_info_option()?.is_none());

		// Bootstrap empty DB.
		let waypoint =
			generate_waypoint::<AptosVM>(&db_rw, &genesis_txn)?;
		maybe_bootstrap::<AptosVM>(&db_rw, &genesis_txn, waypoint)?.ok_or(
			anyhow::anyhow!("Failed to bootstrap DB"),
		)?;
		assert!(db_rw.reader.get_latest_ledger_info_option()?.is_some());

		let validator_signer = ValidatorSigner::new(
			validators[0].data.owner_address,
			validators[0].consensus_key.clone(),
		);

		Ok((db_rw, validator_signer))
	}

	pub fn bootstrap(
		db_dir : PathBuf,
		mempool_client_sender: MempoolClientSender,
		mempool_client_receiver: futures_mpsc::Receiver<MempoolClientRequest>,
		node_config: NodeConfig,
		chain_id: ChainId,
	) -> Result<Self, anyhow::Error> {

		// todo: update this to something more stable
		// keypair will be GENESIS_KEYPAIR.
		// For now, let's write this to a well known location
		let private_key_bytes = GENESIS_KEYPAIR.0.to_bytes(); // get the private key bytes
		let public_key_bytes = GENESIS_KEYPAIR.1.to_bytes(); // get the public key bytes
		let private_key_hex = hex::encode(private_key_bytes); // convert bytes to hex string
		let public_key_hex = hex::encode(public_key_bytes); // convert bytes to hex string
		let chain_id_str = chain_id.to_string();
		let base_dir = PathBuf::from("./.etc/monza");
		let private_key_path = base_dir.join("private_key");
		let public_key_path = base_dir.join("public_key");
		let chain_id_path = base_dir.join("chain_id");
		// mkdir -p
		std::fs::create_dir_all(&base_dir)?;
		std::fs::write(private_key_path, private_key_hex)?;
		std::fs::write(public_key_path, public_key_hex)?;
		std::fs::write(chain_id_path, chain_id_str)?;

		let (db_rw, signer) = Self::bootstrap_empty_db( db_dir, chain_id)?;
		let reader = db_rw.reader.clone();
		let core_mempool = Arc::new(RwLock::new(CoreMempool::new(&node_config)));

		Ok(Self {
			block_executor: Arc::new(RwLock::new(BlockExecutor::new(db_rw.clone()))),
			db: Arc::new(RwLock::new(db_rw)),
			signer,
			core_mempool,
			mempool_client_sender : mempool_client_sender.clone(),
			mempool_client_receiver : Arc::new(RwLock::new(mempool_client_receiver)),
			node_config : node_config.clone(),
			chain_id,
			context : Arc::new(Context::new(
				chain_id,
				reader,
				mempool_client_sender,
				node_config,
				None
			))
		})

	}

	pub fn try_from_env() -> Result<Self, anyhow::Error> {

		// read the db dir from env or use a tempfile
		let db_dir = match std::env::var(Self::DB_PATH_ENV_VAR) {
			Ok(dir) => PathBuf::from(dir),
			Err(_) => {
				let temp_dir = tempfile::tempdir()?;
				temp_dir.path().to_path_buf()
			}
		};

		// use the default signer, block executor, and mempool
		let (mempool_client_sender, mempool_client_receiver) = futures_mpsc::channel::<MempoolClientRequest>(10);
		let node_config = NodeConfig::default();
		let chain_id = ChainId::test();

		Self::bootstrap(
			db_dir,
			mempool_client_sender,
			mempool_client_receiver,
			node_config,
			chain_id,
		)

	}

	pub fn get_ledger_info_with_sigs(
		&self,
		block_id: HashValue,
		root_hash: HashValue,
		version: Version,
	) -> LedgerInfoWithSignatures {
		let block_info = BlockInfo::new(
			1,      
			0,        
			block_id,
			root_hash, version, 
			0,    /* timestamp_usecs, doesn't matter */
			None, 
		);
		let ledger_info = LedgerInfo::new(
			block_info,
			HashValue::zero(), /* consensus_data_hash, doesn't matter */
		);
		LedgerInfoWithSignatures::new(
			ledger_info,
			AggregateSignature::empty(), /* signatures */
		)
	}

	/// Execute a block which gets committed to the state.
	/// `ExecutorState` must be set to `Commit` before calling this method.
	pub async fn execute_block(
		&self,
		block: ExecutableBlock,
	) -> Result<(), anyhow::Error> {

		let block_id = block.block_id.clone();
		let parent_block_id = {
			let block_executor = self.block_executor.read().await;
			block_executor.committed_block_id()
		};

		let state_compute = {
			let block_executor = self.block_executor.write().await;
			block_executor.execute_block(block, parent_block_id, BlockExecutorConfigFromOnchain::new_no_block_limit())?
		};

		let latest_version = {
			let reader = self.db.read().await.reader.clone();
			reader.get_latest_version()?
		};

		{
			let ledger_info_with_sigs = self.get_ledger_info_with_sigs(block_id, state_compute.root_hash(), state_compute.version());
			let block_executor = self.block_executor.write().await;
			block_executor.commit_blocks(
				vec![block_id],
				ledger_info_with_sigs,
			)?;
		} 

		{
			let reader = self.db.read().await.reader.clone();
			let proof = reader.get_state_proof(
				state_compute.version(),
			)?;
		}

		Ok(())
	}

	pub async fn try_get_context(&self) -> Result<Arc<Context>, anyhow::Error> {
		Ok(self.context.clone())
	}

	pub async fn try_get_apis(&self) -> Result<Apis, anyhow::Error> {
		let context = self.try_get_context().await?;
		Ok(get_apis(context))
	}

	pub async fn run_service(&self) -> Result<(), anyhow::Error> {

		let context = self.try_get_context().await?;
		let api_service = get_api_service(context).server("http://127.0.0.1:3000");

		/*let basic_api = BasicApi {
			concurrent_requests_semaphore : None,

		};*/

		let ui = api_service.swagger_ui();
	
		// todo: add cors
		let app = Route::new()
			.nest("/v1", api_service)
			.nest("/spec", ui);
		Server::new(TcpListener::bind("127.0.0.1:3000"))
			.run(app)
			.await.map_err(
				|e| anyhow::anyhow!("Server error: {:?}", e)
			)?;

		Ok(())
	}

	pub async fn get_transaction_sequence_number(
		&self,
		_transaction: &SignedTransaction
	) -> Result<u64, anyhow::Error> {
		// just use the ms since epoch for now
		let ms = chrono::Utc::now().timestamp_millis();
		Ok(ms as u64)	
	}

	/// Ticks the transaction reader.
	pub async fn tick_transaction_reader(
		&self,
		transaction_channel : async_channel::Sender<SignedTransaction>
	) ->  Result<(), anyhow::Error> {

		let mut mempool_client_receiver = self.mempool_client_receiver.write().await;
		for _ in 0..256 {

			// use select to safely timeout a request for a transaction without risking dropping the transaction
			// !warn: this may still be unsafe
			tokio::select! {
				_ = tokio::time::sleep(tokio::time::Duration::from_millis(5)) => { () },
				request = mempool_client_receiver.next() => {
					match request {
						Some(request) => {
							match request {
								MempoolClientRequest::SubmitTransaction(transaction, callback) => {
									// add to the mempool
									{
								
										let mut core_mempool = self.core_mempool.write().await;
										
										let status = core_mempool.add_txn(
											transaction.clone(),
											0,
											transaction.sequence_number(),
											TimelineState::NonQualified,
											true
										);

										match status.code {
											MempoolStatusCode::Accepted => {
											
											},
											_ => {
												anyhow::bail!("Transaction not accepted: {:?}", status);
											}
										}

										// send along to the receiver
										transaction_channel.send(transaction).await.map_err(
											|e| anyhow::anyhow!("Error sending transaction: {:?}", e)
										)?;

									};

									// report status
									let ms = MempoolStatus::new(MempoolStatusCode::Accepted);
									let status: SubmissionStatus = (ms, None);
									callback.send(Ok(status)).map_err(
										|e| anyhow::anyhow!("Error sending callback: {:?}", e)
									)?;

								},
								MempoolClientRequest::GetTransactionByHash(hash, sender) => {
									let mempool = self.core_mempool.read().await;
									let mempool_result = mempool.get_by_hash(hash);
									sender.send(mempool_result).map_err(
										|e| anyhow::anyhow!("Error sending callback: {:?}", e)
									)?;
								},
							}
						},
						None => {
							break;
						}
					}
				}
			}
			
		}

		Ok(())

	}

	pub async fn tick_mempool_pipe(
		&self,
		_transaction_channel : async_channel::Sender<SignedTransaction>
	) -> Result<(), anyhow::Error> {

		// todo: remove this old implementation
		
		Ok(())
	}

	/// Pipes a batch of transactions from the mempool to the transaction channel.
	/// todo: it may be wise to move the batching logic up a level to the consuming structs.
	pub async fn tick_transaction_pipe(
		&self, 
		transaction_channel : async_channel::Sender<SignedTransaction>
	) -> Result<(), anyhow::Error> {
	
		self.tick_transaction_reader(transaction_channel.clone()).await?;

		self.tick_mempool_pipe(transaction_channel).await?;

		Ok(())
	}

}

#[cfg(test)]
mod tests {

	use std::collections::BTreeSet;

	use super::*;
	use aptos_crypto::{
		ed25519::{Ed25519PrivateKey, Ed25519Signature}, HashValue, PrivateKey, Uniform
	};
	use aptos_types::{
		account_address::AccountAddress, block_executor::partitioner::ExecutableTransactions, block_metadata::BlockMetadata, chain_id::{self, ChainId}, transaction::{
			signature_verified_transaction::SignatureVerifiedTransaction, RawTransaction, Script,
			SignedTransaction, Transaction, TransactionPayload
		}
	};
	use aptos_api::{
		accept_type::AcceptType,
		transactions::SubmitTransactionPost
	};
	use futures::SinkExt;
	use futures::channel::oneshot;
	use aptos_sdk::{
        transaction_builder::TransactionFactory,
        types::{AccountKey, LocalAccount},
    };
	use rand::SeedableRng;
	use aptos_storage_interface::state_view::DbStateViewAtVersion;
	use aptos_types::account_config::aptos_test_root_address;
	use aptos_types::state_store::account_with_state_view::AsAccountWithStateView;
	use aptos_types::account_view::AccountView;
	use aptos_types::transaction::signature_verified_transaction::into_signature_verified_block;

	fn create_signed_transaction(gas_unit_price: u64, chain_id : ChainId) -> SignedTransaction {
		let private_key = Ed25519PrivateKey::generate_for_testing();
		let public_key = private_key.public_key();
		let transaction_payload = TransactionPayload::Script(Script::new(vec![0], vec![], vec![]));
		let raw_transaction = RawTransaction::new(
			AccountAddress::random(),
			0,
			transaction_payload,
			0,
			gas_unit_price,
			0,
			chain_id // This is the value used in aptos testing code.
		);
		SignedTransaction::new(raw_transaction, public_key, Ed25519Signature::dummy_signature())
	}


	#[tokio::test]
	async fn test_execute_block() -> Result<(), anyhow::Error> {
		let executor = Executor::try_from_env()?;
		let block_id = HashValue::random();
		let tx = SignatureVerifiedTransaction::Valid(Transaction::UserTransaction(
			create_signed_transaction(0, executor.chain_id.clone()),
		));
		let txs = ExecutableTransactions::Unsharded(vec![tx]);
		let block = ExecutableBlock::new(block_id.clone(), txs);
		executor.execute_block(block).await?;
		Ok(())
	}

	// https://github.com/movementlabsxyz/aptos-core/blob/ea91067b81f9673547417bff9c70d5a2fe1b0e7b/execution/executor-test-helpers/src/integration_test_impl.rs#L535
	#[tokio::test]
	async fn test_execute_block_state_db() -> Result<(), anyhow::Error> {
		// Initialize a root account using a predefined keypair and the test root address.
		let root_account = LocalAccount::new(
			aptos_test_root_address(),
			AccountKey::from_private_key(aptos_vm_genesis::GENESIS_KEYPAIR.0.clone()),
			0,
		);

		// Seed for random number generator, used here to generate predictable results in a test environment.
		let seed = [3u8; 32];
		let mut rng = ::rand::rngs::StdRng::from_seed(seed);

		// Create an executor instance from the environment configuration.
		let executor = Executor::try_from_env()?;
		// Create a transaction factory with the chain ID of the executor, used for creating transactions.
		let tx_factory = TransactionFactory::new(executor.chain_id.clone());

		// Loop to simulate the execution of multiple blocks.
		for _ in 0..10 {
			// Generate a random block ID.
			let block_id = HashValue::random();
			// Clone the signer from the executor for signing the metadata.
			// let signer = executor.signer.clone();
			// Get the current time in microseconds for the block timestamp.
			// let current_time_micros = chrono::Utc::now().timestamp_micros() as u64;

			// Create a block metadata transaction.
			/*let block_metadata = Transaction::BlockMetadata(BlockMetadata::new(
				block_id,
				1,
				0,
				signer.author(),
				vec![0],
				vec![],
				current_time_micros,
			));*/

			// Create a state checkpoint transaction using the block ID.
			// let state_checkpoint_tx = Transaction::StateCheckpoint(block_id.clone());
			// Generate a new account for transaction tests.
			let new_account = LocalAccount::generate(&mut rng);
			let new_account_address = new_account.address();

			// Create a user account creation transaction.
			let user_account_creation_tx = root_account
				.sign_with_transaction_builder(tx_factory.create_user_account(new_account.public_key()));

			// Create a mint transaction to provide the new account with some initial balance.
			let mint_tx = root_account
				.sign_with_transaction_builder(tx_factory.mint(new_account.address(), 2000));
			// Store the hash of the committed transaction for later verification.
			let mint_tx_hash = mint_tx.clone().committed_hash();

			// Group all transactions into a single unsharded block for execution.
			let transactions = ExecutableTransactions::Unsharded(
				into_signature_verified_block(vec![
					// block_metadata,
					Transaction::UserTransaction(user_account_creation_tx),
					Transaction::UserTransaction(mint_tx),
					// state_checkpoint_tx,
				])
			);

			// Create and execute the block.
			let block = ExecutableBlock::new(block_id.clone(), transactions);
			executor.execute_block(block).await?;

			// Access the database reader to verify state after execution.
			let db_reader = executor.db.read().await.reader.clone();
			// Get the latest version of the blockchain state from the database.
			let latest_version = db_reader.get_latest_version()?;
			// Verify the transaction by its hash to ensure it was committed.
			let transaction_result = db_reader.get_transaction_by_hash(
				mint_tx_hash,
				latest_version,
				false,
			)?;
			assert!(transaction_result.is_some());

			// Create a state view at the latest version to inspect account states.
			let state_view = db_reader.state_view_at_version(Some(latest_version))?;
			// Access the state view of the new account to verify its state and existence.
			let account_state_view = state_view.as_account_with_state_view(&new_account_address);
			let queried_account_address = account_state_view.get_account_address()?;
			assert!(queried_account_address.is_some());
			let account_resource = account_state_view.get_account_resource()?;
			assert!(account_resource.is_some());
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_execute_block_state_get_api() -> Result<(), anyhow::Error> {

		// Initialize a root account using a predefined keypair and the test root address.
		let root_account = LocalAccount::new(
			aptos_test_root_address(),
			AccountKey::from_private_key(aptos_vm_genesis::GENESIS_KEYPAIR.0.clone()),
			0,
		);
	
		// Seed for random number generator, used here to generate predictable results in a test environment.
		let seed = [3u8; 32];
		let mut rng = ::rand::rngs::StdRng::from_seed(seed);
	
		// Create an executor instance from the environment configuration.
		let executor = Executor::try_from_env()?;
	
		// Create a transaction factory with the chain ID of the executor.
		let tx_factory = TransactionFactory::new(executor.chain_id.clone());
	
		// Simulate the execution of multiple blocks.
		for _ in 0..10 {  // For example, create and execute 3 blocks.
			let block_id = HashValue::random();  // Generate a random block ID for each block.
	
			// Generate new accounts and create transactions for each block.
			let mut transactions = Vec::new();
			let mut transaction_hashes = Vec::new();
			for _ in 0..2 {  // Each block will contain 2 transactions.
				let new_account = LocalAccount::generate(&mut rng);
				let user_account_creation_tx = root_account
					.sign_with_transaction_builder(tx_factory.create_user_account(new_account.public_key()));
				let tx_hash = user_account_creation_tx.clone().committed_hash();
				transaction_hashes.push(tx_hash);
				transactions.push(Transaction::UserTransaction(user_account_creation_tx));
			}
	
			// Group all transactions into an unsharded block for execution.
			let executable_transactions = ExecutableTransactions::Unsharded(
				transactions.into_iter().map(SignatureVerifiedTransaction::Valid).collect(),
			);
			let block = ExecutableBlock::new(block_id.clone(), executable_transactions);
			executor.execute_block(block).await?;
	
			// Retrieve the executor's API interface and fetch the transaction by each hash.
			let apis = executor.try_get_apis().await?;
			for hash in transaction_hashes {
				let _ = apis.transactions.get_transaction_by_hash_inner(
					&AcceptType::Bcs,
					hash.into()
				).await?;
			}
		}
	
		Ok(())
	}
	

	#[tokio::test]
	async fn test_pipe_mempool() -> Result<(), anyhow::Error> {

		// header
		let mut executor = Executor::try_from_env()?;
		let user_transaction = create_signed_transaction(0);

		// send transaction to mempool
		let (req_sender, callback) = oneshot::channel();
		executor.mempool_client_sender.send(MempoolClientRequest::SubmitTransaction(
			user_transaction.clone(),
			req_sender
		)).await?;

		// tick the transaction pipe
		let (tx, rx) = async_channel::unbounded();
		executor.tick_transaction_pipe(tx).await?;

		// receive the callback
		callback.await??;
		
		// receive the transaction
		let received_transaction = rx.recv().await?;
		assert_eq!(received_transaction, user_transaction);

		Ok(())
	}

	#[tokio::test]
	async fn test_pipe_mempool_while_server_running() -> Result<(), anyhow::Error> {
		
		let mut executor = Executor::try_from_env()?;
		let server_executor = executor.clone();

		let handle = tokio::spawn(async move {
			server_executor.run_service().await?;
			Ok(()) as Result<(), anyhow::Error> 
		});

		let user_transaction = create_signed_transaction(0);

		// send transaction to mempool
		let (req_sender, callback) = oneshot::channel();
		executor.mempool_client_sender.send(MempoolClientRequest::SubmitTransaction(
			user_transaction.clone(),
			req_sender
		)).await?;

		// tick the transaction pipe
		let (tx, rx) = async_channel::unbounded();
		executor.tick_transaction_pipe(tx).await?;

		// receive the callback
		callback.await??;
		
		// receive the transaction
		let received_transaction = rx.recv().await?;
		assert_eq!(received_transaction, user_transaction);

		handle.abort();

		Ok(())
	}


	#[tokio::test]
	async fn test_pipe_mempool_from_api() -> Result<(), anyhow::Error> {

		let executor = Executor::try_from_env()?;
		let mempool_executor = executor.clone();

		let (tx, rx) = async_channel::unbounded();
		let mempool_handle = tokio::spawn(async move {
			loop {
				mempool_executor.tick_transaction_pipe(tx.clone()).await?;
				tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
			};
			Ok(()) as Result<(), anyhow::Error>
		});

		let api = executor.try_get_apis().await?;
		let user_transaction = create_signed_transaction(0);
		let comparison_user_transaction = user_transaction.clone();
		let bcs_user_transaction = bcs::to_bytes(&user_transaction)?;
		let request = SubmitTransactionPost::Bcs(
			aptos_api::bcs_payload::Bcs(bcs_user_transaction)
		);
		api.transactions.submit_transaction(AcceptType::Bcs, request).await?;
		let received_transaction = rx.recv().await?;
		assert_eq!(received_transaction, comparison_user_transaction);

		mempool_handle.abort();
	
		Ok(())
	}

	#[tokio::test]
	async fn test_repeated_pipe_mempool_from_api() -> Result<(), anyhow::Error> {

		let executor = Executor::try_from_env()?;
		let mempool_executor = executor.clone();

		let (tx, rx) = async_channel::unbounded();
		let mempool_handle = tokio::spawn(async move {
			loop {
				mempool_executor.tick_transaction_pipe(tx.clone()).await?;
				tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
			};
			Ok(()) as Result<(), anyhow::Error>
		});

		let api = executor.try_get_apis().await?;
		let mut user_transactions = BTreeSet::new();
		let mut comparison_user_transactions = BTreeSet::new();
		for _ in 0..25 {

			let user_transaction = create_signed_transaction(0);
			let bcs_user_transaction = bcs::to_bytes(&user_transaction)?;
			user_transactions.insert(bcs_user_transaction.clone());

			let request = SubmitTransactionPost::Bcs(
				aptos_api::bcs_payload::Bcs(bcs_user_transaction)
			);
			api.transactions.submit_transaction(AcceptType::Bcs, request).await?;

			let received_transaction = rx.recv().await?;
			let bcs_received_transaction = bcs::to_bytes(&received_transaction)?;
			comparison_user_transactions.insert(bcs_received_transaction.clone());

		}

		assert_eq!(user_transactions.len(), comparison_user_transactions.len());
		assert_eq!(user_transactions, comparison_user_transactions);

		mempool_handle.abort();
	
		Ok(())
	}
}