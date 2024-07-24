use anyhow::Error;
use mempool_util::{MempoolBlockOperations, MempoolTransaction, MempoolTransactionOperations};
use movement_types::{Block, Id};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde_json;
//use std::sync::RwLock;

use std::fmt::Write;
//use std::sync::Arc;

#[derive(Debug)]
pub struct RocksdbMempool {
	// [`rocksdb::DB`] is already interior mutably locked, so we don't need to wrap it in an `RwLock`
	db: DB,
}
impl RocksdbMempool {
	pub fn try_new(path: &str) -> Result<Self, Error> {
		let mut options = Options::default();
		options.create_if_missing(true);
		options.create_missing_column_families(true);

		let mempool_transactions_cf =
			ColumnFamilyDescriptor::new("mempool_transactions", Options::default());
		let transaction_truths_cf =
			ColumnFamilyDescriptor::new("transaction_truths", Options::default());
		let blocks_cf = ColumnFamilyDescriptor::new("blocks", Options::default());
		let transaction_lookups_cf =
			ColumnFamilyDescriptor::new("transaction_lookups", Options::default());

		let db = DB::open_cf_descriptors(
			&options,
			path,
			vec![mempool_transactions_cf, transaction_truths_cf, blocks_cf, transaction_lookups_cf],
		)
		.map_err(|e| Error::new(e))?;

		Ok(RocksdbMempool { db })
	}

	pub fn construct_mempool_transaction_key(transaction: &MempoolTransaction) -> String {
		// Pre-allocate a string with the required capacity
		let mut key = String::with_capacity(32 + 1 + 32 + 1 + 32);
		// Write key components. The numbers are zero-padded to 32 characters.
		key.write_fmt(format_args!(
			"{:032}:{:032}:{}",
			transaction.timestamp,
			transaction.transaction.sequence_number,
			transaction.transaction.id(),
		))
		.unwrap();
		key
	}

	/// Helper function to retrieve the key for mempool transaction from the lookup table.
	async fn get_mempool_transaction_key(
		&self,
		transaction_id: &Id,
	) -> Result<Option<Vec<u8>>, Error> {
		let cf_handle = self
			.db
			.cf_handle("transaction_lookups")
			.ok_or_else(|| Error::msg("CF handle not found"))?;
		self.db.get_cf(&cf_handle, transaction_id.to_vec()).map_err(|e| Error::new(e))
	}
}

impl MempoolTransactionOperations for RocksdbMempool {
	async fn has_mempool_transaction(&self, transaction_id: Id) -> Result<bool, Error> {
		let key = self.get_mempool_transaction_key(&transaction_id).await?;
		match key {
			Some(k) => {
				let cf_handle = self
					.db
					.cf_handle("mempool_transactions")
					.ok_or_else(|| Error::msg("CF handle not found"))?;
				Ok(self.db.get_cf(&cf_handle, k)?.is_some())
			}
			None => Ok(false),
		}
	}

	async fn add_mempool_transaction(&self, tx: MempoolTransaction) -> Result<(), Error> {
		let serialized_tx = serde_json::to_vec(&tx)?;
		let mempool_transactions_cf_handle = self
			.db
			.cf_handle("mempool_transactions")
			.ok_or_else(|| Error::msg("CF handle not found"))?;
		let transaction_lookups_cf_handle = self
			.db
			.cf_handle("transaction_lookups")
			.ok_or_else(|| Error::msg("CF handle not found"))?;

		let key = Self::construct_mempool_transaction_key(&tx);
		self.db.put_cf(&mempool_transactions_cf_handle, &key, &serialized_tx)?;
		self.db
			.put_cf(&transaction_lookups_cf_handle, tx.transaction.id().to_vec(), &key)?;

		Ok(())
	}

	async fn remove_mempool_transaction(&self, transaction_id: Id) -> Result<(), Error> {
		let key = self.get_mempool_transaction_key(&transaction_id).await?;

		match key {
			Some(k) => {
				let cf_handle = self
					.db
					.cf_handle("mempool_transactions")
					.ok_or_else(|| Error::msg("CF handle not found"))?;
				self.db.delete_cf(&cf_handle, k)?;
				let lookups_cf_handle = self
					.db
					.cf_handle("transaction_lookups")
					.ok_or_else(|| Error::msg("CF handle not found"))?;
				self.db.delete_cf(&lookups_cf_handle, transaction_id.to_vec())?;
			}
			None => (),
		}
		Ok(())
	}

