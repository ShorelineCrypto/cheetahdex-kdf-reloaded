use crate::blake2b_internal::{public_key_leaf, sigs_required_leaf, standard_unlock_hash, timelock_leaf, Accumulator};
use crate::encoding::{Encodable, Encoder};
use crate::types::{Address, Hash256, PublicKey, Specifier};
use nom::bytes::complete::{take_until, take_while, take_while_m_n};
use nom::character::complete::char;
use nom::combinator::all_consuming;
use nom::combinator::map_res;
use nom::IResult;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

const POLICY_VERSION: u8 = 1u8;

/*
The full representation of the atomic swap contract is as follows:
    SpendPolicy::Threshold {
        n: 1,
        of: vec![
            SpendPolicy::Threshold {
                n: 2,
                of: vec![
                    SpendPolicy::Hash(<sha256(secret)>),
                    SpendPolicy::PublicKey(<Alice pubkey>)
                ]
            },
            SpendPolicy::Threshold {
                n: 2,
                of: vec![
                    SpendPolicy::After(<timestamp>),
                    SpendPolicy::PublicKey(<Bob pubkey>)
                ]
            },
        ]
    }

In English, the above specifies that:
    - Alice can spend the UTXO if:
        - Alice provides the preimage of the SHA256 hash specified in the UTXO (the secret)
        - Alice provides a valid signature
    - Bob can spend the UTXO if:
        - the current time is greater than the specified timestamp
        - Bob provides a valid signature

To lock funds in such a contract, we generate the address(see SpendPolicy::address) of the above SpendPolicy and use this Address in a transaction output.

The resulting UTXO can then be spent by either Alice or Bob by meeting the conditions specified above.

It is only neccesary to reveal the path that will be satisfied. The other path will be opacified(see SpendPolicy::opacify) and replaced with SpendPolicy::Opaque(<hash of unused path>).

Alice can spend the UTXO by providing a signature, the secret and revealing the relevant path within the full SpendPolicy.

Alice can construct the following SatisfiedPolicy to spend the UTXO:

SatisfiedPolicy {
    policy: SpendPolicy::Threshold {
                n: 1,
                of: vec![
                    SpendPolicy::Threshold {
                        n: 2,
                        of: vec![
                            SpendPolicy::Hash(<sha256(secret)>),
                            SpendPolicy::PublicKey(<Alice pubkey>)
                        ]
                    },
                    SpendPolicy::Opaque(<hash of unused path>),
                ]
            },
    signatures: vec![<Alice signature>],
    preimages: vec![<secret>]
}

Similarly, Bob can spend the UTXO with the following SatisfiedPolicy assuming he waits until the timestamp has passed:

SatisfiedPolicy {
    policy: SpendPolicy::Threshold {
                n: 1,
                of: vec![
                    SpendPolicy::Opaque(<hash of unused path>),
                    SpendPolicy::Threshold {
                        n: 2,
                        of: vec![
                            SpendPolicy::After(<timestamp>),
                            SpendPolicy::PublicKey(<Bob pubkey>)
                        ]
                    }
                ]
            },
    signatures: vec![<Bob signature>],
    preimages: vec![]
}

*/

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "policy")]
pub enum SpendPolicy {
    #[serde(rename = "above")]
    Above(u64),
    #[serde(rename = "after")]
    After(u64),
    #[serde(rename = "pk")]
    PublicKey(PublicKey),
    #[serde(rename = "h")]
    Hash(Hash256),
    #[serde(rename = "thresh")]
    Threshold { n: u8, of: Vec<SpendPolicy> },
    #[serde(rename = "opaque")]
    Opaque(Address),
    #[serde(rename = "uc")]
    UnlockConditions(UnlockCondition), // For v1 compatibility
}

impl Encodable for SpendPolicy {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u8(POLICY_VERSION);
        self.encode_wo_prefix(encoder);
    }
}

impl SpendPolicy {
    pub fn to_u8(&self) -> u8 {
        match self {
            SpendPolicy::Above(_) => 1,
            SpendPolicy::After(_) => 2,
            SpendPolicy::PublicKey(_) => 3,
            SpendPolicy::Hash(_) => 4,
            SpendPolicy::Threshold { n: _, of: _ } => 5,
            SpendPolicy::Opaque(_) => 6,
            SpendPolicy::UnlockConditions(_) => 7,
        }
    }

