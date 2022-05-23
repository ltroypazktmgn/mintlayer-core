// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Author(s): S. Afach, A. Sinitsyn

use crate::detail::tests::test_framework::BlockTestFramework;
use crate::detail::tests::*;
use blockchain_storage::Store;
use common::chain::block::consensus_data::PoWData;
use common::chain::config::ChainConfigBuilder;
use common::chain::ConsensusUpgrade;
use common::chain::NetUpgrades;
use common::chain::OutputSpentState;
use common::chain::UpgradeVersion;
use common::primitives::Compact;
use common::Uint256;

#[test]
fn test_process_genesis_block_wrong_block_source() {
    common::concurrency::model(|| {
        // Genesis can't be from Peer, test it
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let mut consensus = Consensus::new_no_genesis(config.clone(), storage).unwrap();

        // process the genesis block
        let block_source = BlockSource::Peer(0);
        let result = consensus.process_block(config.genesis_block().clone(), block_source);
        assert_eq!(result, Err(BlockError::InvalidBlockSource));
    });
}

#[test]
fn test_process_genesis_block() {
    common::concurrency::model(|| {
        // This test process only Genesis block
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let mut consensus = Consensus::new_no_genesis(config, storage).unwrap();

        // process the genesis block
        let block_source = BlockSource::Local;
        let block_index = consensus
            .process_block(consensus.chain_config.genesis_block().clone(), block_source)
            .ok()
            .flatten()
            .unwrap();
        assert_eq!(
            consensus
                .blockchain_storage
                .get_best_block_id()
                .expect(ERR_BEST_BLOCK_NOT_FOUND),
            Some(consensus.chain_config.genesis_block().get_id())
        );
        assert_eq!(block_index.get_prev_block_id(), &None);
        assert_eq!(block_index.get_chain_trust(), 1);
        assert_eq!(block_index.get_block_height(), BlockHeight::new(0));
    });
}

#[test]
fn test_orphans_chains() {
    common::concurrency::model(|| {
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let mut consensus = Consensus::new(config, storage).unwrap();

        // Process the orphan block
        let new_block = consensus.chain_config.genesis_block().clone();
        for _ in 0..255 {
            let new_block = produce_test_block(&consensus.chain_config, &new_block, true);
            assert_eq!(
                consensus.process_block(new_block.clone(), BlockSource::Local),
                Err(BlockError::Orphan)
            );
        }
    });
}

#[test]
fn test_empty_consensus() {
    common::concurrency::model(|| {
        // No genesis
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let consensus = Consensus::new_no_genesis(config, storage).unwrap();
        assert!(consensus.get_best_block_id().unwrap().is_none());
        assert!(consensus
            .blockchain_storage
            .get_block(consensus.chain_config.genesis_block().get_id())
            .unwrap()
            .is_none());
        // Let's add genesis
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let consensus = Consensus::new(config, storage).unwrap();
        assert!(consensus.get_best_block_id().unwrap().is_some());
        assert!(
            consensus.get_best_block_id().ok().flatten().unwrap()
                == consensus.chain_config.genesis_block().get_id()
        );
        assert!(consensus
            .blockchain_storage
            .get_block(consensus.chain_config.genesis_block().get_id())
            .unwrap()
            .is_some());
        assert!(
            consensus
                .blockchain_storage
                .get_block(consensus.chain_config.genesis_block().get_id())
                .unwrap()
                .unwrap()
                .get_id()
                == consensus.chain_config.genesis_block().get_id()
        );
    });
}

