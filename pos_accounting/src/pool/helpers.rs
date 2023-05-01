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

use common::{
    chain::{DelegationId, OutPoint, PoolId},
    primitives::{
        id::{hash_encoded_to, DefaultHashAlgoStream},
        Amount,
    },
};
use crypto::hash::StreamHasher;

use crate::{Error, PoSAccountingView};

pub fn pool_id_preimage_suffix() -> u32 {
    // arbitrary, we use this to create different values when hashing with no security requirements
    0
}

pub fn delegation_id_preimage_suffix() -> u32 {
    // arbitrary, we use this to create different values when hashing with no security requirements
    1
}

pub fn make_pool_id(input0_outpoint: &OutPoint) -> PoolId {
    let mut hasher = DefaultHashAlgoStream::new();
    hash_encoded_to(&input0_outpoint, &mut hasher);
    // 0 is arbitrary here, we use this as prefix to use this information again
    hash_encoded_to(&pool_id_preimage_suffix(), &mut hasher);
    PoolId::new(hasher.finalize().into())
}

pub fn make_delegation_id(input0_outpoint: &OutPoint) -> DelegationId {
    let mut hasher = DefaultHashAlgoStream::new();
    hash_encoded_to(&input0_outpoint, &mut hasher);
    // 1 is arbitrary here, we use this as prefix to use this information again
    hash_encoded_to(&delegation_id_preimage_suffix(), &mut hasher);
    DelegationId::new(hasher.finalize().into())
}

// FIXME: staker balance as a field in db
pub fn calculate_staker_balance<V: PoSAccountingView<Error = crate::Error>>(
    view: &V,
    pool_id: PoolId,
) -> Result<Option<Amount>, <V as PoSAccountingView>::Error> {
    view.get_pool_balance(pool_id)?
        .map(|pool_balance| {
            let total_delegations_shares = view.get_pool_delegations_shares(pool_id)?.map_or(
                Ok(Amount::ZERO),
                |delegation_shares| {
                    delegation_shares.values().try_fold(Amount::ZERO, |acc, v| {
                        (acc + *v).ok_or(Error::DelegationSharesAdditionError)
                    })
                },
            )?;

            (pool_balance - total_delegations_shares)
                .ok_or(Error::InvariantErrorDelegationSharesMoreThanPoolBalance)
        })
        .transpose()
}
