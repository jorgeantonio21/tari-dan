//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause
use std::collections::{BTreeMap, HashMap};

use tari_dan_engine::runtime::{ActionIdent, RuntimeError};
use tari_template_lib::{
    args,
    args::ComponentAction,
    auth::{
        AccessRule,
        ComponentAccessRules,
        OwnerRule,
        RequireRule,
        ResourceAccessRules,
        ResourceAuthAction,
        RestrictedAccessRule,
        RuleRequirement,
    },
    models::{Amount, ComponentAddress, Metadata, NonFungibleId, ResourceAddress, VaultId},
};
use tari_template_test_tooling::{
    support::assert_error::{
        assert_access_denied_for_action,
        assert_insufficient_funds_for_action,
        assert_reject_reason,
    },
    TemplateTest,
};
use tari_transaction::Transaction;

mod component_access_rules {
    use tari_template_lib::rule;

    use super::*;

    #[test]
    fn it_restricts_component_methods() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner1_proof, _, owner1_key) = test.create_owner_proof();
        let (owner2_proof, _, owner2_key) = test.create_owner_proof();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let owner_rule = AccessRule::Restricted(
            RestrictedAccessRule::Require(RequireRule::Require(owner1_proof.clone().into())).or(
                RestrictedAccessRule::Require(RequireRule::Require(owner2_proof.clone().into())),
            ),
        );

        let component_rules = ComponentAccessRules::new()
            .add_method_rule("set_value", owner_rule.clone())
            .default(AccessRule::DenyAll);

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner
                    OwnerRule::ByAccessRule(owner_rule),
                    // Component
                    component_rules,
                    // Resource
                    ResourceAccessRules::deny_all(),
                    // Badge recall rule
                    AccessRule::DenyAll,
                ])
                .sign(&owner1_key)
                .build(),
            // Because we deny_all on deposits, we need to supply the owner proof to be able to deposit the initial
            // tokens into the new vaults
            vec![owner1_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "set_value", args![1])
                .sign(&owner2_key)
                .build(),
            vec![owner2_proof],
        );

        let (unauth_proof, _, unauth_key) = test.create_owner_proof();

        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "set_value", args![1])
                .sign(&unauth_key)
                .build(),
            vec![unauth_proof],
        );

        assert_access_denied_for_action(reason, ActionIdent::ComponentCallMethod {
            component_address,
            method: "set_value".to_string(),
        });
    }

    #[test]
    fn it_allows_owner_to_update_component_access_rules() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();
        let (user_proof, _, user_key) = test.create_owner_proof();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner
                    OwnerRule::OwnedBySigner,
                    // Component
                    ComponentAccessRules::new().default(AccessRule::DenyAll),
                    // Resource
                    ResourceAccessRules::deny_all(),
                    // Badge recall rule
                    AccessRule::DenyAll
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        // Access Denied
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "set_value", args![1])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_access_denied_for_action(reason, ActionIdent::ComponentCallMethod {
            component_address,
            method: "set_value".to_string(),
        });

        // Allow user to call set_value
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "set_component_access_rules", args![
                    ComponentAccessRules::new()
                        .add_method_rule("set_value", rule!(non_fungible(user_proof.clone())))
                        .default(AccessRule::DenyAll)
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof],
        );

        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "set_value", args![1])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "set_component_access_rules", args![
                    ComponentAccessRules::new().default(AccessRule::AllowAll)
                ])
                .sign(&user_key)
                .build(),
            vec![user_proof],
        );
    }

    #[test]
    fn it_prevents_access_rule_modification_if_owner_is_none() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner
                    OwnerRule::None,
                    // Component
                    ComponentAccessRules::new().default(AccessRule::AllowAll),
                    // Resource
                    ResourceAccessRules::new(),
                    // Badge recall rule
                    AccessRule::DenyAll,
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        // Owner cannot set access rules
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "set_component_access_rules", args![
                    ComponentAccessRules::new().default(AccessRule::AllowAll)
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof],
        );

        assert_reject_reason(reason, RuntimeError::AccessDeniedOwnerRequired {
            action: ComponentAction::SetAccessRules.into(),
        });
    }
}

mod resource_access_rules {
    use tari_dan_engine::runtime::LockError;
    use tari_template_lib::rule;

    use super::*;