#[test]
fn test_spend_inputs_simple() {
    common::concurrency::model(|| {
        let mut consensus = setup_consensus();

        // Create a new block
        let block = produce_test_block(
            &consensus.chain_config,
            consensus.chain_config.genesis_block(),
            false,
        );

        // Check that all tx not in the main chain
        for tx in block.transactions() {
            assert!(
                consensus
                    .blockchain_storage
                    .get_mainchain_tx_index(&OutPointSourceId::from(tx.get_id()))
                    .expect(ERR_STORAGE_FAIL)
                    == None
            );
        }

        // Process the second block
        let new_id = Some(block.get_id());
        assert!(consensus.process_block(block.clone(), BlockSource::Local).is_ok());
        assert_eq!(
            consensus
                .blockchain_storage
                .get_best_block_id()
                .expect(ERR_BEST_BLOCK_NOT_FOUND),
            new_id
        );

        // Check that tx inputs in the main chain and not spend
        for tx in block.transactions() {
            let tx_index = consensus
                .blockchain_storage
                .get_mainchain_tx_index(&OutPointSourceId::from(tx.get_id()))
                .expect("Not found mainchain tx index")
                .expect(ERR_STORAGE_FAIL);

            for input in tx.get_inputs() {
                if tx_index
                    .get_spent_state(input.get_outpoint().get_output_index())
                    .expect("Unable to get spent state")
                    != OutputSpentState::Unspent
                {
                    panic!("Tx input can't be spent");
                }
            }
        }
    });
}

#[test]
fn test_straight_chain() {
    common::concurrency::model(|| {
        const COUNT_BLOCKS: usize = 255;
        // In this test, processing a few correct blocks in a single chain
        let config = Arc::new(create_unit_test_config());
        let storage = Store::new_empty().unwrap();
        let mut consensus = Consensus::new_no_genesis(config, storage).unwrap();

        // process the genesis block
        let block_source = BlockSource::Local;
        let mut block_index = consensus
            .process_block(consensus.chain_config.genesis_block().clone(), block_source)
            .ok()
            .flatten()
            .expect("Unable to process genesis block");
        assert_eq!(
            consensus
                .blockchain_storage
                .get_best_block_id()
                .expect(ERR_BEST_BLOCK_NOT_FOUND),
            Some(consensus.chain_config.genesis_block().get_id())
        );
        assert_eq!(
            block_index.get_block_id(),
            &consensus.chain_config.genesis_block().get_id()
        );
        assert_eq!(block_index.get_prev_block_id(), &None);
        // TODO: ensure that block at height is tested after removing the next
        assert_eq!(block_index.get_chain_trust(), 1);
        assert_eq!(block_index.get_block_height(), BlockHeight::new(0));

        let mut prev_block = consensus.chain_config.genesis_block().clone();
        for _ in 0..COUNT_BLOCKS {
            let prev_block_id = block_index.get_block_id();
            let best_block_id = consensus
                .blockchain_storage
                .get_best_block_id()
                .ok()
                .flatten()
                .expect("Unable to get best block ID");
            assert_eq!(&best_block_id, block_index.get_block_id());
            let block_source = BlockSource::Peer(1);
            let new_block = produce_test_block(&consensus.chain_config, &prev_block, false);
            let new_block_index = dbg!(consensus.process_block(new_block.clone(), block_source))
                .ok()
                .flatten()
                .expect("Unable to process block");

            // TODO: ensure that block at height is tested after removing the next
            assert_eq!(
                new_block_index.get_prev_block_id().as_ref(),
                Some(prev_block_id)
            );
            assert!(new_block_index.get_chain_trust() > block_index.get_chain_trust());
            assert_eq!(
                new_block_index.get_block_height(),
                block_index.get_block_height().next_height()
            );

            block_index = new_block_index;
            prev_block = new_block;
        }
    });
}

