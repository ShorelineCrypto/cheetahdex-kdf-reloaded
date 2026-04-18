use crate::update_coins_config;

// ── HD Wallet Integration Tests ──────────────────────────────────────

mod hd_wallet_integration {
    use crate::PrivKeyBuildPolicy;
    use crypto::CryptoCtx;
    use mm2_core::mm_ctx::MmCtxBuilder;
    use std::str::FromStr;

    /// Standard BIP39 test mnemonic ("abandon" x11 + "about"). Known test vector.
    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn test_detect_priv_key_policy_returns_global_hd() {
        let ctx = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx.clone(), TEST_MNEMONIC)
            .expect("CryptoCtx HD init should succeed");

        let policy = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx)
            .expect("detect_priv_key_policy should succeed");

        assert!(
            matches!(policy, PrivKeyBuildPolicy::GlobalHDAccount(_)),
            "Expected GlobalHDAccount policy, got Iguana or Trezor"
        );
    }

    #[test]
    fn test_detect_priv_key_policy_returns_iguana() {
        let ctx = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_iguana_passphrase(ctx.clone(), TEST_MNEMONIC)
            .expect("CryptoCtx Iguana init should succeed");

        let policy = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx)
            .expect("detect_priv_key_policy should succeed");

        assert!(
            matches!(policy, PrivKeyBuildPolicy::IguanaPrivKey(_)),
            "Expected IguanaPrivKey policy, got GlobalHDAccount or Trezor"
        );
    }

    #[test]
    fn test_hd_policy_gives_deterministic_context() {
        let ctx1 = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx1.clone(), TEST_MNEMONIC).unwrap();
        let policy1 = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx1).unwrap();

        let ctx2 = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx2.clone(), TEST_MNEMONIC).unwrap();
        let policy2 = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx2).unwrap();

        // Extract the GlobalHDAccountArc from both and verify same root key
        match (policy1, policy2) {
            (PrivKeyBuildPolicy::GlobalHDAccount(hd1), PrivKeyBuildPolicy::GlobalHDAccount(hd2)) => {
                assert_eq!(
                    hd1.root_seed_bytes(),
                    hd2.root_seed_bytes(),
                    "Same mnemonic should produce same root seed"
                );
            },
            _ => panic!("Both should be GlobalHDAccount"),
        }
    }

    #[test]
    fn test_hd_derivation_produces_correct_key_for_known_path() {
        use crypto::DerivationPath;

        let ctx = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx.clone(), TEST_MNEMONIC).unwrap();
        let policy = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx).unwrap();

        let global_hd = match policy {
            PrivKeyBuildPolicy::GlobalHDAccount(hd) => hd,
            _ => panic!("Expected GlobalHDAccount"),
        };

        // Derive at m/44'/141'/0'/0/0 (KMD BIP44 path)
        let kmd_path = DerivationPath::from_str("m/44'/141'/0'/0/0").expect("valid path");
        let secret1 = global_hd
            .derive_secp256k1_secret(&kmd_path)
            .expect("derivation should succeed");

        // Derive at m/44'/0'/0'/0/0 (BTC BIP44 path)
        let btc_path = DerivationPath::from_str("m/44'/0'/0'/0/0").expect("valid path");
        let secret2 = global_hd
            .derive_secp256k1_secret(&btc_path)
            .expect("derivation should succeed");

        // Different coin types must produce different keys
        assert_ne!(
            secret1.as_slice(),
            secret2.as_slice(),
            "Different coin_type in BIP44 path must produce different keys"
        );

        // Same path must produce same key (deterministic)
        let secret1_again = global_hd.derive_secp256k1_secret(&kmd_path).unwrap();
        assert_eq!(secret1.as_slice(), secret1_again.as_slice());
    }

    #[test]
    fn test_hd_derived_key_produces_valid_address() {
        use crypto::DerivationPath;
        use keys::KeyPair;
        use keys::Private;

        let ctx = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx.clone(), TEST_MNEMONIC).unwrap();
        let policy = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx).unwrap();

        let global_hd = match policy {
            PrivKeyBuildPolicy::GlobalHDAccount(hd) => hd,
            _ => panic!("Expected GlobalHDAccount"),
        };

        // Derive KMD key at m/44'/141'/0'/0/0
        let path = DerivationPath::from_str("m/44'/141'/0'/0/0").expect("valid path");
        let secret = global_hd.derive_secp256k1_secret(&path).unwrap();

        // Build a key pair from the derived secret
        let private = Private {
            prefix: 188, // KMD WIF prefix
            secret,
            compressed: true,
            checksum_type: bitcrypto::ChecksumType::DSHA256,
        };
        let key_pair = KeyPair::from_private(private).expect("valid key pair from HD-derived secret");

        // Public key should be 33 bytes (compressed)
        assert_eq!(key_pair.public().len(), 33);

        // Address hash should be 20 bytes (RIPEMD160(SHA256(pubkey)))
        assert_eq!(key_pair.public().address_hash().len(), 20);
    }

    #[test]
    fn test_different_mnemonics_produce_different_keys() {
        use crypto::DerivationPath;

        let mnemonic_a = TEST_MNEMONIC;
        let mnemonic_b = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";

        let ctx_a = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx_a.clone(), mnemonic_a).unwrap();
        let policy_a = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx_a).unwrap();

        let ctx_b = MmCtxBuilder::default().into_mm_arc();
        CryptoCtx::init_with_global_hd_account(ctx_b.clone(), mnemonic_b).unwrap();
        let policy_b = PrivKeyBuildPolicy::detect_priv_key_policy(&ctx_b).unwrap();

        let (hd_a, hd_b) = match (policy_a, policy_b) {
            (PrivKeyBuildPolicy::GlobalHDAccount(a), PrivKeyBuildPolicy::GlobalHDAccount(b)) => (a, b),
            _ => panic!("Both should be GlobalHDAccount"),
        };

        let path = DerivationPath::from_str("m/44'/141'/0'/0/0").unwrap();
        let secret_a = hd_a.derive_secp256k1_secret(&path).unwrap();
        let secret_b = hd_b.derive_secp256k1_secret(&path).unwrap();

        assert_ne!(
            secret_a.as_slice(),
            secret_b.as_slice(),
            "Different mnemonics must produce different keys at same path"
        );
    }
}


