// SPDX-License-Identifier: GPL-2.0-or-later
// Independent fixtures from ZIP-212 and the public Sapling encryption APIs.
use super::*;
use rand::{rngs::StdRng, SeedableRng};
use sapling::bundle::{Authorized, OutputDescription};
use sapling::keys::PreparedIncomingViewingKey;
use sapling::note_encryption::{sapling_note_encryption, try_sapling_compact_note_decryption,
                               try_sapling_note_decryption, CompactOutputDescription, SaplingDomain};
use sapling::value::{NoteValue, ValueCommitTrapdoor, ValueCommitment};
use sapling::Rseed;
use zcash_note_encryption::{Domain, NoteEncryption};
use zcash_primitives::transaction::components::sapling::zip212_enforcement;
use zcash_primitives::transaction::{TransactionData, TxVersion};
use zcash_protocol::consensus::ZIP212_GRACE_PERIOD;
use zcash_protocol::value::ZatBalance;

pub(super) fn pirate_params() -> ZcoinConsensusParams {
    serde_json::from_value(json!({
        "overwinter_activation_height": 152855,
        "sapling_activation_height": 152855,
        "blossom_activation_height": null,
        "heartwood_activation_height": null,
        "canopy_activation_height": null,
        "coin_type": 133,
        "hrp_sapling_extended_spending_key": "secret-extended-key-main",
        "hrp_sapling_extended_full_viewing_key": "zxviews",
        "hrp_sapling_payment_address": "zs",
        "b58_pubkey_address_prefix": [28, 184],
        "b58_script_address_prefix": [28, 189]
    }))
    .unwrap()
}

pub(super) fn note_for_version(extfvk: &ExtendedFullViewingKey, value: u64, version: u8) -> Note {
    let note = Note::from_parts(
        extfvk.default_address().1,
        NoteValue::from_raw(value),
        Rseed::AfterZip212([42; 32]),
    );
    match version {
        1 => Note::from_parts(note.recipient(), note.value(), Rseed::BeforeZip212(note.rcm())),
        2 => note,
        _ => panic!("unsupported fixture version"),
    }
}

fn encrypted_output(
    extfvk: &ExtendedFullViewingKey,
    version: u8,
    inconsistent_esk: bool,
) -> (OutputDescription<[u8; 192]>, CompactOutputDescription) {
    let note = note_for_version(extfvk, 123_456, version);
    let mut rng = StdRng::from_seed([19; 32]);
    let encryptor = if inconsistent_esk {
        let other_note = Note::from_parts(note.recipient(), note.value(), Rseed::AfterZip212([43; 32]));
        NoteEncryption::<SaplingDomain>::new_with_esk(
            SaplingDomain::derive_esk(&other_note).unwrap(),
            Some(extfvk.fvk.ovk),
            note.clone(),
            MemoBytes::empty().into_bytes(),
        )
    } else {
        sapling_note_encryption(
            Some(extfvk.fvk.ovk),
            note.clone(),
            MemoBytes::empty().into_bytes(),
            &mut rng,
        )
    };
    let cv = ValueCommitment::derive(note.value(), ValueCommitTrapdoor::random(&mut rng));
    let cmu = note.cmu();
    let ephemeral_key = SaplingDomain::epk_bytes(encryptor.epk());
    let ciphertext = encryptor.encrypt_note_plaintext();
    let outgoing = encryptor.encrypt_outgoing_plaintext(&cv, &cmu, &mut rng);
    let compact = CompactOutputDescription {
        ephemeral_key: ephemeral_key.clone(),
        cmu,
        enc_ciphertext: ciphertext[..52].try_into().unwrap(),
    };
    // Proof and binding-signature bytes are placeholders: these fixtures test
    // encrypted note recognition, not transaction proof verification/broadcast.
    (
        OutputDescription::from_parts(cv, cmu, ephemeral_key, ciphertext, outgoing, [0; 192]),
        compact,
    )
}

