#[cfg(test)]
mod test {
    use crate::encoding::Encoder;
    use crate::types::{Address, Attestation, AttestationValue, Currency, CurrencyVersion, FileContractRevisionV2,
                       Hash256, Keypair, Preimage, PublicKey, SatisfiedPolicy, SiacoinElement, SiacoinInputV1,
                       SiacoinInputV2, SiacoinOutput, SiacoinOutputId, SiacoinOutputVersion, Signature, SpendPolicy,
                       StateElement, UnlockCondition, V2FileContract, V2FileContractElement, V2Transaction};
    use std::convert::TryFrom;
    use std::str::FromStr;

    cross_target_tests! {
        // go test TestSiacoinInputEncodeHash
        fn test_siacoin_input_encode() {
            let public_key = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let unlock_condition = UnlockCondition::new(vec![public_key], 0, 1);

            let vin = SiacoinInputV1 {
                parent_id: Hash256::from_str("0405060000000000000000000000000000000000000000000000000000000000")
                    .unwrap()
                    .into(),
                unlock_condition,
            };

            let hash = Encoder::encode_and_hash(&vin);
            let expected = Hash256::from_str("1d4b77aaa82c71ca68843210679b380f9638f8bec7addf0af16a6536dd54d6b4").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinCurrencyEncodeHashV1
        fn test_siacoin_currency_encode_v1() {
            let currency: Currency = 1u64.into();

            let hash = Encoder::encode_and_hash(&CurrencyVersion::V1(&currency));
            let expected = Hash256::from_str("a1cc3a97fc1ebfa23b0b128b153a29ad9f918585d1d8a32354f547d8451b7826").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinCurrencyEncodeHashV2
        fn test_siacoin_currency_encode_v2() {
            let currency: Currency = 1u64.into();

            let hash = Encoder::encode_and_hash(&CurrencyVersion::V2(&currency));
            let expected = Hash256::from_str("a3865e5e284e12e0ea418e73127db5d1092bfb98ed372ca9a664504816375e1d").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinCurrencyEncodeHashV1Max
        fn test_siacoin_currency_encode_v1_max() {
            let currency = Currency(u128::MAX);

            let hash = Encoder::encode_and_hash(&CurrencyVersion::V1(&currency));
            let expected = Hash256::from_str("4b9ed7269cb15f71ddf7238172a593a8e7ffe68b12c1bf73d67ac8eec44355bb").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinCurrencyEncodeHashV2Max
        fn test_siacoin_currency_encode_v2_max() {
            let currency = Currency(u128::MAX);

            let hash = Encoder::encode_and_hash(&CurrencyVersion::V2(&currency));
            let expected = Hash256::from_str("681467b3337425fd38fa3983531ca1a6214de9264eebabdf9c9bc5d157d202b4").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinOutputEncodeHashV1
        fn test_siacoin_output_encode_v1() {
            let vout = SiacoinOutput {
                value: 1u64.into(),
                address: Address::from_str("72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515dd64b9a56043a")
                    .unwrap(),
            };

            let hash = Encoder::encode_and_hash(&SiacoinOutputVersion::V1(&vout));
            let expected = Hash256::from_str("3253c57e76600721f2bdf03497a71ed47c09981e22ef49aed92e40da1ea91b28").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinOutputEncodeHashV2
        fn test_siacoin_output_encode_v2() {
            let vout = SiacoinOutput {
                value: 1u64.into(),
                address: Address::from_str("72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515dd64b9a56043a")
                    .unwrap(),
            };

            let hash = Encoder::encode_and_hash(&SiacoinOutputVersion::V2(&vout));
            let expected = Hash256::from_str("c278eceae42f594f5f4ca52c8a84b749146d08af214cc959ed2aaaa916eaafd3").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinElementEncodeHash
        fn test_siacoin_element_encode() {
            let state_element = StateElement {
                leaf_index: 1,
                merkle_proof: vec![
                    Hash256::from_str("0405060000000000000000000000000000000000000000000000000000000000").unwrap(),
                    Hash256::from_str("0708090000000000000000000000000000000000000000000000000000000000").unwrap(),
                ],
            };
            let siacoin_element = SiacoinElement {
                id: Hash256::from_str("0102030000000000000000000000000000000000000000000000000000000000").unwrap().into(),
                state_element,
                siacoin_output: SiacoinOutput {
                    value: 1u64.into(),
                    address: Address::from_str(
                        "72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515dd64b9a56043a",
                    )
                    .unwrap(),
                },
                maturity_height: 0,
            };

            let hash = Encoder::encode_and_hash(&siacoin_element);
            let expected = Hash256::from_str("4c46cbe535099409d2ea4255debda3fb62993595e305c78688ec4306f8464d7d").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestStateElementEncodeHash
        fn test_state_element_encode() {
            let state_element = StateElement {
                leaf_index: 1,
                merkle_proof: vec![
                    Hash256::from_str("0405060000000000000000000000000000000000000000000000000000000000").unwrap(),
                    Hash256::from_str("0708090000000000000000000000000000000000000000000000000000000000").unwrap(),
                ],
            };

            let hash = Encoder::encode_and_hash(&state_element);
            let expected = Hash256::from_str("70f868873fcb6196cd54bbb1e9e480188043426d3f7c9dc8fc5a7a536981cef1").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestStateElementEncodeHashNullMerkleProof
        fn test_state_element_encode_null_merkle_proof() {
            let j = r#"{"leafIndex":1}"#;
            let state_element = serde_json::from_str::<StateElement>(j).unwrap();

            let hash = Encoder::encode_and_hash(&state_element);
            let expected = Hash256::from_str("a3865e5e284e12e0ea418e73127db5d1092bfb98ed372ca9a664504816375e1d").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestStateElementEncodeHashNullMerkleProof
        fn test_state_element_encode_empty_merkle_proof() {
            let j = r#"{"leafIndex":1,"merkleProof":[]}"#;
            let state_element = serde_json::from_str::<StateElement>(j).unwrap();

            let hash = Encoder::encode_and_hash(&state_element);
            let expected = Hash256::from_str("a3865e5e284e12e0ea418e73127db5d1092bfb98ed372ca9a664504816375e1d").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSiacoinInputEncodeHashV1
        fn test_siacoin_input_encode_v1() {
            let vin = SiacoinInputV1 {
                parent_id: Hash256::default().into(),
                unlock_condition: UnlockCondition::new(vec![], 0, 0),
            };

            let hash = Encoder::encode_and_hash(&vin);
            let expected = Hash256::from_str("2f806f905436dc7c5079ad8062467266e225d8110a3c58d17628d609cb1c99d0").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSignatureEncodeHash
        fn test_signature_encode() {
            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap().as_ref()).unwrap();

            let hash = Encoder::encode_and_hash(&signature);
            let expected = Hash256::from_str("1e6952fe04eb626ae759a0090af2e701ba35ee6ad15233a2e947cb0f7ae9f7c7").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSatisfiedPolicyPublicKey
        fn test_satisfied_policy_encode_public_key() {
            let public_key = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();

            let policy = SpendPolicy::PublicKey(public_key);

            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();

            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![signature],
                preimages: vec![],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("92d9097978387a5da9d17435b796984dae6bd4342c88684d0949e406755c289c").unwrap();
            assert_eq!(hash, expected);
        }

        // go test TestSatisfiedPolicyHashEmpty
        fn test_satisfied_policy_encode_hash_empty() {
            let policy = SpendPolicy::Hash(Hash256::default());
            // This would throw an error from SpendPolicy::Verify because it does not include a
            // preimage, but both implementations should hash the same way regardless
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![],
                preimages: vec![],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("8499a629589884c5b343e61d1c503101229b44d529a36f2e27c37598067942a6").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_hash_w_preimage() {
            let policy = SpendPolicy::Hash(Hash256::default());

            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![],
                preimages: vec![Preimage::default()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("abac830016d15871dfefad87ddfce263a6936b77e8ec18e7712870d6bf771376").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_hash_w_preimage_and_frivulous_signature() {
            let policy = SpendPolicy::Hash(Hash256::default());
            // This would throw "superfluous signature(s)" error from SpendPolicy::Verify
            // Likely to never happen, but both implementations should hash the same way regardless
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![Signature::default()],
                preimages: vec![Preimage::default()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("22706c8f2cd851feb3e7432ac87be18acc55debd6e9bb738e3bad044f8dab94c").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_hash_frivulous_signature() {
            let policy = SpendPolicy::Hash(Hash256::default());

            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);

            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![Signature::default()],
                preimages: vec![preimage.into()],
            };
            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("cf1a51cb2e76546d96e8034ab050fbe95b6423ad450b2de8a4e76ad8f72500ed").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_hash() {
            let policy = SpendPolicy::Hash(Hash256::default());

            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![],
                preimages: vec![preimage.into()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("e3bbd67ade36322f3de8458b1daa80fd21bb74af88c779b768908e007611f36e").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_unlock_condition_standard() {
            let pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();

            let unlock_condition = UnlockCondition::new(vec![pubkey], 0, 1);

            let policy = SpendPolicy::UnlockConditions(unlock_condition);

            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();

            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![signature],
                preimages: vec![],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("0411ac20ae5472822bdc6c24c9ba2afdd828300ed3706cb1c07a8578276fd72d").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_unlock_condition_complex() {
            let pubkey0 = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey1 = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();
            let pubkey2 = PublicKey::from_bytes(
                &hex::decode("BE043906FD42297BC0A03CAA6E773EF27FC644261C692D090181E704BE4A88C3").unwrap(),
            )
            .unwrap();

            let unlock_condition = UnlockCondition::new(vec![pubkey0, pubkey1, pubkey2], 77777777, 3);

            let policy = SpendPolicy::UnlockConditions(unlock_condition);

            let sig0 = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();
            let sig1 = Signature::try_from(
                hex::decode("0734761D562958F6A82819474171F05A40163901513E5858BFF9E4BD9CAFB04DEF0D6D345BACE7D14E50C5C523433B411C7D7E1618BE010A63C55C34A2DEE70A").unwrap()).unwrap();
            let sig2 = Signature::try_from(
                hex::decode("482A2A905D7A6FC730387E06B45EA0CF259FCB219C9A057E539E705F60AC36D7079E26DAFB66ED4DBA9B9694B50BCA64F1D4CC4EBE937CE08A34BF642FAC1F0C").unwrap()).unwrap();

            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![sig0, sig1, sig2],
                preimages: vec![],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("b4d658dbc32b3e147d2736f75b14ca881d5c04963663993b6448c86f4f1a2815").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_threshold_simple() {
            let sub_policy = SpendPolicy::Hash(Hash256::default());
            let policy = SpendPolicy::Threshold {
                n: 1,
                of: vec![sub_policy],
            };
            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![],
                preimages: vec![preimage.into()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("5cd34ed67f2b2a55d016b4c485dfd1ca2eca75f6831cec9eed9494d6fa735315").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_threshold_atomic_swap_success() {
            let alice_pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let bob_pubkey = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let secret_hash = Hash256::from_str("0100000000000000000000000000000000000000000000000000000000000000").unwrap();

            let policy = SpendPolicy::atomic_swap_success(&alice_pubkey, &bob_pubkey, 77777777, &secret_hash);
            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();

            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![signature],
                preimages: vec![preimage.into()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("30abac67d0017556ae69416f54663edbe2fb14c7bcef028f2d228aef500e8f51").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_satisfied_policy_encode_threshold_atomic_swap_refund() {
            let alice_pubkey = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let bob_pubkey = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let secret_hash = Hash256::from_str("0100000000000000000000000000000000000000000000000000000000000000").unwrap();

            let policy = SpendPolicy::atomic_swap_refund(&alice_pubkey, &bob_pubkey, 77777777, &secret_hash);
            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();

            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);
            let satisfied_policy = SatisfiedPolicy {
                policy,
                signatures: vec![signature],
                preimages: vec![preimage.into()],
            };

            let hash = Encoder::encode_and_hash(&satisfied_policy);
            let expected = Hash256::from_str("69b26bdb1114af01e4626d2a31184706e1dc83d83063c9019f9ee66381bd6923").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_siacoin_input_encode_v2() {
            let sub_policy = SpendPolicy::Hash(Hash256::default());
            let policy = SpendPolicy::Threshold {
                n: 1,
                of: vec![sub_policy],
            };
            let mut preimage = [0u8; 32];
            preimage[..4].copy_from_slice(&[1, 2, 3, 4]);

            let satisfied_policy = SatisfiedPolicy {
                policy: policy.clone(),
                signatures: vec![],
                preimages: vec![preimage.into()],
            };

            let vin = SiacoinInputV2 {
                parent: SiacoinElement {
                    id: SiacoinOutputId::default(),
                    state_element: StateElement {
                        leaf_index: 0,
                        merkle_proof: vec![Hash256::default()],
                    },
                    siacoin_output: SiacoinOutput {
                        value: 1u64.into(),
                        address: policy.address(),
                    },
                    maturity_height: 0,
                },
                satisfied_policy,
            };

            let hash = Encoder::encode_and_hash(&vin);
            let expected = Hash256::from_str("102a2924e7427ee3654bfeea8fc055fd82c2a403598484dbb704da9cdaada3ba").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_attestation_encode() {
            let public_key = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let signature = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();

            let attestation = Attestation {
                public_key,
                key: "HostAnnouncement".to_string(),
                value: AttestationValue(vec![1u8, 2u8, 3u8, 4u8]),
                signature,
            };

            let hash = Encoder::encode_and_hash(&attestation);
            let expected = Hash256::from_str("b28b32c6f91d1b57ab4a9ea9feecca16b35bb8febdee6a0162b22979415f519d").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_file_contract_v2_encode() {
            let pubkey0 = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey1 = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let sig0 = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();
            let sig1 = Signature::try_from(
                hex::decode("0734761D562958F6A82819474171F05A40163901513E5858BFF9E4BD9CAFB04DEF0D6D345BACE7D14E50C5C523433B411C7D7E1618BE010A63C55C34A2DEE70A").unwrap()).unwrap();

            let address0 = Address::standard_address_v1(&pubkey0);
            let address1 = Address::standard_address_v1(&pubkey1);

            let vout0 = SiacoinOutput {
                value: 1u64.into(),
                address: address0,
            };
            let vout1 = SiacoinOutput {
                value: 1u64.into(),
                address: address1,
            };

            let file_contract_v2 = V2FileContract {
                capacity: 0,
                filesize: 1,
                file_merkle_root: Hash256::default(),
                proof_height: 1,
                expiration_height: 1,
                renter_output: vout0,
                host_output: vout1,
                missed_host_value: 1u64.into(),
                total_collateral: 1u64.into(),
                renter_public_key: pubkey0,
                host_public_key: pubkey1,
                revision_number: 1,
                renter_signature: sig0,
                host_signature: sig1,
            };

            let hash = Encoder::encode_and_hash(&file_contract_v2);
            let expected = Hash256::from_str("e851362bab643dc066b9d3c22c0fa0d67bc7b0cb520c689765e2292f4e7f435e").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_file_contract_element_v2_encode() {
            let pubkey0 = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey1 = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let sig0 = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();
            let sig1 = Signature::try_from(
                hex::decode("0734761D562958F6A82819474171F05A40163901513E5858BFF9E4BD9CAFB04DEF0D6D345BACE7D14E50C5C523433B411C7D7E1618BE010A63C55C34A2DEE70A").unwrap()).unwrap();

            let address0 = Address::standard_address_v1(&pubkey0);
            let address1 = Address::standard_address_v1(&pubkey1);

            let vout0 = SiacoinOutput {
                value: 1u64.into(),
                address: address0,
            };
            let vout1 = SiacoinOutput {
                value: 1u64.into(),
                address: address1,
            };

            let file_contract_v2 = V2FileContract {
                capacity: 0,
                filesize: 1,
                file_merkle_root: Hash256::default(),
                proof_height: 1,
                expiration_height: 1,
                renter_output: vout0,
                host_output: vout1,
                missed_host_value: 1u64.into(),
                total_collateral: 1u64.into(),
                renter_public_key: pubkey0,
                host_public_key: pubkey1,
                revision_number: 1,
                renter_signature: sig0,
                host_signature: sig1,
            };

            let state_element = StateElement {
                leaf_index: 1,
                merkle_proof: vec![
                    Hash256::from_str("0405060000000000000000000000000000000000000000000000000000000000").unwrap(),
                    Hash256::from_str("0708090000000000000000000000000000000000000000000000000000000000").unwrap(),
                ],
            };

            let file_contract_element_v2 = V2FileContractElement {
                id: Hash256::from_str("0707070000000000000000000000000000000000000000000000000000000000").unwrap().into(),
                state_element,
                v2_file_contract: file_contract_v2,
            };

            let hash = Encoder::encode_and_hash(&file_contract_element_v2);
            let expected = Hash256::from_str("3005594b14c1615aadaef2d8558713ebeabfa7d54f1dec671ba67ea8264816e6").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_file_contract_revision_v2_encode() {
            let pubkey0 = PublicKey::from_bytes(
                &hex::decode("0102030000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let pubkey1 = PublicKey::from_bytes(
                &hex::decode("06C87838297B7BB16AB23946C99DFDF77FF834E35DB07D71E9B1D2B01A11E96D").unwrap(),
            )
            .unwrap();

            let sig0 = Signature::try_from(
                hex::decode("105641BF4AE119CB15617FC9658BEE5D448E2CC27C9BC3369F4BA5D0E1C3D01EBCB21B669A7B7A17CF8457189EAA657C41D4A2E6F9E0F25D0996D3A17170F309").unwrap()).unwrap();
            let sig1 = Signature::try_from(
                hex::decode("0734761D562958F6A82819474171F05A40163901513E5858BFF9E4BD9CAFB04DEF0D6D345BACE7D14E50C5C523433B411C7D7E1618BE010A63C55C34A2DEE70A").unwrap()).unwrap();

            let address0 = Address::standard_address_v1(&pubkey0);
            let address1 = Address::standard_address_v1(&pubkey1);

            let vout0 = SiacoinOutput {
                value: 1u64.into(),
                address: address0,
            };
            let vout1 = SiacoinOutput {
                value: 1u64.into(),
                address: address1,
            };

            let file_contract_v2 = V2FileContract {
                capacity: 0,
                filesize: 1,
                file_merkle_root: Hash256::default(),
                proof_height: 1,
                expiration_height: 1,
                renter_output: vout0,
                host_output: vout1,
                missed_host_value: 1u64.into(),
                total_collateral: 1u64.into(),
                renter_public_key: pubkey0,
                host_public_key: pubkey1,
                revision_number: 1,
                renter_signature: sig0,
                host_signature: sig1,
            };

            let state_element = StateElement {
                leaf_index: 1,
                merkle_proof: vec![
                    Hash256::from_str("0405060000000000000000000000000000000000000000000000000000000000").unwrap(),
                    Hash256::from_str("0708090000000000000000000000000000000000000000000000000000000000").unwrap(),
                ],
            };

            let file_contract_element_v2 = V2FileContractElement {
                id: Hash256::from_str("0102030000000000000000000000000000000000000000000000000000000000").unwrap().into(),
                state_element,
                v2_file_contract: file_contract_v2.clone(),
            };

            let file_contract_revision_v2 = FileContractRevisionV2 {
                parent: file_contract_element_v2,
                revision: file_contract_v2,
            };

            let hash = Encoder::encode_and_hash(&file_contract_revision_v2);
            let expected = Hash256::from_str("4f23582ec40570345f72adab8cd6249c0167669b78aec9ac7209befefc281f4f").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_v2_transaction_sig_hash() {
            let j = json!(
                {
                    "siacoinInputs": [
                        {
                            "parent": {
                                "id": "b49cba94064a92a75bf8c6f9d32ab18f38bfb14a2252e3e117d04da89d536f29",
                                "stateElement": {
                                    "leafIndex": 302,
                                    "merkleProof": [
                                        "6f41d366712e9dfa423160b5388f3faf673addf43566d7b3562106d15b833f46",
                                        "eb7df5e13eccd812a47f29a233bbf3212b7379ca6dd20ba9981524bfd5eadce6",
                                        "04104cbada51333f8f37a6eb71f1e8cb287da2d62469568a8a36dc8c76602c80",
                                        "16aac5c671d49d8cfc5493cb4c6f34889e30a0d283745c6473406bd60ab5e754",
                                        "1b9ccf2b6f555687b1384091faa9ed1c154f41aaff81dcf393295383ca99f518",
                                        "31337c9db5cdd181f5ff142bd490f779eedb1485e5dd905743280aeac3cd7ac9"
                                    ],
                                },
                                "siacoinOutput": {
                                    "value": "288594172736732570239334030000",
                                    "address": "2757c80b7ec2e493a138fed45b906f9f5735a992b68dcbd2069fbdf418c8b25158f3ac7a816b"
                                },
                                "maturityHeight": 0
                            },
                            "satisfiedPolicy": {
                                "policy": {
                                    "type": "uc",
                                    "policy": {
                                        "timelock": 0,
                                        "publicKeys": [
                                            "ed25519:7931b69fe8888e354d601a778e31bfa97fa89dc6f625cd01cc8aa28046e557e7"
                                        ],
                                        "signaturesRequired": 1
                                    }
                                },
                                "signatures": [
                                    "f43380794a6384e3d24d9908143c05dd37aaac8959efb65d986feb70fe289a5e26b84e0ac712af01a2f85f8727da18aae13a599a51fb066d098591e40cb26902"
                                ]
                            }
                        }
                    ],
                    "siacoinOutputs": [
                        {
                            "value": "1000000000000000000000000000",
                            "address": "000000000000000000000000000000000000000000000000000000000000000089eb0d6a8a69"
                        },
                        {
                            "value": "287594172736732570239334030000",
                            "address": "2757c80b7ec2e493a138fed45b906f9f5735a992b68dcbd2069fbdf418c8b25158f3ac7a816b"
                        }
                    ],
                    "minerFee": "0"
                }
            );

            let tx = serde_json::from_value::<V2Transaction>(j).unwrap();
            let hash = tx.input_sig_hash();
            let expected = Hash256::from_str("ef2f59bb25300bed9accbdcd95e1a2bd9f146ab6b474002670dc908ad68aacac").unwrap();
            assert_eq!(hash, expected);
        }

        fn test_v2_transaction_signing() {
            let j = json!(
                {
                    "siacoinInputs": [
                        {
                            "parent": {
                                "id": "f59e395dc5cbe3217ee80eff60585ffc9802e7ca580d55297782d4a9b4e08589",
                                "stateElement": {
                                    "leafIndex": 3,
                                    "merkleProof": [
                                        "ab0e1726444c50e2c0f7325eb65e5bd262a97aad2647d2816c39d97958d9588a",
                                        "467e2be4d8482eca1f99440b6efd531ab556d10a8371a98a05b00cb284620cf0",
                                        "64d5766fce1ff78a13a4a4744795ad49a8f8d187c01f9f46544810049643a74a",
                                        "31d5151875152bc25d1df18ca6bbda1bef5b351e8d53c277791ecf416fcbb8a8",
                                        "12a92a1ba87c7b38f3c4e264c399abfa28fb46274cfa429605a6409bd6d0a779",
                                        "eda1d58a9282dbf6c3f1beb4d6c7bdc036d14a1cfee8ab1e94fabefa9bd63865",
                                        "e03dee6e27220386c906f19fec711647353a5f6d76633a191cbc2f6dce239e89",
                                        "e70fcf0129c500f7afb49f4f2bb82950462e952b7cdebb2ad0aa1561dc6ea8eb"
                                    ]
                                },
                                "siacoinOutput": {
                                    "value": "300000000000000000000000000000",
                                    "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                                },
                                "maturityHeight": 145
                            },
                            "satisfiedPolicy": {
                                "policy": {
                                    "type": "uc",
                                    "policy": {
                                        "timelock": 0,
                                        "publicKeys": [
                                            "ed25519:cecc1507dc1ddd7295951c290888f095adb9044d1b73d696e6df065d683bd4fc"
                                        ],
                                        "signaturesRequired": 1
                                    }
                                },
                                "signatures": [
                                    "f0a29ba576eb0dbc3438877ac1d3a6da4f3c4cbafd9030709c8a83c2fffa64f4dd080d37444261f023af3bd7a10a9597c33616267d5371bf2c0ade5e25e61903"
                                ]
                            }
                        }
                    ],
                    "siacoinOutputs": [
                        {
                            "value": "1000000000000000000000000000",
                            "address": "000000000000000000000000000000000000000000000000000000000000000089eb0d6a8a69"
                        },
                        {
                            "value": "299000000000000000000000000000",
                            "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                        }
                    ],
                    "minerFee": "0"
                }
            );
            let tx = serde_json::from_value::<V2Transaction>(j).unwrap();
            let keypair = Keypair::from_private_bytes(
                &hex::decode("0100000000000000000000000000000000000000000000000000000000000000").unwrap(),
            )
            .unwrap();
            let sig_hash = tx.input_sig_hash();

            // test that we can correctly regenerate the signature
            let sig: Signature = keypair.sign(&sig_hash.0);
            assert_eq!(tx.siacoin_inputs[0].satisfied_policy.signatures[0], sig);
        }

        fn test_siacoin_output_id_new() {
            let txid = Hash256::from_str("31be0badc64d40fbcb91b63835c07d75ab49addd1fc1d839b8415e1e5ff38cb5").unwrap();
            let output_index = 0u32;
            let output_id = SiacoinOutputId::new(txid, output_index);
            let expected = SiacoinOutputId(
                Hash256::from_str("47b2ceee0a9e246d5f997129a250ecb3d0917f5e844989d520e246145349d292").unwrap(),
            );
            assert_eq!(output_id, expected);
        }

        fn test_v2_transaction_txid() {
            const MAKER_SWAP_TAKER_FEE_TX: &str = r#"{
                "id": "4c14880c567d164fa1ee826abec671e7782e3bc7bbfbad0c30b749f35f0db5e4",
                "siacoinInputs": [
                    {
                        "parent": {
                            "id": "3855e2c359cb43aea0765c6bb411df043ea92c5e5dc45a99f1faf647d1fb00e5",
                            "stateElement": {
                                "leafIndex": 74392410
                            },
                            "siacoinOutput": {
                                "value": "24982638748779641304941874",
                                "address": "2c4a029ef67858d7c3ebf9ce7f1c257fd880b1b073fd3923091423e1658ae23d2b426be204db"
                            },
                            "maturityHeight": 0
                        },
                        "satisfiedPolicy": {
                            "policy": {
                                "type": "pk",
                                "policy": "ed25519:c94acdcbd6a44c25a2640191afd80d6ef6c692c3a0faa0db3cec0189d90f6cd1"
                            },
                            "signatures": [
                                "a2065b3e4b684c7d74bb5e73c2b43bb307f216851e6f5d2138a3cce14280b79e50480b9e2410b9fa71fa2c9823ab915c63d93ddc2acbef078d9cf8cac8c64004"
                            ]
                        }
                    }
                ],
                "siacoinOutputs": [
                    {
                        "value": "8681757519703999646709",
                        "address": "0125788b383a1dd122cd511386dd2668c62e54610cc743307d7a8ba17161a175f4b03c40b656"
                    },
                    {
                        "value": "24973936991259937305295165",
                        "address": "2c4a029ef67858d7c3ebf9ce7f1c257fd880b1b073fd3923091423e1658ae23d2b426be204db"
                    }
                ],
                "arbitraryData": "seTZFS6BSBS34GBqfuwqTQ==",
                "minerFee": "20000000000000000000"
            }"#;

            const MAKER_SWAP_TAKER_FEE_HASH: &str =
                "4c14880c567d164fa1ee826abec671e7782e3bc7bbfbad0c30b749f35f0db5e4";

            let tx = serde_json::from_str::<V2Transaction>(MAKER_SWAP_TAKER_FEE_TX).unwrap();
            let txid = tx.txid();
            let expected = Hash256::from_str(MAKER_SWAP_TAKER_FEE_HASH).unwrap();
            assert_eq!(txid, expected);
        }

        fn test_v2_transaction_txid_storage_proof() {
            const STORAGE_PROOF_TX: &str = r#"{
                "siacoinInputs": [
                    {
                        "parent": {
                            "id": "ab5b0d4a5fc681b9b8bd276082cea42c3a3dc2359b8d2eb49722b544d2561115",
                            "stateElement": {
                                "leafIndex": 74391721
                            },
                            "siacoinOutput": {
                                "value": "17670215106248225520963228288",
                                "address": "15374e49010cac9840f86def0e8d63e3d0b1d11f951995f12c5152b908549f60c418613a8dd8"
                            },
                            "maturityHeight": 0
                        },
                        "satisfiedPolicy": {
                            "policy": {
                                "type": "uc",
                                "policy": {
                                    "timelock": 0,
                                    "publicKeys": [
                                        "ed25519:68271eda7adf5a80d154d15ccff3cebebcc447f0609cd580bb5c638aef227d43"
                                    ],
                                    "signaturesRequired": 1
                                }
                            },
                            "signatures": [
                                "e11e2edbc27b88461285283558e9b146ee620be1745ee9b60697f8e21409ea60ab1a4653710812f6836d3fefa08b96c1be16dcb87d3062034861f26915ecec06"
                            ]
                        }
                    }
                ],
                "siacoinOutputs": [
                    {
                        "value": "17670195106248225520963228288",
                        "address": "15374e49010cac9840f86def0e8d63e3d0b1d11f951995f12c5152b908549f60c418613a8dd8"
                    }
                ],
                "fileContractResolutions": [
                    {
                        "parent": {
                            "id": "c6eb6dca343a61a1a0f0b9f2a4939cdd51c297c987ca4b30b3333bdc0fc92081",
                            "stateElement": {
                                "leafIndex": 74348292
                            },
                            "v2FileContract": {
                                "capacity": 0,
                                "filesize": 0,
                                "fileMerkleRoot": "0000000000000000000000000000000000000000000000000000000000000000",
                                "proofHeight": 550434,
                                "expirationHeight": 550578,
                                "renterOutput": {
                                    "value": "5675257912250488405884928",
                                    "address": "b9d27c701886c5a86009a996eb518b3664973f2e812a4237f867f10483a178bd90fb4bd3244b"
                                },
                                "hostOutput": {
                                    "value": "1596749473887802825375744",
                                    "address": "15374e49010cac9840f86def0e8d63e3d0b1d11f951995f12c5152b908549f60c418613a8dd8"
                                },
                                "missedHostValue": "0",
                                "totalCollateral": "0",
                                "renterPublicKey": "ed25519:e8ccbabe231c1fc6e74ad974bec3ecf6ebca695d78715dae311bd7b6ec897471",
                                "hostPublicKey": "ed25519:6795c1b2ac73485a809d5c4218320426e3243cc700772cb0808c58e7723321d4",
                                "revisionNumber": 17,
                                "renterSignature": "7e1f07d5fb07f2b8ec3e137415165c4f55ad73b1302906b9fa7d22746a6a0a4483bd5d7717b9f8b022c41438c6b3046a7f2888f71a53703b5f9face20d8df507",
                                "hostSignature": "6db94f221b6ea9251c8546fb5f28d759cf0a5b2b389a2b5906aa1e1783316b74e4b41a05c6883f3d086551b4002a386c9b02f5e9048beb0437d6c4858f1ee00e"
                            }
                        },
                        "type": "storageProof",
                        "resolution": {
                            "proofIndex": {
                                "id": "00000000000000005fa0bf6b547eef003d7d667e573a579c5b6e3f842197b462",
                                "stateElement": {
                                    "leafIndex": 74393462
                                },
                                "chainIndex": {
                                    "height": 550434,
                                    "id": "00000000000000005fa0bf6b547eef003d7d667e573a579c5b6e3f842197b462"
                                }
                            },
                            "leaf": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                            "proof": []
                        }
                    }
                ],
                "minerFee": "20000000000000000000000"
            }"#;

            const STORAGE_PROOF_TX_HASH: &str =
                "1dbeb06ab9e487ce1e89bdd4b951f6c678c57ce873e890e8aa399a529569c1ac";

            let tx = serde_json::from_str::<V2Transaction>(STORAGE_PROOF_TX).unwrap();
            let txid = tx.txid();
            let expected = Hash256::from_str(STORAGE_PROOF_TX_HASH).unwrap();
            assert_eq!(txid, expected);
        }

        fn test_v2_transaction_txid_storage_proof_2() {
            const STORAGE_PROOF_TX_2: &str = r#"{
                "siacoinInputs": [
                    {
                        "parent": {
                            "id": "858b04ee9b5313d28728b3adc1206ac61fbf2a3138364c13ffb62dec128da20e",
                            "stateElement": {
                                "leafIndex": 74388637
                            },
                            "siacoinOutput": {
                                "value": "981822898445607153434624000",
                                "address": "0832ac4f609e361dc8920adf85f9eec8843f2c2bb5c1614af28020fdeea490c0fe0d3ba2dbcc"
                            },
                            "maturityHeight": 0
                        },
                        "satisfiedPolicy": {
                            "policy": {
                                "type": "uc",
                                "policy": {
                                    "timelock": 0,
                                    "publicKeys": [
                                        "ed25519:3385212ec223c6badda2a03ac6f308cfa67fc0a42d941212af4ad5c403b0dba8"
                                    ],
                                    "signaturesRequired": 1
                                }
                            },
                            "signatures": [
                                "412bb9bc21165a15b903998b2164a32085bb7adaa59fcc444c7ef01a00a5be12745bc76c1a4a77d4e4625d87ae779e89700fc770e00b3b33d64b90770830a408"
                            ]
                        }
                    }
                ],
                "siacoinOutputs": [
                    {
                        "value": "981802898445607153434624000",
                        "address": "0832ac4f609e361dc8920adf85f9eec8843f2c2bb5c1614af28020fdeea490c0fe0d3ba2dbcc"
                    }
                ],
                "fileContractResolutions": [
                    {
                        "parent": {
                            "id": "c2a54ee8273f904ae2364886b39fb9d4cf67037effe6b3353b51e45cd9c81ce0",
                            "stateElement": {
                                "leafIndex": 74348293
                            },
                            "v2FileContract": {
                                "capacity": 0,
                                "filesize": 0,
                                "fileMerkleRoot": "0000000000000000000000000000000000000000000000000000000000000000",
                                "proofHeight": 550434,
                                "expirationHeight": 550578,
                                "renterOutput": {
                                    "value": "5455864119930680151900160",
                                    "address": "b9d27c701886c5a86009a996eb518b3664973f2e812a4237f867f10483a178bd90fb4bd3244b"
                                },
                                "hostOutput": {
                                    "value": "1544363791036505340772352",
                                    "address": "0832ac4f609e361dc8920adf85f9eec8843f2c2bb5c1614af28020fdeea490c0fe0d3ba2dbcc"
                                },
                                "missedHostValue": "0",
                                "totalCollateral": "0",
                                "renterPublicKey": "ed25519:e8ccbabe231c1fc6e74ad974bec3ecf6ebca695d78715dae311bd7b6ec897471",
                                "hostPublicKey": "ed25519:b0b7a0eca67a5f92ef55803042e42fcad0f1ea28f9cadf36ae3a7cab17bdd27f",
                                "revisionNumber": 15,
                                "renterSignature": "d1d16e3bcda1d6250d9a9f629b64b3c91e6f717d4a938f5bc8ba43a87cf7fa54ce7cb75337ff66d9413ed865067ac631f3119fc029076d864db39e7b8029fa02",
                                "hostSignature": "15c02779204542cf94b0bb21a36570c0d753b8e99af03fc750b615b11eb3c595b1ee30e370af22b3a80b5bb5fc391c91c0aa537c63ee5609292a3030416a510d"
                            }
                        },
                        "type": "storageProof",
                        "resolution": {
                            "proofIndex": {
                                "id": "00000000000000005fa0bf6b547eef003d7d667e573a579c5b6e3f842197b462",
                                "stateElement": {
                                    "leafIndex": 74393462
                                },
                                "chainIndex": {
                                    "height": 550434,
                                    "id": "00000000000000005fa0bf6b547eef003d7d667e573a579c5b6e3f842197b462"
                                }
                            },
                            "leaf": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                            "proof": []
                        }
                    }
                ],
                "minerFee": "20000000000000000000000"
            }"#;

            const STORAGE_PROOF_TX_2_HASH: &str =
                "d5be516291a6549f50a12f1f1f99acae1865eb3b00b674b34c60eefc3c0b15d1";

            let tx = serde_json::from_str::<V2Transaction>(STORAGE_PROOF_TX_2).unwrap();
            let txid = tx.txid();
            let expected = Hash256::from_str(STORAGE_PROOF_TX_2_HASH).unwrap();
            assert_eq!(txid, expected);
        }
    }
}