    pub fn encode_wo_prefix(&self, encoder: &mut Encoder) {
        let opcode = self.to_u8();
        match self {
            SpendPolicy::Above(height) => {
                encoder.write_u8(opcode);
                encoder.write_u64(*height);
            },
            SpendPolicy::After(time) => {
                encoder.write_u8(opcode);
                encoder.write_u64(*time);
            },
            SpendPolicy::PublicKey(pubkey) => {
                encoder.write_u8(opcode);
                encoder.write_slice(&pubkey.to_bytes());
            },
            SpendPolicy::Hash(hash) => {
                encoder.write_u8(opcode);
                encoder.write_slice(&hash.0);
            },
            SpendPolicy::Threshold { n, of } => {
                encoder.write_u8(opcode);
                encoder.write_u8(*n);
                encoder.write_u8(of.len() as u8);
                for policy in of {
                    policy.encode_wo_prefix(encoder);
                }
            },
            SpendPolicy::Opaque(address) => {
                encoder.write_u8(opcode);
                encoder.write_slice(&address.0 .0);
            },
            SpendPolicy::UnlockConditions(unlock_condition) => {
                encoder.write_u8(opcode);
                encoder.write_u64(unlock_condition.timelock);
                encoder.write_u64(unlock_condition.unlock_keys.len() as u64);
                for uc in &unlock_condition.unlock_keys {
                    uc.encode(encoder);
                }
                encoder.write_u64(unlock_condition.signatures_required);
            },
        }
    }

    pub fn address(&self) -> Address {
        if let SpendPolicy::UnlockConditions(unlock_condition) = self {
            return unlock_condition.address();
        }
        let mut encoder = Encoder::default();
        encoder.write_distinguisher("address");

        // if self is a threshold policy, we need to convert all of its subpolicies to opaque
        let new_policy = match self {
            SpendPolicy::Threshold { n, of } => SpendPolicy::Threshold {
                n: *n,
                of: of.iter().map(SpendPolicy::opaque).collect(),
            },
            _ => self.clone(),
        };
        new_policy.encode(&mut encoder);

        Address(encoder.hash())
    }

    pub fn above(height: u64) -> Self { SpendPolicy::Above(height) }

    pub fn after(time: u64) -> Self { SpendPolicy::After(time) }

    pub fn public_key(pk: PublicKey) -> Self { SpendPolicy::PublicKey(pk) }

    pub fn hash(h: Hash256) -> Self { SpendPolicy::Hash(h) }

    pub fn threshold(n: u8, of: Vec<SpendPolicy>) -> Self { SpendPolicy::Threshold { n, of } }

    pub fn opaque(p: &SpendPolicy) -> Self { SpendPolicy::Opaque(p.address()) }

    pub fn unlock_condition(pubkeys: Vec<PublicKey>, timelock: u64, signatures_required: u64) -> Self {
        SpendPolicy::UnlockConditions(UnlockCondition::new(pubkeys, timelock, signatures_required))
    }

    pub fn anyone_can_spend() -> Self { SpendPolicy::threshold(0, vec![]) }

    pub fn opacify(&self) -> Self { SpendPolicy::Opaque(self.address()) }
}

impl SpendPolicy {
    /// Create a HTLC SpendPolicy.
    /// Arguments:
    /// - success_public_key: the public key that is able to claim the funds immediately by providing
    ///     the preimage of `secret_hash`
    /// - refund_public_key: the public key that is able to claim the funds after `lock_time`
    /// - lock_time: the timestamp after which `refund_public_key` can claim the funds
    /// - secret_hash: the sha256 hash of the secret
    pub fn atomic_swap(
        success_public_key: &PublicKey,
        refund_public_key: &PublicKey,
        lock_time: u64,
        secret_hash: &Hash256,
    ) -> Self {
        let policy_after = SpendPolicy::After(lock_time);
        let policy_hash = SpendPolicy::Hash(secret_hash.clone());

        let policy_success = SpendPolicy::Threshold {
            n: 2,
            of: vec![SpendPolicy::PublicKey(success_public_key.clone()), policy_hash],
        };

        let policy_refund = SpendPolicy::Threshold {
            n: 2,
            of: vec![SpendPolicy::PublicKey(refund_public_key.clone()), policy_after],
        };

        SpendPolicy::Threshold {
            n: 1,
            of: vec![policy_success, policy_refund],
        }
    }

    pub fn atomic_swap_success(
        success_public_key: &PublicKey,
        refund_public_key: &PublicKey,
        lock_time: u64,
        secret_hash: &Hash256,
    ) -> Self {
        match Self::atomic_swap(success_public_key, refund_public_key, lock_time, secret_hash) {
            SpendPolicy::Threshold { n, mut of } => {
                of[1] = of[1].opacify();
                SpendPolicy::Threshold { n, of }
            },
            _ => unreachable!(),
        }
    }

    pub fn atomic_swap_refund(
        success_public_key: &PublicKey,
        refund_public_key: &PublicKey,
        lock_time: u64,
        secret_hash: &Hash256,
    ) -> Self {
        match Self::atomic_swap(success_public_key, refund_public_key, lock_time, secret_hash) {
            SpendPolicy::Threshold { n, mut of } => {
                of[0] = of[0].opacify();
                SpendPolicy::Threshold { n, of }
            },
            _ => unreachable!(),
        }
    }
}

/// Sia Go v1 technically supports arbitrary length public keys
/// We only support ed25519 but must be able to deserialize others
/// This data structure deviates from the Go implementation
#[derive(Clone, Debug, PartialEq)]
pub enum UnlockKey {
    Ed25519(PublicKey),
    NonStandard { algorithm: Specifier, public_key: Vec<u8> },
}