	// Updated method signatures and implementations go here
	async fn get_mempool_transaction(
		&self,
		transaction_id: Id,
	) -> Result<Option<MempoolTransaction>, Error> {
		let key = match self.get_mempool_transaction_key(&transaction_id).await? {
			Some(k) => k,
			None => return Ok(None), // If no key found in lookup, return None
		};
		let cf_handle = self
			.db
			.cf_handle("mempool_transactions")
			.ok_or_else(|| Error::msg("CF handle not found"))?;
		match self.db.get_cf(&cf_handle, &key)? {
			Some(serialized_tx) => {
				let tx: MempoolTransaction = serde_json::from_slice(&serialized_tx)?;
				Ok(Some(tx))
			}
			None => Ok(None),
		}
	}

	async fn pop_mempool_transaction(&self) -> Result<Option<MempoolTransaction>, Error> {
		let cf_handle = self
			.db
			.cf_handle("mempool_transactions")
			.ok_or_else(|| Error::msg("CF handle not found"))?;
		let mut iter = self.db.iterator_cf(&cf_handle, rocksdb::IteratorMode::Start);

		match iter.next() {
			None => return Ok(None), // No transactions to pop
			Some(res) => {
				let (key, value) = res?;
				let tx: MempoolTransaction = serde_json::from_slice(&value)?;
				self.db.delete_cf(&cf_handle, &key)?;

				// Optionally, remove from the lookup table as well
				let lookups_cf_handle = self
					.db
					.cf_handle("transaction_lookups")
					.ok_or_else(|| Error::msg("CF handle not found"))?;
				self.db.delete_cf(&lookups_cf_handle, tx.transaction.id().to_vec())?;

				Ok(Some(tx))
			}
		}
	}
}

impl MempoolBlockOperations for RocksdbMempool {
	async fn has_block(&self, block_id: Id) -> Result<bool, Error> {
		let cf_handle =
			self.db.cf_handle("blocks").ok_or_else(|| Error::msg("CF handle not found"))?;
		Ok(self.db.get_cf(&cf_handle, block_id.to_vec())?.is_some())
	}

	async fn add_block(&self, block: Block) -> Result<(), Error> {
		let serialized_block = serde_json::to_vec(&block)?;
		let cf_handle =
			self.db.cf_handle("blocks").ok_or_else(|| Error::msg("CF handle not found"))?;
		self.db.put_cf(&cf_handle, block.id().to_vec(), &serialized_block)?;
		Ok(())
	}

	async fn remove_block(&self, block_id: Id) -> Result<(), Error> {
		let cf_handle =
			self.db.cf_handle("blocks").ok_or_else(|| Error::msg("CF handle not found"))?;
		self.db.delete_cf(&cf_handle, block_id.to_vec())?;
		Ok(())
	}

	async fn get_block(&self, block_id: Id) -> Result<Option<Block>, Error> {
		let cf_handle =
			self.db.cf_handle("blocks").ok_or_else(|| Error::msg("CF handle not found"))?;
		let serialized_block = self.db.get_cf(&cf_handle, block_id.to_vec())?;
		match serialized_block {
			Some(serialized_block) => {
				let block: Block = serde_json::from_slice(&serialized_block)?;
				Ok(Some(block))
			}
			None => Ok(None),
		}
	}
}

#[cfg(test)]
pub mod test {