fn transaction_for_output(output: OutputDescription<[u8; 192]>) -> ZTransaction {
    let bundle = sapling::Bundle::from_parts(
        vec![],
        vec![output],
        ZatBalance::from_i64(-123_456).unwrap(),
        Authorized {
            binding_sig: [0; 64].into(),
        },
    );
    TransactionData::from_parts(
        TxVersion::V4,
        BranchId::Sapling,
        0,
        BlockHeight::from_u32(152_900),
        None,
        None,
        bundle,
        None,
    )
    .freeze()
    .unwrap()
}

#[test]
fn pirate_zip212_policy_requires_every_identity_field_and_preserves_sending() {
    let params = pirate_params();
    for (name, value) in [
        ("overwinter_activation_height", json!(152854)),
        ("sapling_activation_height", json!(152856)),
        ("coin_type", json!(141)),
        ("hrp_sapling_extended_spending_key", json!("different")),
        ("hrp_sapling_extended_full_viewing_key", json!("different")),
        ("hrp_sapling_payment_address", json!("different")),
        ("b58_pubkey_address_prefix", json!([28, 185])),
        ("b58_script_address_prefix", json!([28, 190])),
    ] {
        let mut conf = serde_json::to_value(&params).unwrap();
        conf[name] = value;
        let different: ZcoinConsensusParams = serde_json::from_value(conf).unwrap();
        assert!(different.sapling_receive_override().is_none(), "{name}");
    }
    for height in [152_855, 4_000_000] {
        let height = BlockHeight::from_u32(height);
        assert!(matches!(
            params.sapling_receive_enforcement(height),
            Zip212Enforcement::GracePeriod
        ));
        assert!(matches!(zip212_enforcement(&params, height), Zip212Enforcement::Off));
        assert_eq!(BranchId::for_height(&params, height), BranchId::Sapling);
        assert_eq!(
            TxVersion::suggested_for_branch(BranchId::for_height(&params, height)),
            TxVersion::V4
        );
        assert_eq!(params.activation_height(NetworkUpgrade::Canopy), None);
    }
    let mut later_canopy = params;
    later_canopy.canopy_activation_height = Some(152_856);
    assert!(matches!(
        later_canopy.sapling_receive_enforcement(BlockHeight::from_u32(4_000_000)),
        Zip212Enforcement::GracePeriod
    ));
    assert!(matches!(
        zip212_enforcement(&later_canopy, BlockHeight::from_u32(4_000_000)),
        Zip212Enforcement::On
    ));
}

#[test]
fn pirate_zip212_compact_full_and_outgoing_recovery_accept_both_versions() {
    let params = pirate_params();
    #[allow(deprecated)]
    let extfvk = ExtendedSpendingKey::master(&[7; 32]).to_extended_full_viewing_key();
    let ivk = PreparedIncomingViewingKey::new(&extfvk.fvk.vk.ivk());
    let ufvk = UnifiedFullViewingKey::from_sapling_extended_full_viewing_key(extfvk.clone()).unwrap();
    let keys = HashMap::from([(0u32, ufvk)]);
    let height = BlockHeight::from_u32(152_855);
    let enforcement = params.sapling_receive_enforcement(height);
    for version in [1, 2] {
        let (full, compact) = encrypted_output(&extfvk, version, false);
        assert_eq!(
            try_sapling_compact_note_decryption(&ivk, &compact, enforcement)
                .unwrap()
                .0
                .value()
                .inner(),
            123_456
        );
        assert_eq!(
            try_sapling_note_decryption(&ivk, &full, enforcement)
                .unwrap()
                .0
                .value()
                .inner(),
            123_456
        );
        assert_eq!(
            try_sapling_output_recovery(&extfvk.fvk.ovk, &full, enforcement)
                .unwrap()
                .0
                .value()
                .inner(),
            123_456
        );
        let tx = transaction_for_output(full);
        let decrypted = decrypt_transaction_with_zip212_enforcement(
            &params,
            Some(height),
            None,
            &tx,
            &keys,
            params.sapling_receive_override(),
        );
        assert_eq!(decrypted.sapling_outputs().len(), 1);
        let strict = zcash_client_backend::decrypt_transaction(&params, Some(height), None, &tx, &keys);
        assert_eq!(strict.sapling_outputs().len(), usize::from(version == 1));
    }
}

