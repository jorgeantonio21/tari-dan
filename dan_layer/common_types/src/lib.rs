// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod proto;
pub mod storage;

pub mod optional;
pub mod serde_with;
mod template_id;

use std::cmp::Ordering;

use ::serde::{Deserialize, Serialize};
use borsh::{BorshDeserialize, BorshSerialize};
use tari_common_types::types::{FixedHash, FixedHashSizeError};
use tari_utilities::byte_array::ByteArray;
pub use template_id::TemplateId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct ObjectId(#[serde(deserialize_with = "serde_with::hex::deserialize")] pub [u8; 32]);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ShardId(#[serde(with = "serde_with::hex")] pub [u8; 32]);

impl ShardId {
    pub fn to_le_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn new(id: FixedHash) -> Self {
        let mut v = [0u8; 32];
        v.copy_from_slice(id.as_slice());
        Self(v)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FixedHashSizeError> {
        FixedHash::try_from(bytes).map(Self::new)
    }

    pub fn zero() -> Self {
        Self::new(FixedHash::default())
    }
}

impl From<[u8; 32]> for ShardId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl PartialOrd for ShardId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for ShardId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum SubstateChange {
    Create,
    Destroy,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
pub enum SubstateState {
    DoesNotExist,
    Up { created_by: PayloadId, data: Vec<u8> },
    Down { deleted_by: PayloadId },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectClaim {}

impl ObjectClaim {
    pub fn is_valid(&self, _payload: PayloadId) -> bool {
        true
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
pub struct PayloadId {
    #[serde(deserialize_with = "serde_with::hex::deserialize")]
    id: [u8; 32],
}

impl PayloadId {
    pub fn new(id: FixedHash) -> Self {
        let mut v = [0u8; 32];
        v.copy_from_slice(id.as_slice());
        Self { id: v }
    }

    pub fn zero() -> Self {
        Self::new(FixedHash::default())
    }

    pub fn as_slice(&self) -> &[u8] {
        self.id.as_slice()
    }

    pub fn into_array(self) -> [u8; 32] {
        self.id
    }
}
