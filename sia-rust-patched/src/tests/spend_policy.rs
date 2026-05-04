#[cfg(test)]
mod test {
    use crate::types::{Address, Hash256, PublicKey, SpendPolicy, UnlockCondition, UnlockKey};
    use std::str::FromStr;

    cross_target_tests! {
        fn test_serde_spend_policy_above() {
            let j = json!(
            {
                "type": "above",
                "policy": 100
            }
            );

            let spend_policy_deser = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::Above(100);

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_after() {
            let j = json!(
            {
                "type": "after",
                "policy": 200
            }
            );

            let spend_policy_deser = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::After(200);

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_public_key() {
            let j = json!(
            {
                "type": "pk",
                "policy": "ed25519:0102030000000000000000000000000000000000000000000000000000000000"
            }
            );
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let spend_policy_deser: SpendPolicy = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::PublicKey(pubkey);

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_hash() {
            let j = json!(
            {
                "type": "h",
                "policy": "0102030000000000000000000000000000000000000000000000000000000000"
            }
            );
            let hash = Hash256::from_str("0102030000000000000000000000000000000000000000000000000000000000").unwrap();
            let spend_policy_deser: SpendPolicy = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::Hash(hash);

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_opaque() {
            let j = json!(
            {
                "type": "opaque",
                "policy": "f72e84ee9e344e424a6764068ffd7fdce4b4e50609892c6801bc1ead79d3ae0d71791b277a3a"
            }
            );
            let address =
                Address::from_str("f72e84ee9e344e424a6764068ffd7fdce4b4e50609892c6801bc1ead79d3ae0d71791b277a3a").unwrap();
            let spend_policy_deser: SpendPolicy = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::Opaque(address);

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_threshold() {
            let alice_pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let bob_pubkey = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let secret_hash = Hash256::from_str("0100000000000000000000000000000000000000000000000000000000000000").unwrap();
            let spend_policy = SpendPolicy::atomic_swap_success(&alice_pubkey, &bob_pubkey, 77777777, &secret_hash);

            let j = json!(
                {
                    "type": "thresh",
                    "policy": {
                        "n": 1,
                        "of": [
                            {
                                "type": "thresh",
                                "policy": {
                                    "n": 2,
                                    "of": [
                                        {
                                            "type": "pk",
                                            "policy": "ed25519:0102030000000000000000000000000000000000000000000000000000000000"
                                        },
                                        {
                                            "type": "h",
                                            "policy": "0100000000000000000000000000000000000000000000000000000000000000"
                                        }
                                    ]
                                }
                            },
                            {
                                "type": "opaque",
                                "policy": "f72e84ee9e344e424a6764068ffd7fdce4b4e50609892c6801bc1ead79d3ae0d71791b277a3a"
                            }
                        ]
                    }
                }
            );

            let spend_policy_deser: SpendPolicy = serde_json::from_value::<SpendPolicy>(j).unwrap().into();

            assert_eq!(spend_policy, spend_policy_deser);
        }

        fn test_serde_spend_policy_unlock_conditions_standard() {
            let j = json!(
                {
                    "type": "uc",
                    "policy": {
                        "timelock": 0,
                        "publicKeys": [
                            "ed25519:0102030000000000000000000000000000000000000000000000000000000000"
                        ],
                        "signaturesRequired": 1
                    }
                }
            );

            let public_key = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();

            let uc = UnlockCondition {
                unlock_keys: vec![UnlockKey::Ed25519(public_key)],
                timelock: 0,
                signatures_required: 1,
            };

            let spend_policy_deser: SpendPolicy = serde_json::from_value::<SpendPolicy>(j).unwrap().into();
            let spend_policy = SpendPolicy::UnlockConditions(uc);

            assert_eq!(spend_policy, spend_policy_deser);
        }
    }
}
