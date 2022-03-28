use common::chain::block::Block;
use common::chain::transaction::{Transaction, TxMainChainIndex, TxMainChainPosition};
use common::primitives::{BlockHeight, Id, Idable};
use parity_scale_codec::{Codec, Decode, DecodeAll, Encode};
use storage::traits::{self, MapMut, MapRef, TransactionRo, TransactionRw};

use crate::{BlockchainStorage, BlockchainStorageRead, BlockchainStorageWrite, Transactional};

mod well_known {
    use super::{Block, Codec, Id};

    /// Pre-defined database keys
    pub trait Entry {
        /// Key for this entry
        const KEY: &'static [u8];
        /// Value type for this entry
        type Value: Codec;
    }

    macro_rules! declare_entry {
        ($name:ident: $type:ty) => {
            pub struct $name;
            impl Entry for $name {
                const KEY: &'static [u8] = stringify!($name).as_bytes();
                type Value = $type;
            }
        };
    }

    declare_entry!(StoreVersion: u32);
    declare_entry!(BestBlockId: Id<Block>);
}

storage::decl_schema! {
    // Database schema for blockchain storage
    Schema {
        // Storage for individual values.
        pub DBValues: Single,
        // Storage for blocks.
        pub DBBlocks: Single,
        // Storage for transaction indices.
        pub DBTxIndices: Single,
        // Storage for block IDs indexed by block height.
        pub DBBlockByHeight: Single,
    }
}

type StoreImpl = storage::Store<Schema>;
type RoTxImpl<'tx> = <StoreImpl as traits::Transactional<'tx, Schema>>::TransactionRo;
type RwTxImpl<'tx> = <StoreImpl as traits::Transactional<'tx, Schema>>::TransactionRw;

/// Persistent store for blockchain data
#[derive(Clone)]
pub struct Store(StoreImpl);

/// Store for blockchain data
impl Store {
    /// New empty storage
    pub fn new_empty() -> crate::Result<Self> {
        let mut store = Self(storage::Store::default());
        store.set_storage_version(1)?;
        Ok(store)
    }
}

impl<'tx> crate::Transactional<'tx> for Store {
    type TransactionRo = StoreTx<RoTxImpl<'tx>>;
    type TransactionRw = StoreTx<RwTxImpl<'tx>>;

    fn transaction_ro<'st: 'tx>(&'st self) -> Self::TransactionRo {
        StoreTx(traits::Transactional::transaction_ro(&self.0))
    }

    fn transaction_rw<'st: 'tx>(&'st self) -> Self::TransactionRw {
        StoreTx(traits::Transactional::transaction_rw(&self.0))
    }
}

impl BlockchainStorage for Store {}

macro_rules! delegate_to_transaction {
    ($(fn $f:ident $args:tt -> $ret:ty;)*) => {
        $(delegate_to_transaction!(@SELF [$f ($ret)] $args);)*
    };
    (@SELF $done:tt (&self $(, $($rest:tt)*)?)) => {
        delegate_to_transaction!(
            @BODY transaction_ro (Ok) $done ($($($rest)*)?)
        );
    };
    (@SELF $done:tt (&mut self $(, $($rest:tt)*)?)) => {
        delegate_to_transaction!(
            @BODY transaction_rw (storage::commit) mut $done ($($($rest)*)?)
        );
    };
    (@BODY $txfunc:ident ($commit:path) $($mut:ident)?
        [$f:ident ($ret:ty)]
        ($($arg:ident: $aty:ty),* $(,)?)
    ) => {
        fn $f(&$($mut)? self $(, $arg: $aty)*) -> $ret {
            #[allow(clippy::needless_question_mark)]
            self.$txfunc().run(|tx| $commit(tx.$f($($arg),*)?))
        }
    };
}

