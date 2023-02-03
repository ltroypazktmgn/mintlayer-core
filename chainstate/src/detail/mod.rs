// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod ban_score;
pub mod query;
pub mod tokens;

mod block_index_history_iter;
pub mod bootstrap;
mod chainstateref;
mod error;
mod median_time;
mod orphan_blocks;
pub mod tx_verification_strategy;

pub use self::error::*;
pub use self::median_time::calculate_median_time_past;
pub use self::tokens::is_rfc3986_valid_symbol;
pub use chainstate_types::Locator;
pub use error::{
    BlockError, CheckBlockError, CheckBlockTransactionsError, InitializationError, OrphanCheckError,
};
use pos_accounting::{FlushablePoSAccountingView, PoSAccountingDB, PoSAccountingDeltaData};
// TODO: ConnectTransactionError used in unit tests to check block processing results. We have to find more appropriate place for this error.
pub use transaction_verifier::{
    error::{ConnectTransactionError, TokensError, TxIndexError},
    storage::TransactionVerifierStorageError,
};
use tx_verifier::transaction_verifier;

use std::sync::Arc;

use itertools::Itertools;

use chainstate_storage::{
    BlockchainStorage, BlockchainStorageRead, BlockchainStorageWrite, SealedStorageTag,
    TransactionRw, Transactional,
};
use chainstate_types::{BlockIndex, GenBlockIndex, PropertyQueryError};
use common::{
    chain::{
        block::{timestamp::BlockTimestamp, BlockHeader},
        config::ChainConfig,
        Block, GenBlockId,
    },
    primitives::{id::WithId, BlockDistance, BlockHeight, Id, Idable},
    time_getter::TimeGetter,
};
use consensus::pos::is_due_for_epoch_data_calculation;
use logging::log;
use utils::{
    eventhandler::{EventHandler, EventsController},
    tap_error_log::LogError,
};
use utxo::UtxosDB;

use self::{
    orphan_blocks::{OrphanBlocksRef, OrphanBlocksRefMut},
    query::ChainstateQuery,
    tx_verification_strategy::TransactionVerificationStrategy,
};
use crate::{detail::orphan_blocks::OrphanBlocksPool, ChainstateConfig, ChainstateEvent};

type TxRw<'a, S> = <S as Transactional<'a>>::TransactionRw;
type TxRo<'a, S> = <S as Transactional<'a>>::TransactionRo;
type ChainstateEventHandler = EventHandler<ChainstateEvent>;

// TODO: move this to some configuration, maybe p2p?
pub const HEADER_LIMIT: BlockDistance = BlockDistance::new(2000);

pub type OrphanErrorHandler = dyn Fn(&BlockError) + Send + Sync;

#[must_use]
pub struct Chainstate<S, V> {
    chain_config: Arc<ChainConfig>,
    chainstate_config: ChainstateConfig,
    chainstate_storage: S,
    tx_verification_strategy: V,
    orphan_blocks: OrphanBlocksPool,
    custom_orphan_error_hook: Option<Arc<OrphanErrorHandler>>,
    events_controller: EventsController<ChainstateEvent>,
    time_getter: TimeGetter,
    is_initial_block_download_finished: bool,
}

#[derive(Copy, Clone, Eq, Debug, PartialEq)]
pub enum BlockSource {
    Peer,
    Local,
}

impl<S: BlockchainStorage, V: TransactionVerificationStrategy> Chainstate<S, V> {
    #[allow(dead_code)]
    pub fn wait_for_all_events(&self) {
        self.events_controller.wait_for_all_events();
    }

    fn make_db_tx(
        &mut self,
    ) -> chainstate_storage::Result<chainstateref::ChainstateRef<TxRw<'_, S>, OrphanBlocksRefMut, V>>
    {
        let db_tx = self.chainstate_storage.transaction_rw(None)?;
        Ok(chainstateref::ChainstateRef::new_rw(
            &self.chain_config,
            &self.chainstate_config,
            &self.tx_verification_strategy,
            db_tx,
            self.orphan_blocks.as_rw_ref(),
            self.time_getter.getter(),
        ))
    }

    pub(crate) fn make_db_tx_ro(
        &self,
    ) -> chainstate_storage::Result<chainstateref::ChainstateRef<TxRo<'_, S>, OrphanBlocksRef, V>>
    {
        let db_tx = self.chainstate_storage.transaction_ro()?;
        Ok(chainstateref::ChainstateRef::new_ro(
            &self.chain_config,
            &self.chainstate_config,
            &self.tx_verification_strategy,
            db_tx,
            self.orphan_blocks.as_ro_ref(),
            self.time_getter.getter(),
        ))
    }

