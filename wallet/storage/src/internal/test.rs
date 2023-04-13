// Copyright (c) 2023 RBB S.r.l
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

use super::*;
use crate::DefaultBackend;
use common::chain::tokens::OutputValue;
use common::chain::{Destination, OutPoint, OutPointSourceId, OutputPurpose, TxOutput};
use common::primitives::{Amount, Id, H256};
use crypto::key::extended::{ExtendedKeyKind, ExtendedPrivateKey};
use crypto::key::{KeyKind, PrivateKey};
use crypto::random::Rng;
use rstest::rstest;
use test_utils::random::{make_seedable_rng, Seed};

#[test]
fn storage_get_default_version_in_tx() {
    utils::concurrency::model(|| {
        let mut store = Store::new(DefaultBackend::new_in_memory()).unwrap();
        store.set_storage_version(1).unwrap();
        let vtx = store.transaction_ro().unwrap().get_storage_version().unwrap();
        let vst = store.get_storage_version().unwrap();
        assert_eq!(vtx, 1, "Default storage version wrong");
        assert_eq!(vtx, vst, "Transaction and non-transaction inconsistency");
    })
}

#[cfg(not(loom))]
#[rstest]
#[trace]
#[case(Seed::from_entropy())]
fn read_write_utxo_in_db_transaction(#[case] seed: Seed) {
    let mut rng = make_seedable_rng(seed);
    let mut db_interface = Store::new(DefaultBackend::new_in_memory()).unwrap();

    // generate an account id
    let account_id = AccountId::new_from_xpub(
        &ExtendedPrivateKey::new_from_rng(&mut rng, ExtendedKeyKind::Secp256k1Schnorr).1,
    );

    // generate a utxo and outpoint
    let (_, pub_key) = PrivateKey::new_from_rng(&mut rng, KeyKind::Secp256k1Schnorr);
    let output = TxOutput::Transfer(
        OutputValue::Coin(Amount::from_atoms(rng.gen_range(0..(u128::MAX - 1)))),
        Destination::PublicKey(pub_key),
    );
    let utxo = Utxo::new_for_mempool(output, false);
    let outpoint = OutPoint::new(
        OutPointSourceId::Transaction(Id::new(H256::random_using(&mut rng))),
        0,
    );

    let account_outpoint_id = AccountOutPointId::new(account_id, outpoint);

    assert!(db_interface.set_utxo(&account_outpoint_id, utxo.clone()).is_ok());
    assert_eq!(db_interface.get_utxo(&account_outpoint_id), Ok(Some(utxo)));
    assert!(db_interface.del_utxo(&account_outpoint_id).is_ok());
    assert_eq!(db_interface.get_utxo(&account_outpoint_id), Ok(None));
}