impl BlockchainStorageRead for Store {
    delegate_to_transaction! {
        fn get_storage_version(&self) -> crate::Result<u32>;
        fn get_best_block_id(&self) -> crate::Result<Option<Id<Block>>>;
        fn get_block(&self, id: Id<Block>) -> crate::Result<Option<Block>>;

        fn get_mainchain_tx_index(
            &self,
            tx_id: &Id<Transaction>,
        ) -> crate::Result<Option<TxMainChainIndex>>;

        fn get_mainchain_tx_by_position(
            &self,
            tx_index: &TxMainChainPosition,
        ) -> crate::Result<Option<Transaction>>;

        fn get_block_id_by_height(
            &self,
            height: &BlockHeight,
        ) -> crate::Result<Option<Id<Block>>>;
    }
}

impl BlockchainStorageWrite for Store {
    delegate_to_transaction! {
        fn set_storage_version(&mut self, version: u32) -> crate::Result<()>;
        fn set_best_block_id(&mut self, id: &Id<Block>) -> crate::Result<()>;
        fn add_block(&mut self, block: &Block) -> crate::Result<()>;
        fn del_block(&mut self, id: Id<Block>) -> crate::Result<()>;

        fn set_mainchain_tx_index(
            &mut self,
            tx_id: &Id<Transaction>,
            tx_index: &TxMainChainIndex,
        ) -> crate::Result<()>;

        fn del_mainchain_tx_index(&mut self, tx_id: &Id<Transaction>) -> crate::Result<()>;

        fn set_block_id_at_height(
            &mut self,
            height: &BlockHeight,
            block_id: &Id<Block>,
        ) -> crate::Result<()>;

        fn del_block_id_at_height(&mut self, height: &BlockHeight) -> crate::Result<()>;
    }
}

/// A wrapper around a storage transaction type
pub struct StoreTx<T>(T);

/// Blockchain data storage transaction
impl<Tx: for<'a> traits::GetMapRef<'a, Schema>> BlockchainStorageRead for StoreTx<Tx> {
    fn get_storage_version(&self) -> crate::Result<u32> {
        self.read_value::<well_known::StoreVersion>().map(|v| v.unwrap_or_default())
    }

    fn get_best_block_id(&self) -> crate::Result<Option<Id<Block>>> {
        self.read_value::<well_known::BestBlockId>()
    }

    fn get_block(&self, id: Id<Block>) -> crate::Result<Option<Block>> {
        self.read::<DBBlocks, _, _>(id.as_ref())
    }

    fn get_mainchain_tx_index(
        &self,
        tx_id: &Id<Transaction>,
    ) -> crate::Result<Option<TxMainChainIndex>> {
        self.read::<DBTxIndices, _, _>(tx_id.as_ref())
    }

    fn get_mainchain_tx_by_position(
        &self,
        tx_index: &TxMainChainPosition,
    ) -> crate::Result<Option<Transaction>> {
        let block_id = tx_index.get_block_id();
        match self.0.get::<DBBlocks, _>().get(block_id.as_ref()) {
            Err(e) => Err(e.into()),
            Ok(None) => Ok(None),
            Ok(Some(block)) => {
                let begin = tx_index.get_byte_offset_in_block() as usize;
                let end = begin + tx_index.get_serialized_size() as usize;
                let tx = block.get(begin..end).expect("Transaction outside of block range");
                let tx = Transaction::decode_all(tx).expect("Invalid tx encoding in DB");
                Ok(Some(tx))
            }
        }
    }

    fn get_block_id_by_height(&self, height: &BlockHeight) -> crate::Result<Option<Id<Block>>> {
        self.read::<DBBlockByHeight, _, _>(&height.encode())
    }
}

impl<Tx: for<'a> traits::GetMapMut<'a, Schema>> BlockchainStorageWrite for StoreTx<Tx> {
    fn set_storage_version(&mut self, version: u32) -> crate::Result<()> {
        self.write_value::<well_known::StoreVersion>(&version)
    }

