//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::str::FromStr;

use anyhow::anyhow;
use log::info;
use tari_common_types::types::{FixedHash, PublicKey};
use tari_crypto::keys::PublicKey as PK;
use tari_dan_common_types::ShardId;
use tari_dan_wallet_sdk::apis::{jwt::JrpcPermission, key_manager};
use tari_template_builtin::ACCOUNT_NFT_TEMPLATE_ADDRESS;
use tari_template_lib::{
    args,
    crypto::RistrettoPublicKeyBytes,
    prelude::{NonFungibleAddress, NonFungibleId, ResourceAddress},
};
use tari_transaction::Transaction;
use tari_utilities::{ByteArray, Hashable};
use tari_wallet_daemon_client::types::{MintAccountNFTRequest, MintAccountNFTResponse};
use tokio::sync::broadcast;

use super::context::HandlerContext;
use crate::{
    handlers::get_account_or_default,
    services::{NewAccountNFTInfo, TransactionFinalizedEvent, TransactionSubmittedEvent, WalletEvent},
    DEFAULT_FEE,
};

const LOG_TARGET: &str = "tari::dan::wallet_daemon::handlers::nfts";

pub async fn handle_mint_account_nft(
    context: &HandlerContext,
    token: Option<String>,
    req: MintAccountNFTRequest,
) -> Result<CreateAccountNFTResponse, anyhow::Error> {
    let sdk = context.wallet_sdk();
    let key_manager_api = sdk.key_manager_api();
    sdk.jwt_api().check_auth(token, &[JrpcPermission::Admin])?;

    let account = get_account_or_default(req.account, &sdk.accounts_api())?;
    let inputs = sdk
        .substate_api()
        .locate_dependent_substates(&[&account.address])
        .await?;

    let signing_key_index = account.key_index;
    let signing_key = key_manager_api.derive_key(key_manager::TRANSACTION_BRANCH, signing_key_index)?;

    let owner_key = key_manager_api.next_key(key_manager::TRANSACTION_BRANCH)?;
    let owner_pk = PublicKey::from_secret_key(&owner_key.k);
    let owner_token =
        NonFungibleAddress::from_public_key(RistrettoPublicKeyBytes::from_bytes(owner_pk.as_bytes()).unwrap());

    info!(
        target: LOG_TARGET,
        "Creating new NFT resource address with owner address {}", req.owner_token,
    );

    // check if the component address already exists
    let component_address = new_component_address_from_parts(
        ACCOUNT_NFT_TEMPLATE_ADDRESS,
        owner_token
            .to_public_key()
            .unwrap_or_else(|| panic!("owner_token is not a valid public key: {}", owner_token))
            .as_hash(),
    );
    if wallet_sdk
        .substate_api()
        .scan_for_substate(&SubstateAddress::Component(account_address), None)
        .await
        .is_none()
    {
        let fee = req.fee.unwrap_or(DEFAULT_FEE);
        let transaction = Transaction::builder()
            .fee_transaction_pay_from_component(account.address.as_component_address().unwrap(), fee)
            .with_inputs(
                inputs
                    .iter()
                    .map(|addr| ShardId::from_address(&addr.address, addr.version))
                    .collect(),
            )
            .call_function(
                *ACCOUNT_NFT_TEMPLATE_ADDRESS,
                "create",
                args![req.owner_token, req.token_symbol],
            )
            .sign(&owner_key.k)
            .build();
    }

    let fee = req.fee.unwrap_or(DEFAULT_FEE);
    let transaction = Transaction::builder()
        .fee_transaction_pay_from_component(account.address.as_component_address().unwrap(), fee)
        .with_inputs(
            inputs
                .iter()
                .map(|addr| ShardId::from_address(&addr.address, addr.version))
                .collect(),
        )
        .call_function(
            *ACCOUNT_NFT_TEMPLATE_ADDRESS,
            "create",
            args![req.owner_token, req.token_symbol],
        )
        .sign(&owner_key.k)
        .build();

    let tx_hash = sdk.transaction_api().submit_transaction(transaction).await?;
    let mut events = context.notifier().subscribe();
    context.notifier().notify(TransactionSubmittedEvent {
        hash: tx_hash,
        new_account: None,
        new_account_nft: Some(NewAccountNFTInfo {
            resource_address,
            token_symbol: req.token_symbol,
        }),
    });

    let event = wait_for_result(&mut events, tx_hash).await?;
    if let Some(reject) = event.finalize.result.reject() {
        return Err(anyhow!(
            "Create NFT resource address from account {} was rejected: {}",
            account.name,
            reject
        ));
    }
    if let Some(reason) = event.transaction_failure {
        return Err(anyhow!(
            "Create NFT resource address transaction, from account {}, failed: {}",
            account.name,
            reason
        ));
    }

    // TODO: is there a more direct way to extract nft_id and resource address ??
    let (resource_address, nft_id) = event
        .finalize
        .events
        .iter()
        .filter(|e| e.topic().as_str() == "mint")
        .next()
        .map(|e| {
            (
                e.get_payload("resource_address").expect("Resource address not found"),
                e.get_payload("id").expect("NFTID not found"),
            )
        })
        .expect("NFT ID event payload not found");
    Ok(MintAccountNFTResponse {
        result: event.finalize,
        resource_address: ResourceAddress::from_str(&resource_address)?,
    })
}

// async fn handle_mint_nft(
//     context: &HandlerContext,
//     token: Option<String>,
//     req: MintAccountNFTRequest,
// ) -> Result<MintAccountNFTResponse, anyhow::Error> {
// }

// async fn handle_create_account_nft(context: &HandlerContext, )

async fn wait_for_result(
    events: &mut broadcast::Receiver<WalletEvent>,
    tx_hash: FixedHash,
) -> Result<TransactionFinalizedEvent, anyhow::Error> {
    loop {
        let wallet_event = events.recv().await?;
        match wallet_event {
            WalletEvent::TransactionFinalized(event) if event.hash == tx_hash => return Ok(event),
            _ => {},
        }
    }
}
