// Copyright (c) 2021 RBB S.r.l
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

use thiserror::Error;

use chainstate_types::pos_randomness::PoSRandomnessError;
use common::{chain::Block, primitives::Id};

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum ExtraConsensusDataError {
    #[error("Kernel output was not found in block: {0}")]
    PoSKernelOutputRetrievalFailed(Id<Block>),
    #[error("Randomness calculation failed: {0}")]
    PoSRandomnessCalculationFailed(#[from] PoSRandomnessError),
}