    fn set_best_block_id(&mut self, id: &Id<Block>) -> crate::Result<()> {
        self.write_value::<well_known::BestBlockId>(id)
    }

    fn add_block(&mut self, block: &Block) -> crate::Result<()> {
        self.write::<DBBlocks, _, _>(block.get_id().encode(), block)
    }

    fn del_block(&mut self, id: Id<Block>) -> crate::Result<()> {
        self.0.get_mut::<DBBlocks, _>().del(id.as_ref()).map_err(Into::into)
    }

    fn set_mainchain_tx_index(
        &mut self,
        tx_id: &Id<Transaction>,
        tx_index: &TxMainChainIndex,
    ) -> crate::Result<()> {
        self.write::<DBTxIndices, _, _>(tx_id.encode(), tx_index)
    }

    fn del_mainchain_tx_index(&mut self, tx_id: &Id<Transaction>) -> crate::Result<()> {
        self.0.get_mut::<DBTxIndices, _>().del(tx_id.as_ref()).map_err(Into::into)
    }

    fn set_block_id_at_height(
        &mut self,
        height: &BlockHeight,
        block_id: &Id<Block>,
    ) -> crate::Result<()> {
        self.write::<DBBlockByHeight, _, _>(height.encode(), block_id)
    }

    fn del_block_id_at_height(&mut self, height: &BlockHeight) -> crate::Result<()> {
        self.0.get_mut::<DBBlockByHeight, _>().del(&height.encode()).map_err(Into::into)
    }
}

impl<'a, Tx: traits::GetMapRef<'a, Schema>> StoreTx<Tx> {
    // Read a value from the database and decode it
    fn read<Col, I, T>(&'a self, key: &[u8]) -> crate::Result<Option<T>>
    where
        Col: storage::schema::Column<Kind = storage::schema::Single>,
        Schema: storage::schema::HasColumn<Col, I>,
        T: Decode,
    {
        let col = self.0.get::<Col, I>();
        let data = col.get(key).map_err(crate::Error::from)?;
        Ok(data.map(|d| T::decode_all(d).expect("Cannot decode a database value")))
    }

    // Read a value for a well-known entry
    fn read_value<E: well_known::Entry>(&'a self) -> crate::Result<Option<E::Value>> {
        self.read::<DBValues, _, _>(E::KEY)
    }
}

impl<'a, Tx: traits::GetMapMut<'a, Schema>> StoreTx<Tx> {
    // Encode a value and write it to the database
    fn write<Col, I, T>(&'a mut self, key: Vec<u8>, value: &T) -> crate::Result<()>
    where
        Col: storage::schema::Column<Kind = storage::schema::Single>,
        Schema: storage::schema::HasColumn<Col, I>,
        T: Encode,
    {
        self.0.get_mut::<Col, I>().put(key, value.encode()).map_err(Into::into)
    }

    // Write a value for a well-known entry
    fn write_value<E: well_known::Entry>(&'a mut self, val: &E::Value) -> crate::Result<()> {
        self.write::<DBValues, _, _>(E::KEY.to_vec(), val)
    }
}

impl<T: traits::TransactionRw<Error = storage::Error>> traits::TransactionRw for StoreTx<T> {
    type Error = crate::Error;

    fn commit(self) -> crate::Result<()> {
        self.0.commit().map_err(Into::into)
    }

    fn abort(self) -> crate::Result<()> {
        self.0.abort().map_err(Into::into)
    }
}

impl<T: traits::TransactionRo<Error = storage::Error>> traits::TransactionRo for StoreTx<T> {
    type Error = crate::Error;

