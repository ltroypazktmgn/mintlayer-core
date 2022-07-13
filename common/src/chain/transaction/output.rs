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

use crate::{
    address::pubkeyhash::PublicKeyHash,
    primitives::{Amount, Id},
};
use script::Script;
use serialization::{Decode, Encode};

use self::stakelock::LockedStateData;

pub mod stakelock;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum Destination {
    #[codec(index = 0)]
    Address(PublicKeyHash), // Address type to be added
    #[codec(index = 1)]
    PublicKey(crypto::key::PublicKey), // Key type to be added
    #[codec(index = 2)]
    ScriptHash(Id<Script>),
    #[codec(index = 3)]
    AnyoneCanSpend, // zero verification; used primarily for testing. Never use this for real money
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub enum OutputPurpose {
    Transfer(Destination),
    StakeLock(LockedStateData),
}

impl OutputPurpose {
    pub fn destination(&self) -> &Destination {
        match self {
            OutputPurpose::Transfer(d) => d,
            OutputPurpose::StakeLock(d) => d.owner(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct TxOutput {
    value: Amount,
    purpose: OutputPurpose,
}

impl TxOutput {
    pub fn new(value: Amount, purpose: OutputPurpose) -> Self {
        TxOutput { value, purpose }
    }

    pub fn value(&self) -> Amount {
        self.value
    }

    pub fn purpose(&self) -> &OutputPurpose {
        &self.purpose
    }
}