#[test]
fn test_get_ancestor() {
    use crate::detail::tests::test_framework::BlockTestFramework;
    let mut btf = BlockTestFramework::new();

    // We will create two chains that split at height 100
    const SPLIT_HEIGHT: usize = 100;
    const ANCESTOR_HEIGHT: usize = 50;
    const FIRST_CHAIN_HEIGHT: usize = 500;
    const SECOND_CHAIN_LENGTH: usize = 300;
    btf.create_chain(&btf.genesis().get_id(), SPLIT_HEIGHT)
        .expect("Chain creation to succeed");

    let ancestor = btf.block_indexes[ANCESTOR_HEIGHT].clone();
    let split = btf.block_indexes[SPLIT_HEIGHT].clone();

    // Create the first chain and test get_ancestor for this chain's  last block
    btf.create_chain(split.get_block_id(), FIRST_CHAIN_HEIGHT - SPLIT_HEIGHT)
        .expect("second chain");
    let last_block_in_first_chain =
        btf.block_indexes.last().expect("last block in first chain").clone();

    const ANCESTOR_IN_FIRST_CHAIN_HEIGHT: usize = 400;
    let ancestor_in_first_chain = btf
        .block_indexes
        .get(ANCESTOR_IN_FIRST_CHAIN_HEIGHT)
        .expect("ancestor in first chain")
        .clone();

    assert_eq!(
        last_block_in_first_chain,
        btf.consensus
            .make_db_tx()
            .get_ancestor(
                &last_block_in_first_chain,
                u64::try_from(FIRST_CHAIN_HEIGHT).unwrap().into()
            )
            .expect("ancestor")
    );

    assert_eq!(
        ancestor,
        btf.consensus
            .make_db_tx()
            .get_ancestor(
                &last_block_in_first_chain,
                u64::try_from(ANCESTOR_HEIGHT).unwrap().into()
            )
            .expect("ancestor")
    );

    assert_eq!(
        ancestor_in_first_chain,
        btf.consensus
            .make_db_tx()
            .get_ancestor(
                &last_block_in_first_chain,
                u64::try_from(ANCESTOR_IN_FIRST_CHAIN_HEIGHT).unwrap().into()
            )
            .expect("ancestor in first chain")
    );

    // Create a second chain and test get_ancestor for this chain's last block
    btf.create_chain(split.get_block_id(), SECOND_CHAIN_LENGTH - SPLIT_HEIGHT)
        .expect("second chain");
    let last_block_in_second_chain =
        btf.block_indexes.last().expect("last block in first chain").clone();
    assert_eq!(
        ancestor,
        btf.consensus
            .make_db_tx()
            .get_ancestor(
                &last_block_in_second_chain,
                u64::try_from(ANCESTOR_HEIGHT).unwrap().into()
            )
            .expect("ancestor")
    );

    assert_eq!(
        Err(BlockError::InvalidAncestorHeight {
            ancestor_height: u64::try_from(SECOND_CHAIN_LENGTH + 1).unwrap().into(),
            block_height: u64::try_from(SECOND_CHAIN_LENGTH).unwrap().into(),
        }),
        btf.consensus.make_db_tx().get_ancestor(
            &last_block_in_second_chain,
            u64::try_from(SECOND_CHAIN_LENGTH + 1).unwrap().into()
        )
    );
}

#[test]
fn test_last_common_ancestor() {
    use crate::detail::tests::test_framework::BlockTestFramework;
    let mut btf = BlockTestFramework::new();

    // We will create two chains that split at height 100
    const SPLIT_HEIGHT: usize = 100;
    const FIRST_CHAIN_HEIGHT: usize = 500;
    const SECOND_CHAIN_LENGTH: usize = 300;
    btf.create_chain(&btf.genesis().get_id(), SPLIT_HEIGHT)
        .expect("Chain creation to succeed");
    let genesis = btf.block_indexes.get(0).expect("genesis_block").clone();
    let split = btf.block_indexes[SPLIT_HEIGHT].clone();

    // First branch of fork
    btf.create_chain(split.get_block_id(), FIRST_CHAIN_HEIGHT - SPLIT_HEIGHT)
        .expect("Chain creation to succeed");
    let last_block_in_first_chain =
        btf.block_indexes.last().expect("last block in first chain").clone();

    // Second branch of fork
    btf.create_chain(split.get_block_id(), SECOND_CHAIN_LENGTH - SPLIT_HEIGHT)
        .expect("second chain");
    let last_block_in_second_chain =
        btf.block_indexes.last().expect("last block in first chain").clone();

    assert_eq!(
        btf.consensus
            .make_db_tx()
            .last_common_ancestor(&last_block_in_first_chain, &last_block_in_second_chain),
        Ok(split.clone())
    );

    assert_eq!(
        btf.consensus
            .make_db_tx()
            .last_common_ancestor(&last_block_in_second_chain, &last_block_in_first_chain),
        Ok(split.clone())
    );

    assert_eq!(
        btf.consensus
            .make_db_tx()
            .last_common_ancestor(&last_block_in_first_chain, &last_block_in_first_chain),
        Ok(last_block_in_first_chain)
    );

    assert_eq!(
        btf.consensus.make_db_tx().last_common_ancestor(&genesis, &split),
        Ok(genesis)
    );
}