    fn finalize(self) -> crate::Result<()> {
        self.0.finalize().map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_storage_get_default_version_in_tx() {
        common::concurrency::model(|| {
            let store = Store::new_empty().unwrap();
            let vtx = store.transaction_ro().run(|tx| tx.get_storage_version()).unwrap();
            let vst = store.get_storage_version().unwrap();
            assert_eq!(vtx, 1, "Default storage version wrong");
            assert_eq!(vtx, vst, "Transaction and non-transaction inconsistency");
        })
    }

    #[test]
    #[cfg(not(loom))]
    fn test_storage_manipulation() {
        use common::primitives::consensus_data::ConsensusData;
        use common::{chain::SpendablePosition, primitives::H256};

        // Prepare some test data
        let tx0 = Transaction::new(0xaabbccdd, vec![], vec![], 12).unwrap();
        let tx1 = Transaction::new(0xbbccddee, vec![], vec![], 34).unwrap();
        let block0 = Block::new(
            vec![tx0.clone()],
            Id::new(&H256::default()),
            12,
            ConsensusData::None,
        )
        .unwrap();
        let block1 =
            Block::new(vec![tx1.clone()], block0.get_id(), 34, ConsensusData::None).unwrap();

        // Set up the store
        let mut store = Store::new_empty().unwrap();

        // Storage version manipulation
        assert_eq!(store.get_storage_version(), Ok(1));
        assert_eq!(store.set_storage_version(2), Ok(()));
        assert_eq!(store.get_storage_version(), Ok(2));

        // Storte is now empty, the block is not there
        assert_eq!(store.get_block(block0.get_id()), Ok(None));

        // Insert the first block and check it is there
        assert_eq!(store.add_block(&block0), Ok(()));
        assert_eq!(&store.get_block(block0.get_id()).unwrap().unwrap(), &block0);

        // Insert, remove, and reinsert the second block
        assert_eq!(store.get_block(block1.get_id()), Ok(None));
        assert_eq!(store.add_block(&block1), Ok(()));
        assert_eq!(&store.get_block(block0.get_id()).unwrap().unwrap(), &block0);
        assert_eq!(store.del_block(block1.get_id()), Ok(()));
        assert_eq!(store.get_block(block1.get_id()), Ok(None));
        assert_eq!(store.add_block(&block1), Ok(()));
        assert_eq!(&store.get_block(block0.get_id()).unwrap().unwrap(), &block0);

        // Test the transaction extraction from a block
        let enc_tx0 = tx0.encode();
        let enc_block0 = block0.encode();
        let offset_tx0 = enc_block0
            .windows(enc_tx0.len())
            .enumerate()
            .find_map(|(i, d)| (d == enc_tx0).then(|| i))
            .unwrap();
        assert!(
            &enc_block0[offset_tx0..].starts_with(&enc_tx0),
            "Transaction format has changed, adjust the offset in this test",
        );
        let pos_tx0 = TxMainChainPosition::new(
            &block0.get_id().get(),
            offset_tx0 as u32,
            enc_tx0.len() as u32,
        );
        assert_eq!(
            &store.get_mainchain_tx_by_position(&pos_tx0).unwrap().unwrap(),
            &tx0
        );

        // Test setting and retrieving best chain id
        assert_eq!(store.get_best_block_id(), Ok(None));
        assert_eq!(store.set_best_block_id(&block0.get_id()), Ok(()));
        assert_eq!(store.get_best_block_id(), Ok(Some(block0.get_id())));
        assert_eq!(store.set_best_block_id(&block1.get_id()), Ok(()));
        assert_eq!(store.get_best_block_id(), Ok(Some(block1.get_id())));

        // Chain index operations
        let idx_tx0 = TxMainChainIndex::new(pos_tx0.into(), 1).expect("Tx index creation failed");
        assert_eq!(store.get_mainchain_tx_index(&tx0.get_id()), Ok(None));
        assert_eq!(
            store.set_mainchain_tx_index(&tx0.get_id(), &idx_tx0),
            Ok(())
        );
        assert_eq!(
            store.get_mainchain_tx_index(&tx0.get_id()),
            Ok(Some(idx_tx0.clone()))
        );
        assert_eq!(store.del_mainchain_tx_index(&tx0.get_id()), Ok(()));
        assert_eq!(store.get_mainchain_tx_index(&tx0.get_id()), Ok(None));
        assert_eq!(
            store.set_mainchain_tx_index(&tx0.get_id(), &idx_tx0),
            Ok(())
        );

        // Retrieve transactions by ID using the index
        assert_eq!(store.get_mainchain_tx_index(&tx1.get_id()), Ok(None));
        if let Ok(Some(index)) = store.get_mainchain_tx_index(&tx0.get_id()) {
            if let SpendablePosition::Transaction(ref p) = index.get_tx_position() {
                assert_eq!(store.get_mainchain_tx_by_position(p), Ok(Some(tx0)));
            } else {
                unreachable!();
            };
        } else {
            unreachable!();
        }
    }

    #[test]
    fn get_set_transactions() {
        common::concurrency::model(|| {
            // Set up the store and initialize the version to 2
            let mut store = Store::new_empty().unwrap();
            assert_eq!(store.set_storage_version(2), Ok(()));

            // Concurrently bump version and run a transactiomn that reads the version twice.
            let thr1 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let _ = store.transaction_rw().run(|tx| {
                        let v = tx.get_storage_version()?;
                        tx.set_storage_version(v + 1)?;
                        storage::commit(())
                    });
                })
            };
            let thr0 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let tx_result = store.transaction_ro().run(|tx| {
                        let v1 = tx.get_storage_version()?;
                        let v2 = tx.get_storage_version()?;
                        assert!([2, 3].contains(&v1));
                        assert_eq!(v1, v2, "Version query in a transaction inconsistent");
                        Ok(())
                    });
                    assert!(tx_result.is_ok());
                })
            };

