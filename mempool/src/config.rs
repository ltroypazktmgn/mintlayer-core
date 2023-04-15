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

use std::time::Duration;

pub type Time = Duration;

pub const ENABLE_RBF: bool = false;

// Number of times we try to add transaction if the tip moves suring validation
pub const MAX_TX_ADDITION_ATTEMPTS: usize = 3;

pub const ROLLING_FEE_BASE_HALFLIFE: Time = Duration::new(60 * 60 * 12, 1);
// TODO this will be defined elsewhere (some of limits.rs file)
pub const MAX_BLOCK_SIZE_BYTES: usize = 1_000_000;

pub const MAX_BIP125_REPLACEMENT_CANDIDATES: usize = 100;

// TODO this should really be taken from some global node settings
pub const RELAY_FEE_PER_BYTE: usize = 1;

pub const MAX_MEMPOOL_SIZE_BYTES: usize = 300_000_000;

pub const DEFAULT_MEMPOOL_EXPIRY: Duration = Duration::new(336 * 60 * 60, 0);

pub const ROLLING_FEE_DECAY_INTERVAL: Time = Duration::new(10, 0);
