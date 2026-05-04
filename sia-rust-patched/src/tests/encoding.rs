//! # Encoding and Hashing Tests for Sia Types
//!
//! This module provides test coverage for encoding, hashing, and deserialization of core
//! consensus types ported from the Go Sia implementation. These tests are essential to ensure
//! compatibility with the official `walletd` node and maintain protocol correctness.
//!
//! ## Purpose
//! The tests primarily verify:
//! - **Address derivation**
//! - **Transaction encoding**
//! - **Transaction deserialization**
//!
//! ## ⚠ Security Warning
//! These tests are **consensus-critical**. Any failure to accurately decode or deserialize valid
//! transactions produced by `walletd` could result in serious security issues.
//!
//! In particular, a deserialization failure could break atomic swaps. If the Rust code cannot
//! correctly decode/deserialize the Sia transaction that reveals the shared secret, one party could
//! potentially claim both sets of funds, breaking the atomicity of the swap.

#[cfg(test)]
mod test {
    use crate::blake2b_internal::standard_unlock_hash;
    use crate::encoding::Encoder;
    use crate::types::{Address, Hash256, PublicKey, SpendPolicy, UnlockCondition};
    use std::str::FromStr;

    cross_target_tests! {
        // go test TestUnlockConditions2of2Multisig
        fn test_unlock_condition_unlock_hash_2of2_multisig() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey2 = PublicKey::from_bytes(
                &hex::decode("0101010000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey, pubkey2], 0, 2);

            let hash = unlock_condition.unlock_hash();
            let expected = Hash256::from_str("1e94357817d236167e54970a8c08bbd41b37bfceeeb52f6c1ce6dd01d50ea1e7").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestUnlockConditions1of2Multisig
        fn test_unlock_condition_unlock_hash_1of2_multisig() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey2 = PublicKey::from_bytes(
                &hex::decode("0101010000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey, pubkey2], 0, 1);

            let hash = unlock_condition.unlock_hash();
            let expected = Hash256::from_str("d7f84e3423da09d111a17f64290c8d05e1cbe4cab2b6bed49e3a4d2f659f0585").unwrap();
            assert_eq!(hash, expected);

            let address = unlock_condition.address();
            let expected = Address::from_str("d7f84e3423da09d111a17f64290c8d05e1cbe4cab2b6bed49e3a4d2f659f0585264e9181a51a").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyAboveEncodeHash
        fn test_spend_policy_encode_above() {
            let policy = SpendPolicy::above(1);

            let hash = Encoder::encode_and_hash(&policy);
            let expected = Hash256::from_str("bebf6cbdfb440a92e3e5d832ac30fe5d226ff6b352ed3a9398b7d35f086a8ab6").unwrap();
            assert_eq!(hash, expected);

            let address = policy.address();
            let expected =
                Address::from_str("188b997bb99dee13e95f92c3ea150bd76b3ec72e5ba57b0d57439a1a6e2865e9b25ea5d1825e").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyAfterEncodeHash
        fn test_spend_policy_encode_after() {
            let policy = SpendPolicy::after(1);
            let hash = Encoder::encode_and_hash(&policy);
            let expected = Hash256::from_str("07b0f28eafd87a082ad11dc4724e1c491821260821a30bec68254444f97d9311").unwrap();
            assert_eq!(hash, expected);

            let address = policy.address();
            let expected =
                Address::from_str("60c74e0ce5cede0f13f83b0132cb195c995bc7688c9fac34bbf2b14e14394b8bbe2991bc017f").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyPublicKeyEncodeHash
        fn test_spend_policy_encode_pubkey() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let policy = SpendPolicy::PublicKey(pubkey);

            let hash = Encoder::encode_and_hash(&policy);
            let expected = Hash256::from_str("4355c8f80f6e5a98b70c9c2f9a22f17747989b4744783c90439b2b034f698bfe").unwrap();
            assert_eq!(hash, expected);

            let address = policy.address();
            let expected =
                Address::from_str("55a7793237722c6df8222fd512063cb74228085ef1805c5184713648c159b919ac792fbad0e1").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyHash
        fn test_spend_policy_encode_hash() {
            let hash = Hash256::from_str("0102030000000000000000000000000000000000000000000000000000000000").unwrap();
            let policy = SpendPolicy::Hash(hash);

            let hash = Encoder::encode_and_hash(&policy);
            let expected = Hash256::from_str("9938967aefa6cbecc1f1620d2df5170d6811d4b2f47a879b621c1099a3b0628a").unwrap();
            assert_eq!(hash, expected);

            let address = policy.address();
            let expected =
                Address::from_str("a4d5a06d8d3c2e45aa26627858ce8e881505ae3c9d122a1d282c7824163751936cffb347e435").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyThreshold
        fn test_spend_policy_encode_threshold() {
            let policy = SpendPolicy::Threshold {
                n: 1,
                of: vec![SpendPolicy::above(1), SpendPolicy::after(1)],
            };

            let hash = Encoder::encode_and_hash(&policy);
            let expected = Hash256::from_str("7d792df6cd0b5e0f795287b3bf4087bbcc4c1bd0c52880a552cdda3e5e33d802").unwrap();
            assert_eq!(hash, expected);

            let address = policy.address();
            let expected =
                Address::from_str("4179b53aba165e46e4c85b3c8766bb758fb6f0bfa5721550b81981a3ec38efc460557dc1ded4").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestPolicyUnlockConditionEncodeSpecialCase
        // UnlockCondtion SpendPolicy has a special case for backwards compatibility of v1 addresses
        // see SpendPolicy::address
        fn test_spend_policy_encode_unlock_condition() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey], 0, 1);

            let sub_policy = SpendPolicy::UnlockConditions(unlock_condition);
            let base_address = sub_policy.address();
            let expected =
                Address::from_str("72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515dd64b9a56043a").unwrap();
            assert_eq!(base_address, expected);

            let policy = SpendPolicy::Threshold {
                n: 1,
                of: vec![sub_policy],
            };
            let address = policy.address();
            let expected =
                Address::from_str("1498a58c843ce66740e52421632d67a0f6991ea96db1fc97c29e46f89ae56e3534078876331d").unwrap();
            assert_eq!(address, expected);
        }

        // go test TestSiacoinUnlockConditionEncodeHash
        fn test_unlock_condition_encode() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey], 0, 1);

            let hash = Encoder::encode_and_hash(&unlock_condition);
            let expected = Hash256::from_str("5d49bae37b97c86573a1525246270c180464acf33d63cc2ac0269ef9a8cb9d98").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinPublicKeyEncodeHash
        fn test_public_key_encode() {
            let public_key = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();

            let hash = Encoder::encode_and_hash(&public_key);
            let expected = Hash256::from_str("d487326614f066416308bf6aa4e5041d1949928e4b26ede98e3cebb36a3b1726").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestStandardUnlockHash
        fn test_unlock_condition_unlock_hash_standard() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey.clone()], 0, 1);

            let hash = unlock_condition.unlock_hash();
            let expected = Hash256::from_str("72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515d").unwrap();
            assert_eq!(hash, expected);
            let hash = standard_unlock_hash(&pubkey);
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinAddressEncodeHash
        fn test_address_encode() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![pubkey.clone()], 0, 1);
            let address = unlock_condition.address();

            let hash = Encoder::encode_and_hash(&address);
            let expected = Hash256::from_str("d64b9a56043a909494f07520915e10dae62d75dba24b17c8414f8f3f30c53425").unwrap();
            assert_eq!(hash, expected);
        }
    }
}