    pub fn query(
        &self,
    ) -> Result<ChainstateQuery<TxRo<'_, S>, OrphanBlocksRef, V>, PropertyQueryError> {
        self.make_db_tx_ro().map(ChainstateQuery::new).map_err(PropertyQueryError::from)
    }

    pub fn subscribe_to_events(&mut self, handler: ChainstateEventHandler) {
        self.events_controller.subscribe_to_events(handler);
    }

    pub fn new(
        chain_config: Arc<ChainConfig>,
        chainstate_config: ChainstateConfig,
        chainstate_storage: S,
        tx_verification_strategy: V,
        custom_orphan_error_hook: Option<Arc<OrphanErrorHandler>>,
        time_getter: TimeGetter,
    ) -> Result<Self, crate::ChainstateError> {
        use crate::ChainstateError;

        let best_block_id = chainstate_storage
            .get_best_block_id()
            .map_err(|e| ChainstateError::FailedToInitializeChainstate(e.into()))
            .log_err()?;

        let mut chainstate = Self::new_no_genesis(
            chain_config,
            chainstate_config,
            chainstate_storage,
            tx_verification_strategy,
            custom_orphan_error_hook,
            time_getter,
        );

        chainstate
            .process_tx_index_enabled_flag()
            .map_err(crate::ChainstateError::from)?;

        if best_block_id.is_none() {
            chainstate
                .process_genesis()
                .map_err(crate::ChainstateError::ProcessBlockError)
                .log_err()?;
        } else {
            chainstate.check_genesis().map_err(crate::ChainstateError::from)?;
        }

        Ok(chainstate)
    }

    fn new_no_genesis(
        chain_config: Arc<ChainConfig>,
        chainstate_config: ChainstateConfig,
        chainstate_storage: S,
        tx_verification_strategy: V,
        custom_orphan_error_hook: Option<Arc<OrphanErrorHandler>>,
        time_getter: TimeGetter,
    ) -> Self {
        let orphan_blocks = OrphanBlocksPool::new(*chainstate_config.max_orphan_blocks);
        Self {
            chain_config,
            chainstate_config,
            chainstate_storage,
            tx_verification_strategy,
            orphan_blocks,
            custom_orphan_error_hook,
            events_controller: EventsController::new(),
            time_getter,
            is_initial_block_download_finished: false,
        }
    }

    fn check_genesis(&self) -> Result<(), InitializationError> {
        let dbtx = self.make_db_tx_ro()?;

        let config_geneis_id = self.chain_config().genesis_block_id();
        if config_geneis_id == dbtx.get_best_block_id()? {
            // Best block is genesis, everything fine
            return Ok(());
        }

        // Look up the parent of block 1 to figure out the genesis ID according to storage
        let block1_id = dbtx
            .get_block_id_by_height(&BlockHeight::new(1))?
            .ok_or(InitializationError::Block1Missing)?;
        let block1 = dbtx
            .get_block(Id::new(block1_id.get()))?
            .ok_or(InitializationError::Block1Missing)?;
        let stored_genesis_id = block1.prev_block_id();

        // Check storage genesis ID matches chain config genesis ID
        utils::ensure!(
            config_geneis_id == stored_genesis_id,
            InitializationError::GenesisMismatch(config_geneis_id, stored_genesis_id),
        );

        Ok(())
    }

    /// Check that transaction index state is consistent between DB and config.
    fn process_tx_index_enabled_flag(&mut self) -> Result<(), BlockError> {
        let mut db_tx = self
            .chainstate_storage
            .transaction_rw(None)
            .map_err(BlockError::from)
            .log_err()?;

        let tx_index_enabled = db_tx
            .get_is_mainchain_tx_index_enabled()
            .map_err(BlockError::StorageError)
            .log_err()?;

        if let Some(tx_index_enabled) = tx_index_enabled {
            // Make sure DB indexing state is same as in the config.
            // TODO: Allow changing state (creating new or deleting existing index).
            utils::ensure!(
                *self.chainstate_config.tx_index_enabled == tx_index_enabled,
                BlockError::TxIndexConfigError
            );
        } else {
            // First start, enable or disable indexing depending on config.
            db_tx
                .set_is_mainchain_tx_index_enabled(*self.chainstate_config.tx_index_enabled)
                .map_err(BlockError::StorageError)
                .log_err()?;
        }

        db_tx.commit().expect("Set tx indexing failed");

        Ok(())
    }

    fn broadcast_new_tip_event(&self, new_block_index: &Option<BlockIndex>) {
        match new_block_index {
            Some(ref new_block_index) => {
                let new_height = new_block_index.block_height();
                let new_id = *new_block_index.block_id();
                self.events_controller.broadcast(ChainstateEvent::NewTip(new_id, new_height))
            }
            None => (),
        }
    }

    /// returns the new block index, which is the new tip, if any
    fn process_orphans(&mut self, last_processed_block: &Id<Block>) -> Option<BlockIndex> {
        let orphans = self.orphan_blocks.take_all_children_of(&(*last_processed_block).into());
        let (block_indexes, block_errors): (Vec<Option<BlockIndex>>, Vec<BlockError>) = orphans
            .into_iter()
            .map(|blk| self.process_block(blk, BlockSource::Local))
            .partition_result();

        block_errors.into_iter().for_each(|e| match &self.custom_orphan_error_hook {
            Some(handler) => handler(&e),
            None => logging::log::error!("Failed to process a chain of orphan blocks: {}", e),
        });

        // since we processed the blocks in order, the last one is the best tip
        block_indexes.into_iter().flatten().rev().next()
    }

    fn process_db_commit_error(
        &mut self,
        db_error: chainstate_storage::Error,
        block: WithId<Block>,
        block_source: BlockSource,
        attempt_number: usize,
    ) -> Result<Option<BlockIndex>, BlockError> {
        if attempt_number >= *self.chainstate_config.max_db_commit_attempts {
            Err(BlockError::DatabaseCommitError(
                block.get_id(),
                *self.chainstate_config.max_db_commit_attempts,
                db_error,
            ))
        } else {
            // TODO: test reattempts using mocks of the database that emulate failure
            self.attempt_to_process_block(block, block_source, attempt_number + 1)
        }
    }

    fn activate_seal_epoch(&mut self, height: BlockHeight) -> Result<(), BlockError> {
        if is_due_for_epoch_data_calculation(&self.chain_config, height) {
            let current_epoch_index = self.chain_config.epoch_index_from_height(&height);
            let sealed_epoch_distance_from_tip =
                self.chain_config.sealed_epoch_distance_from_tip() as u64;

            if current_epoch_index >= sealed_epoch_distance_from_tip {
                let epoch_index_to_seal = current_epoch_index - sealed_epoch_distance_from_tip;

                let first_block_epoch_to_seal =
                    epoch_index_to_seal * self.chain_config.epoch_length().get();
                let last_block_epoch_to_seal =
                    first_block_epoch_to_seal + self.chain_config.epoch_length().get() - 1;

                // iterate over every block in the epoch and merge every block delta into a singe delta
                let mut db_tx = self.chainstate_storage.transaction_rw(None).log_err()?;
                let epoch_delta = (first_block_epoch_to_seal..=last_block_epoch_to_seal)
                    .try_fold(
                        PoSAccountingDeltaData::new(),
                        |mut delta: PoSAccountingDeltaData, height| -> Result<_, BlockError> {
                            let genblock_id = db_tx
                                .get_block_id_by_height(&BlockHeight::new(height))?
                                .ok_or_else(|| {
                                    BlockError::BlockAtHeightNotFound(BlockHeight::new(height))
                                })?;
                            match genblock_id.classify(&self.chain_config) {
                                GenBlockId::Genesis(_) => (), /* skip genesis block for now */
                                GenBlockId::Block(block_id) => {
                                    let block_delta = db_tx
                                        .get_accounting_delta(block_id)?
                                        .ok_or(BlockError::PoSAccountingDeltaNotFound(block_id))?;
                                    delta.merge_with_delta(block_delta)?;
                                }
                            };
                            Ok(delta)
                        },
                    )
                    .log_err()?;

                // flush delta to sealed storage
                let mut db = PoSAccountingDB::<_, SealedStorageTag>::new(&mut db_tx);
                db.batch_write_delta(epoch_delta)
                    .map_err(TransactionVerifierStorageError::from)
                    .log_err()?;
                db_tx.commit()?;
            }
        }

        Ok(())
    }

    pub fn attempt_to_process_block(
        &mut self,
        block: WithId<Block>,
        block_source: BlockSource,
        attempt_number: usize,
    ) -> Result<Option<BlockIndex>, BlockError> {
        log::info!("Processing block: {}", block.get_id());

        let mut chainstate_ref = self.make_db_tx().map_err(BlockError::from).log_err()?;

        let block = chainstate_ref.check_legitimate_orphan(block_source, block).log_err()?;

        let best_block_id = chainstate_ref
            .get_best_block_id()
            .map_err(BlockError::BestBlockLoadError)
            .log_err()?;

        chainstate_ref
            .check_block(&block)
            .map_err(BlockError::CheckBlockFailed)
            .log_err()?;

        let block_index = chainstate_ref.accept_block(&block).log_err()?;
        let block_height = block_index.block_height();
        let result = chainstate_ref.activate_best_chain(block_index, best_block_id).log_err()?;
        let db_commit_result = chainstate_ref.commit_db_tx().log_err();
        match db_commit_result {
            Ok(_) => {}
            Err(err) => {
                return self
                    .process_db_commit_error(err, block, block_source, attempt_number)
                    .log_err()
            }
        }

        self.activate_seal_epoch(block_height)?;

        let new_block_index_after_orphans = self.process_orphans(&block.get_id());
        let result = match new_block_index_after_orphans {
            Some(result_from_orphan) => Some(result_from_orphan),
            None => result,
        };

        self.broadcast_new_tip_event(&result);

        if let Some(ref bi) = result {
            log::info!(
                "New tip in chainstate {} with height {}",
                bi.block_id(),
                bi.block_height()
            );

            self.is_initial_block_download_finished = self.is_fresh_block(&bi.block_timestamp());
        }

        Ok(result)
    }

    /// returns the block index of the new tip
    pub fn process_block(
        &mut self,
        block: WithId<Block>,
        block_source: BlockSource,
    ) -> Result<Option<BlockIndex>, BlockError> {
        self.attempt_to_process_block(block, block_source, 0)
    }

    /// Initialize chainstate with genesis block
    pub fn process_genesis(&mut self) -> Result<(), BlockError> {
        // Gather information about genesis.
        let genesis = self.chain_config.genesis_block();
        let genesis_id = self.chain_config.genesis_block_id();
        let utxo_count = genesis.utxos().len() as u32;
        let genesis_index = common::chain::TxMainChainIndex::new(genesis_id.into(), utxo_count)
            .expect("Genesis not constructed correctly");

        // Initialize storage with given info
        let mut db_tx = self
            .chainstate_storage
            .transaction_rw(None)
            .map_err(BlockError::from)
            .log_err()?;
        db_tx
            .set_best_block_id(&genesis_id)
            .map_err(BlockError::StorageError)
            .log_err()?;
        db_tx
            .set_block_id_at_height(&BlockHeight::zero(), &genesis_id)
            .map_err(BlockError::StorageError)
            .log_err()?;

        if *self.chainstate_config.tx_index_enabled {
            db_tx
                .set_mainchain_tx_index(&genesis_id.into(), &genesis_index)
                .map_err(BlockError::StorageError)
                .log_err()?;
        }

        // initialize the utxo-set by adding genesis outputs to it
        UtxosDB::initialize_db(&mut db_tx, &self.chain_config);

        db_tx.commit().expect("Genesis database initialization failed");
        Ok(())
    }

    pub fn preliminary_block_check(
        &self,
        block: WithId<Block>,
    ) -> Result<WithId<Block>, BlockError> {
        let chainstate_ref = self.make_db_tx_ro().map_err(BlockError::from)?;
        chainstate_ref.check_block(&block).log_err()?;
        Ok(block)
    }

    pub fn preliminary_header_check(&self, block: BlockHeader) -> Result<(), BlockError> {
        let chainstate_ref = self.make_db_tx_ro().map_err(BlockError::from)?;
        chainstate_ref.check_block_header(&block).log_err()?;
        Ok(())
    }

    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        &self.chain_config
    }

    pub fn chainstate_config(&self) -> &ChainstateConfig {
        &self.chainstate_config
    }

    pub fn orphan_blocks_pool(&self) -> &OrphanBlocksPool {
        &self.orphan_blocks
    }

    pub fn events_controller(&self) -> &EventsController<ChainstateEvent> {
        &self.events_controller
    }

    pub fn is_initial_block_download(&self) -> Result<bool, PropertyQueryError> {
        if self.is_initial_block_download_finished {
            return Ok(false);
        }

        // TODO: Add a check for importing and reindex.

        // TODO: Add a check for the chain trust.

        let tip_timestamp = match self.query()?.get_best_block_header() {
            Ok(h) => Ok(h.timestamp()),
            // There is only the genesis block, so the initial block download isn't finished yet.
            Err(PropertyQueryError::GenesisHeaderRequested) => return Ok(true),
            Err(e) => Err(e),
        }?;
        Ok(!self.is_fresh_block(&tip_timestamp))
    }

    /// Returns true if the given block timestamp is newer than `ChainstateConfig::max_tip_age`.
    fn is_fresh_block(&self, time: &BlockTimestamp) -> bool {
        let now = self.time_getter.get_time();
        time.as_duration_since_epoch() + self.chainstate_config.max_tip_age.clone().into() > now
    }
}

#[cfg(test)]
mod test;