	use super::*;
	use movement_types::Transaction;
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_rocksdb_mempool_basic_operations() -> Result<(), Error> {
		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path().to_str().unwrap();
		let mempool = RocksdbMempool::try_new(path)?;

		let tx = MempoolTransaction::test();
		let tx_id = tx.id();
		mempool.add_mempool_transaction(tx.clone()).await?;
		assert!(mempool.has_mempool_transaction(tx_id.clone()).await?);
		let tx2 = mempool.get_mempool_transaction(tx_id.clone()).await?;
		assert_eq!(Some(tx), tx2);
		mempool.remove_mempool_transaction(tx_id.clone()).await?;
		assert!(!mempool.has_mempool_transaction(tx_id.clone()).await?);

		let block = Block::test();
		let block_id = block.id();
		mempool.add_block(block.clone()).await?;
		assert!(mempool.has_block(block_id.clone()).await?);
		let block2 = mempool.get_block(block_id.clone()).await?;
		assert_eq!(Some(block), block2);
		mempool.remove_block(block_id.clone()).await?;
		assert!(!mempool.has_block(block_id.clone()).await?);

		Ok(())
	}

	#[tokio::test]
	async fn test_rocksdb_transaction_operations() -> Result<(), Error> {
		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path().to_str().unwrap();
		let mempool = RocksdbMempool::try_new(path)?;

		let tx = Transaction::test();
		let tx_id = tx.id();
		mempool.add_transaction(tx.clone()).await?;
		assert!(mempool.has_transaction(tx_id.clone()).await?);
		let tx2 = mempool.get_transaction(tx_id.clone()).await?;
		assert_eq!(Some(tx), tx2);
		mempool.remove_transaction(tx_id.clone()).await?;
		assert!(!mempool.has_transaction(tx_id.clone()).await?);

		Ok(())
	}

	#[tokio::test]
	async fn test_transaction_slot_based_ordering() -> Result<(), Error> {
		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path().to_str().unwrap();
		let mempool = RocksdbMempool::try_new(path)?;

		let tx1 = MempoolTransaction::at_time(Transaction::new(vec![1], 0), 2);
		let tx2 = MempoolTransaction::at_time(Transaction::new(vec![2], 0), 64);
		let tx3 = MempoolTransaction::at_time(Transaction::new(vec![3], 0), 128);

		mempool.add_mempool_transaction(tx2.clone()).await?;
		mempool.add_mempool_transaction(tx1.clone()).await?;
		mempool.add_mempool_transaction(tx3.clone()).await?;

		let txs = mempool.pop_mempool_transactions(3).await?;
		assert_eq!(txs[0], tx1);
		assert_eq!(txs[1], tx2);
		assert_eq!(txs[2], tx3);

		Ok(())
	}

	#[tokio::test]
	async fn test_transaction_sequence_number_based_ordering() -> Result<(), Error> {
		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path().to_str().unwrap();
		let mempool = RocksdbMempool::try_new(path)?;

		let tx1 = MempoolTransaction::at_time(Transaction::new(vec![1], 0), 2);
		let tx2 = MempoolTransaction::at_time(Transaction::new(vec![2], 1), 2);
		let tx3 = MempoolTransaction::at_time(Transaction::new(vec![3], 0), 64);

		mempool.add_mempool_transaction(tx2.clone()).await?;
		mempool.add_mempool_transaction(tx1.clone()).await?;
		mempool.add_mempool_transaction(tx3.clone()).await?;

		let txs = mempool.pop_mempool_transactions(3).await?;
		assert_eq!(txs[0], tx1);
		assert_eq!(txs[1], tx2);
		assert_eq!(txs[2], tx3);

		Ok(())
	}

	#[tokio::test]
	async fn test_slot_and_transaction_based_ordering() -> Result<(), Error> {
		let temp_dir = tempdir().unwrap();
		let path = temp_dir.path().to_str().unwrap();
		let mempool = RocksdbMempool::try_new(path)?;

		let tx1 = MempoolTransaction::at_time(Transaction::new(vec![1], 0), 0);
		let tx2 = MempoolTransaction::at_time(Transaction::new(vec![2], 1), 0);
		let tx3 = MempoolTransaction::at_time(Transaction::new(vec![3], 2), 0);

		mempool.add_mempool_transaction(tx2.clone()).await?;
		mempool.add_mempool_transaction(tx1.clone()).await?;
		mempool.add_mempool_transaction(tx3.clone()).await?;

		let txs = mempool.pop_mempool_transactions(3).await?;
		assert_eq!(txs[0], tx1);
		assert_eq!(txs[1], tx2);
		assert_eq!(txs[2], tx3);

		Ok(())
	}
}