#[test]
fn test_consensus_type() {
    use common::chain::ConsensusUpgrade;
    use common::chain::NetUpgrades;
    use common::chain::UpgradeVersion;
    use common::Uint256;

    let ignore_consensus = BlockHeight::new(0);
    let pow = BlockHeight::new(5);
    let ignore_again = BlockHeight::new(10);
    let pow_again = BlockHeight::new(15);

    let min_difficulty =
        Uint256([0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF]);

    let upgrades = vec![
        (
            ignore_consensus,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::IgnoreConsensus),
        ),
        (
            pow,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::PoW {
                initial_difficulty: min_difficulty.into(),
            }),
        ),
        (
            ignore_again,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::IgnoreConsensus),
        ),
        (
            pow_again,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::PoW {
                initial_difficulty: min_difficulty.into(),
            }),
        ),
    ];

    let net_upgrades = NetUpgrades::initialize(upgrades).expect("valid netupgrades");

    // Internally this calls Consensus::new, which processes the genesis block
    // This should succeed because ChainConfigBuilder by default uses create_mainnet_genesis to
    // create the genesis_block, and this function creates a genesis block with
    // ConsenssuData::None, which agreess with the net_upgrades we defined above.
    let config = ChainConfigBuilder::new().with_net_upgrades(net_upgrades).build();
    let consensus = ConsensusBuilder::new().with_config(config.clone()).build();

    let mut btf = BlockTestFramework::with_consensus(consensus);

    // The next block will have height 1. At this height, we are still under IngoreConsenssu, so
    // processing a block with PoWData will fail
    let pow_block = produce_test_block_with_consensus_data(
        &config,
        btf.genesis(),
        false,
        ConsensusData::PoW(PoWData::new(Compact(0), 0, vec![])),
    );
    assert!(matches!(
        btf.add_special_block(pow_block),
        Err(BlockError::ConsensusTypeMismatch(..))
    ));

    // Create 4 more blocks with Consensus Nonw
    btf.create_chain(&btf.genesis().get_id(), 4).expect("chain creation");

    // The next block will be at height 5, so it is expected to be a PoW block. Let's crate a block
    // with ConsensusData::None and see that adding it fails
    let block_without_consensus_data = produce_test_block_with_consensus_data(
        &config,
        &btf.get_block(btf.block_indexes[4].get_block_id().clone()).unwrap().unwrap(),
        false,
        ConsensusData::None,
    );
    assert!(matches!(
        btf.add_special_block(block_without_consensus_data),
        Err(BlockError::ConsensusTypeMismatch(..))
    ));

    // Mine blocks 5-9 with minimal difficulty, as expected by net upgrades
    for i in 5..10 {
        let prev_block =
            btf.get_block(btf.block_indexes[i - 1].get_block_id().clone()).unwrap().unwrap();
        let mut mined_block = btf.random_block(&prev_block, None);
        let bits = min_difficulty.into();
        assert!(
            crate::detail::pow::work::mine(&mut mined_block, u128::MAX, bits, vec![])
                .expect("Unexpected conversion error")
        );
        assert!(btf.add_special_block(mined_block).is_ok());
    }

    // Block 10 should ignore consensus according to net upgrades. The following Pow block should
    // fail.
    let prev_block = btf.get_block(btf.block_indexes[9].get_block_id().clone()).unwrap().unwrap();
    let mut mined_block = btf.random_block(&prev_block, None);
    let bits = min_difficulty.into();
    assert!(
        crate::detail::pow::work::mine(&mut mined_block, u128::MAX, bits, vec![])
            .expect("Unexpected conversion error")
    );
    assert!(matches!(
        btf.add_special_block(mined_block),
        Err(BlockError::ConsensusTypeMismatch(..))
    ));

    // Create blocks 10-14 without consensus data as required by net_upgrades
    btf.create_chain(&prev_block.get_id(), 5).expect("chain creation");

    // At height 15 we are again proof of work, ignoring consensus should fail
    let prev_block = btf.get_block(btf.block_indexes[14].get_block_id().clone()).unwrap().unwrap();
    let block_without_consensus_data =
        produce_test_block_with_consensus_data(&config, &prev_block, false, ConsensusData::None);
    assert!(matches!(
        btf.add_special_block(block_without_consensus_data),
        Err(BlockError::ConsensusTypeMismatch(..))
    ));

    // Mining should work
    for i in 15..20 {
        let prev_block =
            btf.get_block(btf.block_indexes[i - 1].get_block_id().clone()).unwrap().unwrap();
        let mut mined_block = btf.random_block(&prev_block, None);
        let bits = min_difficulty.into();
        assert!(
            crate::detail::pow::work::mine(&mut mined_block, u128::MAX, bits, vec![])
                .expect("Unexpected conversion error")
        );
        assert!(btf.add_special_block(mined_block).is_ok());
    }
}