impl<'de> Deserialize<'de> for UnlockKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UnlockKeyVisitor;

        impl<'de> serde::de::Visitor<'de> for UnlockKeyVisitor {
            type Value = UnlockKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string representing a Sia v1 UnlockKey; most often 'ed25519:<hex>'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match UnlockKey::from_str(value) {
                    Ok(key) => Ok(key),
                    Err(e) => Err(E::custom(format!("failed to parse UnlockKey: {}", e.0))),
                }
            }
        }

        deserializer.deserialize_str(UnlockKeyVisitor)
    }
}

impl Serialize for UnlockKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn parse_specifier(input: &str) -> IResult<&str, Specifier> {
    let (input, prefix_str) = take_until(":")(input)?;
    let specifier = Specifier::from_str_expect(prefix_str);
    let (input, _) = char(':')(input)?;
    Ok((input, specifier))
}

fn parse_unlock_key(input: &str) -> IResult<&str, UnlockKey> {
    let (input, specifier) = parse_specifier(input)?;
    match specifier {
        Specifier::Ed25519 => {
            let (input, public_key) = map_res(
                all_consuming(map_res(
                    take_while_m_n(64, 64, |c: char| c.is_ascii_hexdigit()),
                    hex::decode,
                )),
                |bytes: Vec<u8>| PublicKey::from_bytes(&bytes),
            )(input)?;
            Ok((input, UnlockKey::Ed25519(public_key)))
        },
        _ => {
            let (input, public_key) =
                all_consuming(map_res(take_while(|c: char| c.is_ascii_hexdigit()), |hex_str: &str| {
                    hex::decode(hex_str)
                }))(input)?;
            Ok((input, UnlockKey::NonStandard {
                algorithm: specifier,
                public_key,
            }))
        },
    }
}

#[derive(Debug)]
pub struct UnlockKeyParseError(pub String);

impl FromStr for UnlockKey {
    type Err = UnlockKeyParseError;

    fn from_str(input: &str) -> Result<UnlockKey, Self::Err> {
        match all_consuming(parse_unlock_key)(input) {
            Ok((_, key)) => Ok(key),
            Err(e) => Err(UnlockKeyParseError(e.to_string())), // TODO unit test to check how verbose or useful this is
        }
    }
}

impl fmt::Display for UnlockKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnlockKey::Ed25519(public_key) => write!(f, "ed25519:{}", hex::encode(public_key.as_bytes())),
            UnlockKey::NonStandard { algorithm, public_key } => {
                write!(f, "{}:{}", algorithm, hex::encode(public_key))
            },
        }
    }
}

impl Encodable for PublicKey {
    fn encode(&self, encoder: &mut Encoder) { encoder.write_slice(&self.to_bytes()); }
}

impl Encodable for UnlockKey {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            UnlockKey::Ed25519(public_key) => {
                Specifier::Ed25519.encode(encoder);
                encoder.write_u64(32); // ed25519 public key length
                public_key.encode(encoder);
            },
            UnlockKey::NonStandard { algorithm, public_key } => {
                algorithm.encode(encoder);
                encoder.write_u64(public_key.len() as u64);
                encoder.write_slice(public_key);
            },
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockCondition {
    #[serde(rename = "publicKeys")]
    pub unlock_keys: Vec<UnlockKey>,
    pub timelock: u64,
    pub signatures_required: u64,
}

impl Encodable for UnlockCondition {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.timelock);
        encoder.write_u64(self.unlock_keys.len() as u64);
        for unlock_key in &self.unlock_keys {
            unlock_key.encode(encoder);
        }
        encoder.write_u64(self.signatures_required);
    }
}

impl UnlockCondition {
    pub fn new(pubkeys: Vec<PublicKey>, timelock: u64, signatures_required: u64) -> Self {
        let unlock_keys = pubkeys.into_iter().map(UnlockKey::Ed25519).collect();

        UnlockCondition {
            unlock_keys,
            timelock,
            signatures_required,
        }
    }

    pub fn standard_unlock(public_key: PublicKey) -> Self {
        UnlockCondition {
            unlock_keys: vec![UnlockKey::Ed25519(public_key)],
            timelock: 0,
            signatures_required: 1,
        }
    }

    pub fn unlock_hash(&self) -> Hash256 {
        // almost all UnlockConditions are standard, so optimize for that case
        if let UnlockKey::Ed25519(public_key) = &self.unlock_keys[0] {
            if self.timelock == 0 && self.unlock_keys.len() == 1 && self.signatures_required == 1 {
                return standard_unlock_hash(public_key);
            }
        }

        let mut accumulator = Accumulator::default();

        accumulator.add_leaf(timelock_leaf(self.timelock));

        for unlock_key in &self.unlock_keys {
            accumulator.add_leaf(public_key_leaf(unlock_key));
        }

        accumulator.add_leaf(sigs_required_leaf(self.signatures_required));
        accumulator.root()
    }

    pub fn address(&self) -> Address { Address(self.unlock_hash()) }
}
