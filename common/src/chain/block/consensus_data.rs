// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::chain::TxInput;
use crate::chain::{signature::Transactable, TxOutput};
use crate::primitives::Compact;
use crate::Uint256;
use crypto::vrf::VRFReturn;
use serialization::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ConsensusData {
    #[codec(index = 0)]
    None,
    #[codec(index = 1)]
    PoW(PoWData),
    #[codec(index = 2)]
    PoS(PoSData),
}

pub struct BlockRewardTransactable<'a> {
    pub(in crate::chain) inputs: Option<&'a [TxInput]>,
    pub(in crate::chain) outputs: Option<&'a [TxOutput]>,
}

impl<'a> Transactable for BlockRewardTransactable<'a> {
    fn inputs(&self) -> Option<&[TxInput]> {
        self.inputs
    }

    fn outputs(&self) -> Option<&[TxOutput]> {
        self.outputs
    }

    fn version_byte(&self) -> Option<u8> {
        None
    }

    fn lock_time(&self) -> Option<u32> {
        None
    }

    fn flags(&self) -> Option<u32> {
        None
    }
}

impl ConsensusData {
    pub fn derive_transactable(&self) -> BlockRewardTransactable {
        match self {
            ConsensusData::None => BlockRewardTransactable {
                inputs: None,
                outputs: None,
            },
            ConsensusData::PoW(ref pow_data) => BlockRewardTransactable {
                inputs: None,
                outputs: Some(pow_data.outputs()),
            },
            ConsensusData::PoS(pos_data) => BlockRewardTransactable {
                inputs: Some(&pos_data.kernel_inputs),
                outputs: Some(&pos_data.reward_outputs),
            },
        }
    }

    pub fn get_block_proof(&self) -> Option<Uint256> {
        match self {
            ConsensusData::None => Some(1u64.into()),
            ConsensusData::PoW(ref pow_data) => pow_data.get_block_proof(),
            ConsensusData::PoS(_) => Some(1u64.into()),
        }
    }
}

/// Fake PoS just to test spending block rewards; will be removed at some point in the future
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PoSData {
    kernel_inputs: Vec<TxInput>,
    reward_outputs: Vec<TxOutput>,
    vrf_data: VRFReturn,
    bits: Compact,
}

impl PoSData {
    pub fn new(
        kernel_inputs: Vec<TxInput>,
        reward_outputs: Vec<TxOutput>,
        vrf_data: VRFReturn,
        bits: Compact,
    ) -> Self {
        Self {
            kernel_inputs,
            reward_outputs,
            vrf_data,
            bits,
        }
    }

    pub fn kernel_inputs(&self) -> &Vec<TxInput> {
        &self.kernel_inputs
    }

    pub fn reward_outputs(&self) -> &Vec<TxOutput> {
        &self.reward_outputs
    }

    pub fn bits(&self) -> &Compact {
        &self.bits
    }

    pub fn vrf_data(&self) -> &VRFReturn {
        &self.vrf_data
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Encode, Decode)]
pub struct PoWData {
    bits: Compact,
    nonce: u128,
    reward_outputs: Vec<TxOutput>,
}

impl PoWData {
    pub fn new(bits: Compact, nonce: u128, reward_outputs: Vec<TxOutput>) -> Self {
        PoWData {
            bits,
            nonce,
            reward_outputs,
        }
    }
    pub fn bits(&self) -> Compact {
        self.bits
    }

    pub fn nonce(&self) -> u128 {
        self.nonce
    }

    pub fn outputs(&self) -> &[TxOutput] {
        &self.reward_outputs
    }

    pub fn update_nonce(&mut self, nonce: u128) {
        self.nonce = nonce;
    }

    pub fn get_block_proof(&self) -> Option<Uint256> {
        // 2**256 / (target + 1) == ~target / (target+1) + 1    (eqn shamelessly stolen from bitcoind)
        let target: Uint256 = self.bits.try_into().ok()?;
        let mut ret = !target;
        let mut ret1 = target;
        ret1.increment();
        ret = ret / ret1;
        ret.increment();
        Some(ret)
    }
}