#[test]
fn pirate_zip212_rejects_wrong_keys_commitments_versions_and_authenticated_wrong_ephemeral_key() {
    #[allow(deprecated)]
    let extfvk = ExtendedSpendingKey::master(&[8; 32]).to_extended_full_viewing_key();
    #[allow(deprecated)]
    let other = ExtendedSpendingKey::master(&[9; 32]).to_extended_full_viewing_key();
    let ivk = PreparedIncomingViewingKey::new(&extfvk.fvk.vk.ivk());
    let other_ivk = PreparedIncomingViewingKey::new(&other.fvk.vk.ivk());
    let policy = pirate_params().sapling_receive_enforcement(BlockHeight::from_u32(4_000_000));
    for version in [1, 2] {
        let (full, mut compact) = encrypted_output(&extfvk, version, false);
        assert!(try_sapling_compact_note_decryption(&other_ivk, &compact, policy).is_none());
        assert!(try_sapling_note_decryption(&other_ivk, &full, policy).is_none());
        assert!(try_sapling_output_recovery(&other.fvk.ovk, &full, policy).is_none());
        compact.cmu = note_for_version(&other, 123_456, version).cmu();
        assert!(try_sapling_compact_note_decryption(&ivk, &compact, policy).is_none());
    }
    let (_, mut bad_version) = encrypted_output(&extfvk, 2, false);
    bad_version.enc_ciphertext[0] ^= 1; // Compact plaintext lead byte 0x02 -> 0x03.
    assert!(try_sapling_compact_note_decryption(&ivk, &bad_version, policy).is_none());
    // The ciphertext authenticates using this output's actual ephemeral key;
    // rejection must come from ZIP-212's rseed-derived key consistency check.
    let (full, compact) = encrypted_output(&extfvk, 2, true);
    assert!(try_sapling_compact_note_decryption(&ivk, &compact, policy).is_none());
    assert!(try_sapling_note_decryption(&ivk, &full, policy).is_none());
    assert!(try_sapling_output_recovery(&extfvk.fvk.ovk, &full, policy).is_none());
}

#[test]
fn non_pirate_zip212_boundaries_remain_strict() {
    let mut params = pirate_params();
    params.overwinter_activation_height = 347_500;
    params.sapling_activation_height = 419_200;
    params.canopy_activation_height = Some(1_046_400);
    #[allow(deprecated)]
    let extfvk = ExtendedSpendingKey::master(&[10; 32]).to_extended_full_viewing_key();
    let ivk = PreparedIncomingViewingKey::new(&extfvk.fvk.vk.ivk());
    let canopy = params.canopy_activation_height.unwrap();
    for (height, accepts_v1, accepts_v2) in [
        (canopy - 1, true, false),
        (canopy, true, true),
        (canopy + ZIP212_GRACE_PERIOD - 1, true, true),
        (canopy + ZIP212_GRACE_PERIOD, false, true),
    ] {
        assert!(params.sapling_receive_override().is_none());
        let policy = params.sapling_receive_enforcement(BlockHeight::from_u32(height));
        for (version, expected) in [(1, accepts_v1), (2, accepts_v2)] {
            let (full, compact) = encrypted_output(&extfvk, version, false);
            assert_eq!(
                try_sapling_compact_note_decryption(&ivk, &compact, policy).is_some(),
                expected
            );
            assert_eq!(try_sapling_note_decryption(&ivk, &full, policy).is_some(), expected);
            assert_eq!(
                try_sapling_output_recovery(&extfvk.fvk.ovk, &full, policy).is_some(),
                expected
            );
        }
    }
}
