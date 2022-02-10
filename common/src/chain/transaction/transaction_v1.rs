pub use crate::chain::transaction::input::*;
pub use crate::chain::transaction::output::*;
pub use crate::chain::transaction::TransactionCreationError;
use crate::primitives::{id, Id, Idable};
use crypto::hash::StreamHasher;
use parity_scale_codec_derive::{Decode as DecodeDer, Encode as EncodeDer};

use super::Transaction;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, EncodeDer, DecodeDer)]
pub struct TransactionV1 {
    flags: u32,
    inputs: Vec<TxInput>,
    outputs: Vec<TxOutput>,
    lock_time: u32,
}

impl TransactionV1 {
    // This has to be the same its index in the Transaction enum
    pub const VERSION_BYTE: u8 = 0x01;

    pub fn new(
        flags: u32,
        inputs: Vec<TxInput>,
        outputs: Vec<TxOutput>,
        lock_time: u32,
    ) -> Result<Self, TransactionCreationError> {
        let tx = TransactionV1 {
            flags,
            inputs,
            outputs,
            lock_time,
        };
        Ok(tx)
    }

    pub fn is_replaceable(&self) -> bool {
        (self.flags & 1) != 0
    }

    pub fn get_flags(&self) -> u32 {
        self.flags
    }

    pub fn get_inputs(&self) -> &Vec<TxInput> {
        &self.inputs
    }

    pub fn get_outputs(&self) -> &Vec<TxOutput> {
        &self.outputs
    }

    pub fn get_lock_time(&self) -> u32 {
        self.lock_time
    }

    pub fn get_serialized_hash(&self) -> Id<Transaction> {
        Id::new(&id::hash_encoded(self))
    }
}

impl Idable<TransactionV1> for TransactionV1 {
    fn get_id(&self) -> Id<Self> {
        let mut hash_stream = id::DefaultHashAlgoStream::new();

        // Collect data from inputs, excluding witnesses
        let inputs: Vec<&OutPoint> = self.get_inputs().iter().map(TxInput::get_outpoint).collect();

        // Include the transaction format version first
        id::hash_encoded_to(&Self::VERSION_BYTE, &mut hash_stream);
        // Followed by transaction contents
        id::hash_encoded_to(&self.get_flags(), &mut hash_stream);
        id::hash_encoded_to(&inputs, &mut hash_stream);
        id::hash_encoded_to(&self.get_outputs(), &mut hash_stream);
        id::hash_encoded_to(&self.get_lock_time(), &mut hash_stream);
        Id::new(&hash_stream.finalize().into())
    }
}
