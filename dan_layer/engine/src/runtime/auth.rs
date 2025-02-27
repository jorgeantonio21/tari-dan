//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{fmt::Display, sync::Arc};

use tari_template_lib::models::{NonFungibleAddress, ProofId};

#[derive(Debug, Clone)]
pub struct AuthParams {
    pub initial_ownership_proofs: Vec<NonFungibleAddress>,
}

#[derive(Debug, Clone)]
pub struct AuthorizationScope {
    /// Virtual proofs are system-issued non-fungibles that exist for no longer than the execution e.g. derived from
    /// the transaction signer public key
    virtual_proofs: Arc<Vec<NonFungibleAddress>>,

    /// Resource-based proofs
    proofs: Vec<ProofId>,
}

impl AuthorizationScope {
    pub fn new(virtual_proofs: Vec<NonFungibleAddress>) -> Self {
        Self {
            virtual_proofs: Arc::new(virtual_proofs),
            proofs: vec![],
        }
    }

    pub fn virtual_proofs(&self) -> &[NonFungibleAddress] {
        &self.virtual_proofs
    }

    pub fn proofs(&self) -> &[ProofId] {
        &self.proofs
    }

    pub fn add_proof(&mut self, proof_id: ProofId) {
        self.proofs.push(proof_id);
    }

    pub fn remove_proof(&mut self, proof_id: &ProofId) -> Option<ProofId> {
        self.proofs
            .iter()
            .position(|p| p == proof_id)
            .map(|i| self.proofs.remove(i))
    }

    pub(super) fn update_from_child(&mut self, child: AuthorizationScope) {
        self.proofs.extend(child.proofs);
    }
}

impl Display for AuthorizationScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Virtual: [")?;
        for proof in self.virtual_proofs.iter() {
            write!(f, "{}", proof)?;
        }
        write!(f, "], Proofs: [")?;
        for proof in &self.proofs {
            write!(f, "{}", proof)?;
        }
        write!(f, "]")
    }
}
