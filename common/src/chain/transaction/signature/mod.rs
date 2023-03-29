// Copyright (c) 2021-2022 RBB S.r.l
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

use crypto::hash::StreamHasher;
use serialization::Encode;

use crate::{
    chain::{ChainConfig, TxInput},
    primitives::{
        id::{hash_encoded_to, DefaultHashAlgoStream},
        H256,
    },
};

use self::inputsig::{
    classical_multisig::{
        authorize_classical_multisig::ClassicalMultisigSigningError,
        multisig_partial_signature::PartiallySignedMultisigStructureError,
    },
    standard_signature::StandardInputSignature,
    InputWitness,
};

use super::{signed_transaction::SignedTransaction, Destination, Transaction, TxOutput};

pub mod inputsig;
pub mod sighashtype;

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum TransactionSigError {
    #[error("Invalid sighash value provided")]
    InvalidSigHashValue(u8),
    #[error("Invalid input index was provided (provided: `{0}` vs available: `{1}`")]
    InvalidInputIndex(usize, usize),
    #[error("Utxos count does not match inputs count (Utxo count: `{0}` vs inputs: `{1}`")]
    InvalidUtxoCountVsInputs(usize, usize),
    #[error("Invalid signature index was provided (provided: `{0}` vs available: `{1}`")]
    InvalidSignatureIndex(usize, usize),
    #[error("Requested signature hash without the presence of any inputs")]
    SigHashRequestWithoutInputs,
    #[error("Attempted to verify signatures for a transaction without inputs")]
    SignatureVerificationWithoutInputs,
    #[error("Attempted to verify signatures for a transaction without signatures")]
    SignatureVerificationWithoutSigs,
    #[error("Input corresponding to output number {0} does not exist (number of inputs is {1})")]
    InvalidOutputIndexForModeSingle(usize, usize),
    #[error("Decoding witness failed ")]
    DecodingWitnessFailed,
    #[error("Signature verification failed ")]
    SignatureVerificationFailed,
    #[error("Public key to address mismatch")]
    PublicKeyToAddressMismatch,
    #[error("Address authorization decoding failed")]
    AddressAuthDecodingFailed(String),
    #[error("Signature decoding failed")]
    InvalidSignatureEncoding,
    #[error("No signature!")]
    SignatureNotFound,
    #[error("Producing signature failed!")]
    ProducingSignatureFailed(crypto::key::SignatureError),
    #[error("Private key does not match with spender public key")]
    SpendeePrivatePublicKeyMismatch,
    #[error("AnyoneCanSpend should not use standard signatures, this place should be unreachable")]
    AttemptedToVerifyStandardSignatureForAnyoneCanSpend,
    #[error("AnyoneCanSpend should not use standard signatures, so producing a signature for it is not possible")]
    AttemptedToProduceSignatureForAnyoneCanSpend,
    #[error("Classical multisig signature attempted in uni-party function")]
    AttemptedToProduceClassicalMultisigSignatureForAnyoneCanSpend,
    #[error("Number of signatures does not match number of inputs")]
    InvalidWitnessCount,
    #[error("Invalid classical multisig challenge")]
    InvalidClassicalMultisig(#[from] PartiallySignedMultisigStructureError),
    #[error("Incomplete classical multisig signature(s)")]
    IncompleteClassicalMultisigSignature,
    #[error("Invalid classical multisig signature(s)")]
    InvalidClassicalMultisigSignature,
    #[error("The hash provided does not match the hash in the witness")]
    ClassicalMultisigWitnessHashMismatch,
    #[error("Producing classical multisig signing failed: {0}")]
    ClassicalMultisigSigningFailed(#[from] ClassicalMultisigSigningError),
    #[error("Standard signature creation failed. Invalid classical multisig authorization")]
    InvalidClassicalMultisigAuthorization,
    #[error("Standard signature creation failed. Incomplete classical multisig authorization")]
    IncompleteClassicalMultisigAuthorization,
    #[error("Unsupported yet!")]
    Unsupported,
}

pub fn signature_hash_for_inputs(
    stream: &mut DefaultHashAlgoStream,
    mode: sighashtype::SigHashType,
    inputs: &[TxInput],
    target_input: &TxInput,
) {
    match mode.inputs_mode() {
        sighashtype::InputsMode::CommitWhoPays => {
            hash_encoded_to(&(inputs.len() as u32), stream);
            for input in inputs {
                hash_encoded_to(&input.outpoint(), stream);
            }
        }
        sighashtype::InputsMode::AnyoneCanPay => {
            hash_encoded_to(&target_input.outpoint(), stream);
        }
    }
}

pub fn signature_hash_for_outputs(
    stream: &mut DefaultHashAlgoStream,
    mode: sighashtype::SigHashType,
    outputs: &[TxOutput],
    target_input_num: usize,
) -> Result<(), TransactionSigError> {
    match mode.outputs_mode() {
        sighashtype::OutputsMode::All => {
            hash_encoded_to(&outputs, stream);
        }
        sighashtype::OutputsMode::None => (),
        sighashtype::OutputsMode::Single => {
            let output = outputs.get(target_input_num).ok_or({
                TransactionSigError::InvalidInputIndex(target_input_num, outputs.len())
            })?;
            hash_encoded_to(&output, stream);
        }
    }
    Ok(())
}

trait SignatureHashableElement {
    fn signature_hash(
        &self,
        stream: &mut DefaultHashAlgoStream,
        mode: sighashtype::SigHashType,
        target_input: &TxInput,
        target_input_num: usize,
    ) -> Result<(), TransactionSigError>;
}

impl SignatureHashableElement for &[TxInput] {
    fn signature_hash(
        &self,
        stream: &mut DefaultHashAlgoStream,
        mode: sighashtype::SigHashType,
        target_input: &TxInput,
        _target_input_num: usize,
    ) -> Result<(), TransactionSigError> {
        match mode.inputs_mode() {
            sighashtype::InputsMode::CommitWhoPays => {
                hash_encoded_to(&(self.len() as u32), stream);
                for input in *self {
                    hash_encoded_to(&input.outpoint(), stream);
                }
            }
            sighashtype::InputsMode::AnyoneCanPay => {
                hash_encoded_to(&target_input.outpoint(), stream);
            }
        }
        Ok(())
    }
}

impl SignatureHashableElement for &[TxOutput] {
    fn signature_hash(
        &self,
        stream: &mut DefaultHashAlgoStream,
        mode: sighashtype::SigHashType,
        _target_input: &TxInput,
        target_input_num: usize,
    ) -> Result<(), TransactionSigError> {
        match mode.outputs_mode() {
            sighashtype::OutputsMode::All => {
                hash_encoded_to(self, stream);
            }
            sighashtype::OutputsMode::None => (),
            sighashtype::OutputsMode::Single => {
                let output = self.get(target_input_num).ok_or({
                    TransactionSigError::InvalidInputIndex(target_input_num, self.len())
                })?;
                hash_encoded_to(&output, stream);
            }
        }
        Ok(())
    }
}

fn hash_encoded_if_some<T: Encode>(val: &Option<T>, stream: &mut DefaultHashAlgoStream) {
    match val {
        Some(ref v) => hash_encoded_to(&v, stream),
        None => (),
    }
}

pub trait Signable {
    fn inputs(&self) -> Option<&[TxInput]>;
    fn outputs(&self) -> Option<&[TxOutput]>;
    fn version_byte(&self) -> Option<u8>;
    fn lock_time(&self) -> Option<u32>;
    fn flags(&self) -> Option<u32>;
}

pub trait Transactable: Signable {
    fn signatures(&self) -> Option<&[InputWitness]>;
}

impl Signable for Transaction {
    fn inputs(&self) -> Option<&[TxInput]> {
        Some(self.inputs())
    }

    fn outputs(&self) -> Option<&[TxOutput]> {
        Some(self.outputs())
    }

    fn version_byte(&self) -> Option<u8> {
        Some(self.version_byte())
    }

    fn lock_time(&self) -> Option<u32> {
        Some(self.lock_time())
    }

    fn flags(&self) -> Option<u32> {
        Some(self.flags())
    }
}

impl Signable for SignedTransaction {
    fn inputs(&self) -> Option<&[TxInput]> {
        Some(self.inputs())
    }

    fn outputs(&self) -> Option<&[TxOutput]> {
        Some(self.outputs())
    }

    fn version_byte(&self) -> Option<u8> {
        Some(self.version_byte())
    }

    fn lock_time(&self) -> Option<u32> {
        Some(self.lock_time())
    }

    fn flags(&self) -> Option<u32> {
        Some(self.flags())
    }
}

impl Transactable for SignedTransaction {
    fn signatures(&self) -> Option<&[InputWitness]> {
        Some(self.signatures())
    }
}

fn stream_signature_hash<T: Signable>(
    tx: &T,
    inputs_utxos: &[TxOutput],
    stream: &mut DefaultHashAlgoStream,
    mode: sighashtype::SigHashType,
    target_input_num: usize,
) -> Result<(), TransactionSigError> {
    // TODO: even though this works fine, we need to make this function
    // pull the inputs/outputs automatically through macros;
    // the current way is not safe and may produce issues in the future

    let inputs = match tx.inputs() {
        Some(ins) => ins,
        None => return Err(TransactionSigError::SigHashRequestWithoutInputs),
    };

    let outputs = tx.outputs().unwrap_or_default();

    let target_input = inputs.get(target_input_num).ok_or(
        TransactionSigError::InvalidInputIndex(target_input_num, inputs.len()),
    )?;

    hash_encoded_to(&mode.get(), stream);

    hash_encoded_if_some(&tx.version_byte(), stream);
    hash_encoded_if_some(&tx.flags(), stream);
    hash_encoded_if_some(&tx.lock_time(), stream);

    inputs.signature_hash(stream, mode, target_input, target_input_num)?;
    outputs.signature_hash(stream, mode, target_input, target_input_num)?;

    // Include utxos of the inputs to make it possible to verify the inputs scripts and amounts without downloading the full transactions
    if inputs.len() != inputs_utxos.len() {
        return Err(TransactionSigError::InvalidUtxoCountVsInputs(
            inputs_utxos.len(),
            inputs.len(),
        ));
    } else {
        hash_encoded_to(&inputs_utxos, stream);
    }

    // TODO: for P2SH add OP_CODESEPARATOR position
    hash_encoded_to(&u32::MAX, stream);

    Ok(())
}

pub fn signature_hash<T: Signable>(
    mode: sighashtype::SigHashType,
    tx: &T,
    inputs_utxos: &[TxOutput],
    input_num: usize,
) -> Result<H256, TransactionSigError> {
    let mut stream = DefaultHashAlgoStream::new();

    stream_signature_hash(tx, inputs_utxos, &mut stream, mode, input_num)?;

    let result = stream.finalize().into();
    Ok(result)
}

fn verify_standard_input_signature<T: Transactable>(
    chain_config: &ChainConfig,
    outpoint_destination: &Destination,
    witness: &StandardInputSignature,
    tx: &T,
    inputs_utxos: &[TxOutput],
    input_num: usize,
) -> Result<(), TransactionSigError> {
    let sighash = signature_hash(witness.sighash_type(), tx, inputs_utxos, input_num)?;
    witness.verify_signature(chain_config, outpoint_destination, &sighash)?;
    Ok(())
}

pub fn verify_signature<T: Transactable>(
    chain_config: &ChainConfig,
    outpoint_destination: &Destination,
    tx: &T,
    inputs_utxos: &[TxOutput],
    input_num: usize,
) -> Result<(), TransactionSigError> {
    let inputs = tx.inputs().ok_or(TransactionSigError::SignatureVerificationWithoutInputs)?;
    let sigs = tx.signatures().ok_or(TransactionSigError::SignatureVerificationWithoutSigs)?;
    let input_witness = sigs.get(input_num).ok_or(TransactionSigError::InvalidSignatureIndex(
        input_num,
        inputs.len(),
    ))?;

    match input_witness {
        InputWitness::NoSignature(_) => match outpoint_destination {
            Destination::Address(_)
            | Destination::PublicKey(_)
            | Destination::ScriptHash(_)
            | Destination::ClassicMultisig(_) => {
                return Err(TransactionSigError::SignatureNotFound)
            }
            Destination::AnyoneCanSpend => {}
        },
        InputWitness::Standard(witness) => verify_standard_input_signature(
            chain_config,
            outpoint_destination,
            witness,
            tx,
            inputs_utxos,
            input_num,
        )?,
    }
    Ok(())
}

#[cfg(test)]
mod tests;