#[test]
fn test_update_coin_config_success() {
    let conf = json!([
        {
            "coin": "RICK",
            "asset": "RICK",
            "fname": "RICK (TESTCOIN)",
            "rpcport": 25435,
            "txversion": 4,
            "overwintered": 1,
            "mm2": 1,
        },
        {
            "coin": "MORTY",
            "asset": "MORTY",
            "fname": "MORTY (TESTCOIN)",
            "rpcport": 16348,
            "txversion": 4,
            "overwintered": 1,
            "mm2": 1,
        },
        {
            "coin": "ETH",
            "name": "ethereum",
            "fname": "Ethereum",
            "etomic": "0x0000000000000000000000000000000000000000",
            "rpcport": 80,
            "mm2": 1,
            "required_confirmations": 3,
        },
        {
            "coin": "ARPA",
            "name": "arpa-chain",
            "fname": "ARPA Chain",
            // ARPA coin contains the protocol already. This coin should be skipped.
            "protocol": {
                "type":"ERC20",
                "protocol_data": {
                    "platform": "ETH",
                    "contract_address": "0xBA50933C268F567BDC86E1aC131BE072C6B0b71a"
                }
            },
            "rpcport": 80,
            "mm2": 1,
            "required_confirmations": 3,
        },
        {
            "coin": "JST",
            "name": "JST",
            "fname": "JST (TESTCOIN)",
            "etomic": "0x996a8ae0304680f6a69b8a9d7c6e37d65ab5ab56",
            "rpcport": 80,
            "mm2": 1,
        },
    ]);
    let actual = update_coins_config(conf).unwrap();
    let expected = json!([
        {
            "coin": "RICK",
            "asset": "RICK",
            "fname": "RICK (TESTCOIN)",
            "rpcport": 25435,
            "txversion": 4,
            "overwintered": 1,
            "mm2": 1,
            "protocol": {
                "type": "UTXO"
            },
        },
        {
            "coin": "MORTY",
            "asset": "MORTY",
            "fname": "MORTY (TESTCOIN)",
            "rpcport": 16348,
            "txversion": 4,
            "overwintered": 1,
            "mm2": 1,
            "protocol": {
                "type": "UTXO"
            },
        },
        {
            "coin": "ETH",
            "name": "ethereum",
            "fname": "Ethereum",
            "rpcport": 80,
            "mm2": 1,
            "required_confirmations": 3,
            "protocol": {
                "type": "ETH"
            },
        },
        {
            "coin": "ARPA",
            "name": "arpa-chain",
            "fname": "ARPA Chain",
            "protocol": {
                "type": "ERC20",
                "protocol_data": {
                    "platform": "ETH",
                    "contract_address": "0xBA50933C268F567BDC86E1aC131BE072C6B0b71a"
                }
            },
            "rpcport": 80,
            "mm2": 1,
            "required_confirmations": 3,
        },
        {
            "coin": "JST",
            "name": "JST",
            "fname": "JST (TESTCOIN)",
            "rpcport": 80,
            "mm2": 1,
            "protocol": {
                "type": "ERC20",
                "protocol_data": {
                    "platform": "ETH",
                    "contract_address": "0x996a8ae0304680f6a69b8a9d7c6e37d65ab5ab56"
                }
            },
        },
    ]);
    assert_eq!(actual, expected);
}

#[test]
fn test_update_coin_config_error_not_array() {
    let conf = json!({
        "coin": "RICK",
        "asset": "RICK",
        "fname": "RICK (TESTCOIN)",
        "rpcport": 25435,
        "txversion": 4,
        "overwintered": 1,
        "mm2": 1,
    });
    let error = update_coins_config(conf).err().unwrap();
    assert!(error.contains("Coins config must be an array"));
}

#[test]
fn test_update_coin_config_error_not_object() {
    let conf = json!([["Ford", "BMW", "Fiat"]]);
    let error = update_coins_config(conf).err().unwrap();
    assert!(error.contains("Expected object, found"));
}

#[test]
fn test_update_coin_config_invalid_etomic() {
    let conf = json!([
        {
            "coin": "JST",
            "name": "JST",
            "fname": "JST (TESTCOIN)",
            "etomic": 12345678,
            "rpcport": 80,
            "mm2": 1,
        },
    ]);
    let error = update_coins_config(conf).err().unwrap();
    assert!(error.contains("Expected etomic as string, found"));
}
