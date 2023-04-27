//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use serde::{Deserialize, Serialize};
use tari_template_lib::Hash;

use crate::{
    events::Event,
    fees::{FeeCostBreakdown, FeeReceipt},
    instruction_result::InstructionResult,
    logs::LogEntry,
    substate::SubstateDiff,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// The finalized result to commit. If the fee transaction succeeds but the transaction fails, this will be accept.
    pub finalize: FinalizeResult,
    /// If the fee transaction passes but the transaction fails, this will be the reason for the transaction failure.
    pub transaction_failure: Option<RejectReason>,
    /// The fee payment summary including the Resource containing the fees taken during execution.
    pub fee_receipt: Option<FeeReceipt>,
}

impl ExecuteResult {
    pub fn expect_success(&self) -> &SubstateDiff {
        let diff = self.expect_finalization_success();

        if let Some(ref reason) = self.transaction_failure {
            panic!("Transaction failed: {}", reason);
        }

        diff
    }

    pub fn expect_failure(&self) -> &RejectReason {
        match self.finalize.result {
            TransactionResult::Accept(_) => panic!("Transaction succeeded"),
            TransactionResult::Reject(ref reason) => reason,
        }
    }

    pub fn expect_transaction_failure(&self) -> &RejectReason {
        if let Some(ref reason) = self.transaction_failure {
            reason
        } else {
            panic!("Transaction succeeded");
        }
    }

    pub fn expect_finalization_success(&self) -> &SubstateDiff {
        match self.finalize.result {
            TransactionResult::Accept(ref diff) => diff,
            TransactionResult::Reject(ref reason) => panic!("Transaction failed: {}", reason),
        }
    }

    pub fn expect_fees_paid_in_full(&self) -> &FeeReceipt {
        let receipt = self.fee_receipt.as_ref().expect("No fee receipt");
        assert!(receipt.is_paid_in_full(), "Fees not paid in full");
        receipt
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeResult {
    pub transaction_hash: Hash,
    pub logs: Vec<LogEntry>,
    pub events: Vec<Event>,
    // TOOD: Remove from FinalizeResult
    pub execution_results: Vec<InstructionResult>,
    pub result: TransactionResult,
    pub cost_breakdown: Option<FeeCostBreakdown>,
}

impl FinalizeResult {
    pub fn new(
        transaction_hash: Hash,
        logs: Vec<LogEntry>,
        events: Vec<Event>,
        result: TransactionResult,
        cost_breakdown: FeeCostBreakdown,
    ) -> Self {
        Self {
            transaction_hash,
            logs,
            execution_results: Vec::new(),
            result,
            events,
            cost_breakdown: Some(cost_breakdown),
        }
    }

    pub fn reject(transaction_hash: Hash, reason: RejectReason) -> Self {
        Self {
            transaction_hash,
            logs: vec![],
            events: vec![],
            execution_results: Vec::new(),
            result: TransactionResult::Reject(reason),
            cost_breakdown: None,
        }
    }

    pub fn is_accept(&self) -> bool {
        matches!(self.result, TransactionResult::Accept(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionResult {
    Accept(SubstateDiff),
    Reject(RejectReason),
}

impl TransactionResult {
    pub fn is_accept(&self) -> bool {
        matches!(self, Self::Accept(_))
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, Self::Reject(_))
    }

    pub fn accept(&self) -> Option<&SubstateDiff> {
        match self {
            Self::Accept(substate_diff) => Some(substate_diff),
            Self::Reject(_) => None,
        }
    }

    pub fn reject(&self) -> Option<&RejectReason> {
        match self {
            Self::Accept(_) => None,
            Self::Reject(reject_result) => Some(reject_result),
        }
    }

    pub fn expect(self, msg: &str) -> SubstateDiff {
        match self {
            Self::Accept(substate_diff) => substate_diff,
            Self::Reject(reject_result) => {
                panic!("{}: {:?}", msg, reject_result);
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RejectReason {
    ShardsNotPledged(String),
    ExecutionFailure(String),
    PreviousQcRejection,
    ShardPledgedToAnotherPayload(String),
    ShardRejected(String),
    FeeTransactionFailed,
    FeesNotPaid(String),
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RejectReason::ShardsNotPledged(msg) => write!(f, "Shards not pledged: {}", msg),
            RejectReason::ExecutionFailure(msg) => write!(f, "Execution failure: {}", msg),
            RejectReason::PreviousQcRejection => write!(f, "Previous QC was a rejection"),
            RejectReason::ShardPledgedToAnotherPayload(msg) => write!(f, "Shard pledged to another payload: {}", msg),
            RejectReason::ShardRejected(msg) => write!(f, "Shard was rejected: {}", msg),
            RejectReason::FeeTransactionFailed => write!(f, "Fee transaction failed"),
            RejectReason::FeesNotPaid(msg) => write!(f, "Fee not paid: {}", msg),
        }
    }
}