            let _ = thr0.join();
            let _ = thr1.join();
            assert_eq!(store.get_storage_version(), Ok(3));
        })
    }

    #[test]
    fn test_storage_transactions() {
        common::concurrency::model(|| {
            // Set up the store and initialize the version to 2
            let mut store = Store::new_empty().unwrap();
            assert_eq!(store.set_storage_version(2), Ok(()));

            // Concurrently bump version by 3 and 5 in two separate threads
            let thr0 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let tx_result = store.transaction_rw().run(|tx| {
                        let v = tx.get_storage_version()?;
                        tx.set_storage_version(v + 3)?;
                        storage::commit(())
                    });
                    assert!(tx_result.is_ok());
                })
            };
            let thr1 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let tx_result = store.transaction_rw().run(|tx| {
                        let v = tx.get_storage_version()?;
                        tx.set_storage_version(v + 5)?;
                        storage::commit(())
                    });
                    assert!(tx_result.is_ok());
                })
            };

            let _ = thr0.join();
            let _ = thr1.join();
            assert_eq!(store.get_storage_version(), Ok(10));
        })
    }

    #[test]
    fn test_storage_transactions_with_result_check() {
        common::concurrency::model(|| {
            // Set up the store and initialize the version to 2
            let mut store = Store::new_empty().unwrap();
            assert_eq!(store.set_storage_version(2), Ok(()));

            // Concurrently bump version by 3 and 5 in two separate threads
            let thr0 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let mut tx = store.transaction_rw();
                    let v = tx.get_storage_version().unwrap();
                    assert!(tx.set_storage_version(v + 3).is_ok());
                    assert!(tx.commit().is_ok());
                })
            };
            let thr1 = {
                let store = Store::clone(&store);
                common::thread::spawn(move || {
                    let mut tx = store.transaction_rw();
                    let v = tx.get_storage_version().unwrap();
                    assert!(tx.set_storage_version(v + 5).is_ok());
                    assert!(tx.commit().is_ok());
                })
            };

            let _ = thr0.join();
            let _ = thr1.join();
            assert_eq!(store.get_storage_version(), Ok(10));
        })
    }
}
