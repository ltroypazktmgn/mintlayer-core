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

//! Application-level interface for the persistent wallet storage.

mod internal;
mod is_transaction_seal;
pub mod schema;

use common::chain::{OutPoint, Transaction};
use common::primitives::Id;
pub use internal::Store;
use std::path::PathBuf;

use crate::internal::StoreTxRw;
use utxo::Utxo;
use wallet_types::wallet_tx::WalletTx;

/// Possibly failing result of wallet storage query
pub type Result<T> = storage::Result<T>;
pub type Error = storage::Error;

/// Queries on persistent wallet data
pub trait WalletStorageRead {
    /// Get storage version
    fn get_storage_version(&self) -> Result<u32>;
    fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<Utxo>>;
    fn get_transaction(&self, id: &Id<Transaction>) -> Result<Option<WalletTx>>;
}

/// Modifying operations on persistent wallet data
pub trait WalletStorageWrite: WalletStorageRead {
    /// Set storage version
    fn set_storage_version(&mut self, version: u32) -> Result<()>;
    fn set_utxo(&mut self, outpoint: &OutPoint, entry: Utxo) -> Result<()>;
    fn del_utxo(&mut self, outpoint: &OutPoint) -> Result<()>;
    fn set_transaction(&mut self, id: &Id<Transaction>, tx: &WalletTx) -> Result<()>;
}

/// Marker trait for types where read/write operations are run in a transaction
pub trait IsTransaction: is_transaction_seal::Seal {}

/// Operations on read-only transactions
pub trait TransactionRo: WalletStorageRead + IsTransaction {
    /// Close the transaction
    fn close(self);
}

/// Operations on read-write transactions
pub trait TransactionRw: WalletStorageWrite + IsTransaction {
    /// Abort the transaction
    fn abort(self);

    /// Commit the transaction
    fn commit(self) -> crate::Result<()>;
}

/// Support for transactions over wallet storage
pub trait Transactional<'t> {
    /// Associated read-only transaction type.
    type TransactionRo: TransactionRo + 't;

    /// Associated read-write transaction type.
    type TransactionRw: TransactionRw + 't;

    /// Start a read-only transaction.
    fn transaction_ro<'s: 't>(&'s self) -> crate::Result<Self::TransactionRo>;

    /// Start a read-write transaction.
    fn transaction_rw<'s: 't>(&'s self, size: Option<usize>) -> crate::Result<Self::TransactionRw>;
}

pub trait WalletStorage: WalletStorageWrite + for<'tx> Transactional<'tx> + Send {}

impl Store<storage_sqlite::Sqlite> {
    /// Create a default storage (mostly for testing, may want to remove this later)
    pub fn new_in_memory() -> crate::Result<Self> {
        Self::new(storage_sqlite::Sqlite::new_in_memory())
    }

    pub fn new_from_path(path: PathBuf) -> crate::Result<Self> {
        Self::new(storage_sqlite::Sqlite::new(path))
    }
}

pub type WalletStorageImpl = Store<storage_sqlite::Sqlite>;
pub type WalletStorageTxRwImpl<'st> = StoreTxRw<'st, storage_sqlite::Sqlite>;