    #[test]
    fn it_denies_actions_on_resource() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_account, owner_proof, owner_key) = test.create_empty_account();
        let (user_proof, _, user_key) = test.create_owner_proof();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner
                    OwnerRule::OwnedBySigner,
                    // Component
                    ComponentAccessRules::new().default(AccessRule::AllowAll),
                    // Resource
                    ResourceAccessRules::new().withdrawable(AccessRule::DenyAll),
                    // Badge recall rule
                    AccessRule::DenyAll,
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        // User cannot get tokens
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);

        // Owner can get tokens
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        // Owner gives user permission to withdraw tokens
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "set_tokens_access_rules", args![
                    ResourceAccessRules::new().withdrawable(rule!(non_fungible(user_proof.clone())))
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof],
        );

        // User can get tokens, and deposit them in the owners account (deposit is default allow)
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof],
        );
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn it_denies_recall_for_owner() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();
        let (user_account, user_proof, _) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner - Everyone!
                    OwnerRule::ByAccessRule(AccessRule::AllowAll),
                    // Component
                    ComponentAccessRules::new().default(AccessRule::AllowAll),
                    // Resource
                    ResourceAccessRules::new().withdrawable(rule!(non_fungible(user_proof.clone()))),
                    // Badge recall rule
                    AccessRule::DenyAll
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        // Give the user a withdraw and deposit badge
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_badge_by_name", args!["withdraw"])
                .put_last_instruction_output_on_workspace("withdraw_perm")
                .call_method(component_address, "take_badge_by_name", args!["deposit"])
                .put_last_instruction_output_on_workspace("deposit_perm")
                .call_method(user_account, "deposit", args![Workspace("withdraw_perm")])
                .call_method(user_account, "deposit", args![Workspace("deposit_perm")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let badge_vault: VaultId = test.extract_component_value(component_address, "$.badges");
        let badge_resource = *test
            .read_only_state_store()
            .get_vault(&badge_vault)
            .unwrap()
            .resource_address();
        let vaults: HashMap<ResourceAddress, VaultId> = test.extract_component_value(user_account, "$.vaults");
        let user_badge_vault_id = vaults[&badge_resource];

        // Now try recall them. This won't succeed because recall only respects access rules not ownership, so the call
        // is denied for the owner.
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "recall_badge", args![
                    user_badge_vault_id,
                    "withdraw"
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Recall);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn it_allows_resource_access_with_badge_then_recall() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "using_badge_rules", args![])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        let vault: VaultId = test.extract_component_value(component_address, "$.badges");
        // Find the resource address for the badge from the output substates
        let badge_resource = *test
            .read_only_state_store()
            .get_vault(&vault)
            .unwrap()
            .resource_address();
        let vault: VaultId = test.extract_component_value(component_address, "$.tokens");
        let token_resource = *test
            .read_only_state_store()
            .get_vault(&vault)
            .unwrap()
            .resource_address();

        // User cannot get the tokens
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);

        // Give the user a withdraw and deposit badge
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_badge_by_name", args!["withdraw"])
                .put_last_instruction_output_on_workspace("withdraw_perm")
                .call_method(component_address, "take_badge_by_name", args!["deposit"])
                .put_last_instruction_output_on_workspace("deposit_perm")
                .call_method(user_account, "deposit", args![Workspace("withdraw_perm")])
                .call_method(user_account, "deposit", args![Workspace("deposit_perm")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        // User can take tokens
        let result = test.execute_expect_success(
            Transaction::builder()
                .call_method(user_account, "create_proof_by_non_fungible_ids", args![
                    badge_resource,
                    vec![
                        NonFungibleId::from_string("withdraw"),
                        NonFungibleId::from_string("deposit")
                    ]
                ])
                .put_last_instruction_output_on_workspace("proof")
                .call_method(component_address, "get_nft_data_using_proof", args![Workspace("proof")])
                .call_method(component_address, "take_tokens_using_proof", args![
                    Workspace("proof"),
                    Amount(10)
                ])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        let badge_data = result.finalize.execution_results[2].decode::<Vec<Metadata>>().unwrap();
        assert!(badge_data.iter().all(|b| b.contains_key("colour")));

        let vaults: BTreeMap<ResourceAddress, VaultId> = test.extract_component_value(user_account, "$.vaults");
        let user_badge_vault_id = vaults[&badge_resource];

        // Recall badge
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "recall_badge", args![
                    user_badge_vault_id,
                    "withdraw"
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        // User can no longer withdraw tokens
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(user_account, "create_proof_for_resource", args![badge_resource])
                .put_last_instruction_output_on_workspace("proof")
                .call_method(user_account, "withdraw", args![token_resource, Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .sign(&user_key)
                .build(),
            vec![user_proof],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);
    }

    #[test]
    fn it_allows_access_for_proofs_by_amount() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "using_resource_rules", args![])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let access_rules_component = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        // Find the resource address for the badge from the output substates
        let badge_resource = result
            .finalize
            .result
            .accept()
            .unwrap()
            .up_iter()
            .filter_map(|(addr, s)| s.substate_value().as_resource().map(|r| (addr, r)))
            .filter(|(_, r)| r.resource_type().is_non_fungible())
            .map(|(addr, _)| addr.as_resource_address().unwrap())
            .next()
            .unwrap();

        // User cannot get the tokens
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(access_rules_component, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);

        // Give the user a badge
        test.execute_expect_success(
            Transaction::builder()
                .call_method(access_rules_component, "mint_new_badge", args![])
                .put_last_instruction_output_on_workspace("permission")
                .call_method(user_account, "deposit", args![Workspace("permission")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        // User can take tokens
        test.execute_expect_success(
            Transaction::builder()
                .call_method(user_account, "create_proof_by_amount", args![badge_resource, Amount(1)])
                .put_last_instruction_output_on_workspace("proof")
                .call_method(access_rules_component, "take_tokens_using_proof", args![
                    Workspace("proof"),
                    Amount(10)
                ])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );
    }

    #[test]
    fn it_locks_resources_used_in_proofs() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_account, owner_proof, owner_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_configured_rules", args![
                    // Owner
                    OwnerRule::OwnedBySigner,
                    // Component
                    ComponentAccessRules::new(),
                    // Resource
                    ResourceAccessRules::new(),
                    // Badge recall rule
                    AccessRule::DenyAll
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        // Find the resource address for the tokens from the output substates
        let token_resource = result
            .finalize
            .result
            .accept()
            .unwrap()
            .up_iter()
            .filter_map(|(addr, s)| s.substate_value().as_resource().map(|r| (addr, r)))
            .filter(|(_, r)| r.resource_type().is_fungible())
            .map(|(addr, _)| addr.as_resource_address().unwrap())
            .next()
            .unwrap();

        // Take some tokens, generate a proof from the bucket (locking them up), and then try withdrawing them
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(1000)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .call_method(owner_account, "create_proof_by_amount", args![
                    token_resource,
                    Amount(1000)
                ])
                .put_last_instruction_output_on_workspace("proof")
                .call_method(owner_account, "withdraw", args![token_resource, Amount(1000)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        assert_insufficient_funds_for_action(reason);

        // Drop the proof before withdraw/deposit
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(1000)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .call_method(owner_account, "create_proof_by_amount", args![
                    token_resource,
                    Amount(1000)
                ])
                .put_last_instruction_output_on_workspace("proof")
                .drop_all_proofs_in_workspace()
                .call_method(owner_account, "withdraw", args![token_resource, Amount(1000)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );
    }

    #[test]
    fn it_permits_cross_template_calls_using_proofs() {
        let mut test = TemplateTest::new(["tests/templates/access_rules", "tests/templates/composability"]);

        // Create sender and receiver accounts
        let (owner_account, owner_proof, owner_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "using_resource_rules", args![])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        // Find the resource address for the tokens from the output substates
        let badge_resource = result
            .finalize
            .result
            .accept()
            .unwrap()
            .up_iter()
            .filter_map(|(addr, s)| s.substate_value().as_resource().map(|r| (addr, r)))
            .filter(|(_, r)| r.resource_type().is_non_fungible())
            .map(|(addr, _)| addr.as_resource_address().unwrap())
            .next()
            .unwrap();

        let cross_call_template = test.get_template_address("Composability");
        // Try to take tokens without proof. Even though I'm the owner of the resource, the scope does not carry over
        // when cross-template calls are made.
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_function(cross_call_template, "call_component_with_args", args![
                    component_address,
                    "take_tokens",
                    args![Amount(10)],
                ])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);

        // Do a cross template call using a proof
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "mint_new_badge", args![])
                .put_last_instruction_output_on_workspace("badge")
                .call_method(owner_account, "deposit", args![Workspace("badge")])
                .call_method(
                    owner_account,
                    "create_proof_for_resource",
                    args![badge_resource],
                )
                .put_last_instruction_output_on_workspace("proof")
                // This is quite interesting: we have to pass in the proof to call_component_with_args_using_proof to bring it into scope so that the cross template call can use it.
                // Another way would be for the arguments to resolve recursively at the "base" call site, rather than resolving workspace args in invoke_call. This requires indexing the Args type.
                .call_function(
                    cross_call_template,
                    "call_component_with_args_using_proof",
                    args![
                        component_address,
                        "take_tokens_using_proof",
                        Workspace("proof"),
                        args![Workspace("proof"), Amount(10)],
                    ],
                )
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn it_creates_a_proof_from_bucket() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_proof, _, owner_key) = test.create_owner_proof();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "using_badge_rules", args![])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        // Find the resource address for the badge from the output substates
        let badge_resource = result
            .finalize
            .result
            .accept()
            .unwrap()
            .up_iter()
            .filter_map(|(addr, s)| s.substate_value().as_resource().map(|r| (addr, r)))
            .filter(|(_, r)| r.resource_type().is_non_fungible())
            .map(|(addr, _)| addr.as_resource_address().unwrap())
            .next()
            .unwrap();

        // User cannot get the tokens
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Withdraw);

        // Give the user a withdraw and deposit badge
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_badge_by_name", args!["withdraw"])
                .put_last_instruction_output_on_workspace("withdraw_perm")
                .call_method(component_address, "take_badge_by_name", args!["deposit"])
                .put_last_instruction_output_on_workspace("deposit_perm")
                .call_method(user_account, "deposit", args![Workspace("withdraw_perm")])
                .call_method(user_account, "deposit", args![Workspace("deposit_perm")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        // Side case: we try deposit back the badges before we drop the proof. This is invalid.
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_method(
                    user_account,
                    "withdraw_many_non_fungibles",
                    args![
                        badge_resource,
                        vec![
                            NonFungibleId::from_string("withdraw"),
                            NonFungibleId::from_string("deposit")
                        ]
                    ],
                )
                .put_last_instruction_output_on_workspace("badges")
                // TODO: this perhaps should be a native instruction
                .call_function(
                    access_rules_template,
                    "create_proof_from_bucket",
                    args![Workspace("badges")],
                )
                .put_last_instruction_output_on_workspace("proof")
                .call_method(
                    component_address,
                    "take_tokens_using_proof",
                    args![Workspace("proof"), Amount(10)],
                )
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                // Deposit before dropping the proof
                .call_method(user_account, "deposit", args![Workspace("badges")])
                .drop_all_proofs_in_workspace()
                .sign(&owner_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_reject_reason(reason, RuntimeError::InvalidOpDepositLockedBucket {
            // badges is the 1st bucket
            bucket_id: 0.into(),
            locked_amount: Amount(2),
        });

        // User can take tokens, using a proof obtained from a bucket
        test.execute_expect_success(
            Transaction::builder()
                .call_method(
                    user_account,
                    "withdraw_many_non_fungibles",
                    args![
                        badge_resource,
                        vec![
                            NonFungibleId::from_string("withdraw"),
                            NonFungibleId::from_string("deposit")
                        ]
                    ],
                )
                .put_last_instruction_output_on_workspace("badges")
                // TODO: this perhaps should be a native instruction
                .call_function(
                    access_rules_template,
                    "create_proof_from_bucket",
                    args![Workspace("badges")],
                )
                .put_last_instruction_output_on_workspace("proof")
                .call_method(
                    component_address,
                    "take_tokens_using_proof",
                    args![Workspace("proof"), Amount(10)],
                )
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .drop_all_proofs_in_workspace()
                .call_method(user_account, "deposit", args![Workspace("badges")])
                .sign(&owner_key)
                .build(),
            vec![user_proof],
        );
    }

    #[test]
    fn it_restricts_resource_actions_to_component() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_account, owner_proof, owner_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(
                    access_rules_template,
                    "resource_actions_restricted_to_component",
                    args![],
                )
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();
        // Find the resource address for the tokens from the output substates
        let token_resource = result
            .finalize
            .result
            .accept()
            .unwrap()
            .up_iter()
            .filter_map(|(addr, s)| s.substate_value().as_resource().map(|r| (addr, r)))
            .filter(|(_, r)| r.resource_type().is_fungible())
            .map(|(addr, _)| addr.as_resource_address().unwrap())
            .next()
            .unwrap();

        // Minting using a template function will fail
        let reason = test.execute_expect_failure(
            Transaction::builder()
                .call_function(access_rules_template, "mint_resource", args![token_resource])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        assert_access_denied_for_action(reason, ResourceAuthAction::Mint);

        // Minting in a component context will succeed
        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "mint_more_tokens", args![Amount(1000)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof],
        );
    }

    #[test]
    fn it_allows_resource_actions_if_auth_hook_passes() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        // Create sender and receiver accounts
        let (owner_account, owner_proof, owner_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_auth_hook", args![true, "valid_auth_hook"])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        test.execute_expect_success(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );
    }

    #[test]
    fn it_denies_resource_actions_if_auth_hook_fails() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        let (owner_account, owner_proof, owner_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_auth_hook", args![false, "valid_auth_hook"])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        let result = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(owner_account, "deposit", args![Workspace("tokens")])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        assert_reject_reason(result, RuntimeError::AccessDeniedAuthHook {
            action_ident: ResourceAuthAction::Deposit.into(),
            details: "Panic! Access denied for action Deposit".to_string(),
        });
    }

    #[test]
    fn it_disallows_hook_that_writes_to_caller_component() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        let (_owner_account, owner_proof, owner_key) = test.create_empty_account();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_auth_hook", args![
                    true,
                    "malicious_auth_hook_set_state"
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        let result = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_reject_reason(result, RuntimeError::AccessDeniedSetComponentState {
            attempted_on: user_account.into(),
            attempted_by: Box::new(component_address.into()),
        });
    }

    #[test]
    fn it_disallows_hook_that_attempts_mutable_call_to_caller() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        let (_owner_account, owner_proof, owner_key) = test.create_empty_account();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_auth_hook", args![
                    true,
                    "malicious_auth_hook_call_mut"
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        let result = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        assert_reject_reason(
            result,
            RuntimeError::LockError(LockError::MultipleWriteLockRequested {
                address: user_account.into(),
            }),
        );
    }

    #[test]
    fn it_disallows_hook_that_attempts_mutable_call_to_another_component_in_the_transaction() {
        let mut test = TemplateTest::new(["tests/templates/access_rules", "tests/templates/state"]);

        let (_owner_account, owner_proof, owner_key) = test.create_empty_account();
        let (user_account, user_proof, user_key) = test.create_empty_account();

        // User has a state component
        let state_template = test.get_template_address("State");
        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(state_template, "restricted", args![])
                .sign(&user_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let state_component = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        let access_rules_template = test.get_template_address("AccessRulesTest");

        let result = test.execute_expect_success(
            Transaction::builder()
                .call_function(access_rules_template, "with_auth_hook_attack_component", args![
                    state_component
                ])
                .sign(&owner_key)
                .build(),
            vec![owner_proof.clone()],
        );

        let component_address = result.finalize.execution_results[0]
            .decode::<ComponentAddress>()
            .unwrap();

        let result = test.execute_expect_failure(
            Transaction::builder()
                .call_method(component_address, "take_tokens", args![Amount(10)])
                .put_last_instruction_output_on_workspace("tokens")
                .call_method(state_component, "set", args![1])
                // The hook should not be able to set the state component to 123
                .call_method(user_account, "deposit", args![Workspace("tokens")])
                .sign(&user_key)
                .build(),
            vec![user_proof.clone()],
        );

        // Check that the access hook fails (it does not have permission to call set in the state component)
        // even though the transaction signer has ownership of the object and the previous call to set works.
        assert_reject_reason(result, RuntimeError::AccessDeniedAuthHook {
            action_ident: ResourceAuthAction::Deposit.into(),
            details: String::new(),
        });
    }

    #[test]
    fn it_fails_if_auth_hook_is_invalid() {
        let mut test = TemplateTest::new(["tests/templates/access_rules"]);

        let access_rules_template = test.get_template_address("AccessRulesTest");

        [
            "invalid_auth_hook1",
            "invalid_auth_hook2",
            "invalid_auth_hook3",
            "invalid_auth_hook4",
            "invalid_auth_hook5",
            "hook_doesnt_exist",
        ]
        .iter()
        .for_each(|hook| {
            let reason = test.execute_expect_failure(
                Transaction::builder()
                    .call_function(access_rules_template, "with_auth_hook", args![true, hook])
                    .sign(test.get_test_secret_key())
                    .build(),
                vec![test.get_test_proof()],
            );

            assert_reject_reason(reason, RuntimeError::InvalidArgument {
                argument: "CreateResourceArg",
                // Partial error text
                reason: "Authorize hook".to_string(),
            });
        })
    }
}