fn make_invalid_pow_block(
    block: &mut Block,
    max_nonce: u128,
    bits: Compact,
) -> Result<bool, BlockError> {
    let mut data = PoWData::new(bits, 0, vec![]);
    for nonce in 0..max_nonce {
        data.update_nonce(nonce);
        block.update_consensus_data(ConsensusData::PoW(data.clone()));

        if !crate::detail::pow::work::check_proof_of_work(block.get_id().get(), bits)? {
            return Ok(true);
        }
    }

    Ok(false)
}

#[test]
fn test_pow() {
    let ignore_consensus = BlockHeight::new(0);
    let pow_consensus = BlockHeight::new(1);
    let difficulty =
        Uint256([0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0x0FFFFFFFFFFFFFFF]);

    let upgrades = vec![
        (
            ignore_consensus,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::IgnoreConsensus),
        ),
        (
            pow_consensus,
            UpgradeVersion::ConsensusUpgrade(ConsensusUpgrade::PoW {
                initial_difficulty: difficulty.into(),
            }),
        ),
    ];

    let net_upgrades = NetUpgrades::initialize(upgrades).expect("valid netupgrades");

    // Internally this calls Consensus::new, which processes the genesis block
    // This should succeed because ChainConfigBuilder by default uses create_mainnet_genesis to
    // create the genesis_block, and this function creates a genesis block with
    // ConsenssuData::None, which agreess with the net_upgrades we defined above.
    let config = ChainConfigBuilder::new().with_net_upgrades(net_upgrades).build();
    let consensus = ConsensusBuilder::new().with_config(config).build();

    let mut btf = BlockTestFramework::with_consensus(consensus);

    // Let's create a block with random (invalid) PoW data and see that it fails the consensus
    // checks
    let prev_block = btf
        .get_block(
            btf.block_indexes
                .last()
                .expect("genesis should be there")
                .get_block_id()
                .clone(),
        )
        .unwrap()
        .unwrap();
    let mut random_invalid_block = btf.random_block(&prev_block, None);
    make_invalid_pow_block(&mut random_invalid_block, u128::MAX, difficulty.into())
        .expect("generate invalid block");
    let res = btf.add_special_block(random_invalid_block.clone());
    assert!(matches!(res, Err(BlockError::InvalidPoW)));

    // Now let's actually mine the block, i.e. find valid PoW and see that consensus checks pass
    let mut valid_block = random_invalid_block;
    let bits = difficulty.into();
    assert!(
        crate::detail::pow::work::mine(&mut valid_block, u128::MAX, bits, vec![])
            .expect("Unexpected conversion error")
    );
    assert!(btf.add_special_block(valid_block.clone()).is_ok());
}
