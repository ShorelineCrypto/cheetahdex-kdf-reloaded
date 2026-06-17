#[cfg(test)]
mod test {
    macro_rules! test_serde {
        ($type:ty, $json_value:expr) => {{
            let json_str = $json_value.to_string();
            let value: $type = serde_json::from_str(&json_str).unwrap();
            let serialized = serde_json::to_string(&value).unwrap();
            let serialized_json_value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
            assert_eq!($json_value, serialized_json_value);
        }};
    }
    // Ensure the original value matches the value after round-trip (serialize -> deserialize -> serialize)
    use crate::types::{
        Address, Event, Hash256, SiacoinElement, SiacoinOutput, StateElement, UnlockKey, V2Transaction,
    };

    cross_target_tests! {
            fn test_serde_address() {
                test_serde!(
                    Address,
                    json!("591fcf237f8854b5653d1ac84ae4c107b37f148c3c7b413f292d48db0c25a8840be0653e411f")
                );
            }

            fn test_serde_unlock_key() {
                test_serde!(
                    UnlockKey,
                    json!("ed25519:0102030000000000000000000000000000000000000000000000000000000000")
                );
            }

            fn test_serde_sia_hash() {
                test_serde!(
                    Hash256,
                    json!("dc07e5bf84fbda867a7ed7ca80c6d1d81db05cef16ff38f6ba80b6bf01e1ddb1")
                );
            }

            fn test_serde_siacoin_output() {
                let j = json!({
                    "value": "300000000000000000000000000000",
                    "address": "591fcf237f8854b5653d1ac84ae4c107b37f148c3c7b413f292d48db0c25a8840be0653e411f"
                });
                test_serde!(SiacoinOutput, j);
            }

            // check that merkleProof field serde is the same when it is null, missing or empty
            fn test_serde_state_element() {
                let j = json!({
                    "id": "dc07e5bf84fbda867a7ed7ca80c6d1d81db05cef16ff38f6ba80b6bf01e1ddb1",
                    "leafIndex": 21,
                    "merkleProof": null
                });
                let null_proof = serde_json::from_value::<StateElement>(j).unwrap();

                let j = json!({
                    "id": "dc07e5bf84fbda867a7ed7ca80c6d1d81db05cef16ff38f6ba80b6bf01e1ddb1",
                    "leafIndex": 21,
                    "merkleProof": []
                });
                let empty_proof = serde_json::from_value::<StateElement>(j).unwrap();

                let j = json!({
                    "id": "dc07e5bf84fbda867a7ed7ca80c6d1d81db05cef16ff38f6ba80b6bf01e1ddb1",
                    "leafIndex": 21
                });
                let missing_proof = serde_json::from_value::<StateElement>(j).unwrap();

                assert_eq!(null_proof, empty_proof);
                assert_eq!(null_proof, missing_proof);
            }

            fn test_serde_siacoin_element() {
                let j = json!(  {
                    "id": "0102030000000000000000000000000000000000000000000000000000000000",
                    "stateElement": {
                        "leafIndex": 1,
                        "merkleProof": [
                            "0405060000000000000000000000000000000000000000000000000000000000",
                            "0708090000000000000000000000000000000000000000000000000000000000"
                        ]
                    },
                    "siacoinOutput": {
                        "value": "1",
                        "address": "72b0762b382d4c251af5ae25b6777d908726d75962e5224f98d7f619bb39515dd64b9a56043a"
                    },
                    "maturityHeight": 0
                }
            );
            serde_json::from_value::<SiacoinElement>(j).unwrap();
        }

        fn test_serde_siacoin_element_missing_merkle_proof() {
            let json_str = r#"
            {
                "id": "16406893374eb18eeea95e8c0d6b6c325275ecb99cf2fec7a6708b0b8def75bd",
                "stateElement": {
                    "leafIndex": 391
                },
                "siacoinOutput": {
                    "value": "10000000000000000000000000000",
                    "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                },
                "maturityHeight": 334
            }"#;
        serde_json::from_str::<SiacoinElement>(json_str).unwrap();
    }

    fn test_serde_event_v2_contract_resolution_storage_proof() {
        let j = r#"
            {
                "id": "16406893374eb18eeea95e8c0d6b6c325275ecb99cf2fec7a6708b0b8def75bd",
                "index": {
                "height": 190,
                "id": "22693d8885ad7b5e2abf22fe838fd6ae9856142f898607ffd2ddb8dd3d7ca67b"
                },
                "confirmations": 17,
                "type": "v2ContractResolution",
                "data": {
                "resolution": {
                    "parent": {
                    "id": "e5adb3e8e49d9bd29e54966e809cc652f08dfca2183fad00f3da29df83f65091",
                    "stateElement": {
                        "leafIndex": 351
                    },
                    "v2FileContract": {
                        "capacity": 0,
                        "filesize": 0,
                        "fileMerkleRoot": "0000000000000000000000000000000000000000000000000000000000000000",
                        "proofHeight": 179,
                        "expirationHeight": 189,
                        "renterOutput": {
                        "value": "10000000000000000000000000000",
                        "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                        },
                        "hostOutput": {
                        "value": "0",
                        "address": "000000000000000000000000000000000000000000000000000000000000000089eb0d6a8a69"
                        },
                        "missedHostValue": "0",
                        "totalCollateral": "0",
                        "renterPublicKey": "ed25519:cecc1507dc1ddd7295951c290888f095adb9044d1b73d696e6df065d683bd4fc",
                        "hostPublicKey": "ed25519:cecc1507dc1ddd7295951c290888f095adb9044d1b73d696e6df065d683bd4fc",
                        "revisionNumber": 0,
                        "renterSignature": "88b5f53a69759264f60cb227e7d4fdb25ee185f9c9b9bcf4f6e94c413ace76e1d1dcf72d509670e3d4e89d3dccb326d9c74411909e0a2e0e7e1e18bf3acb6c0c",
                        "hostSignature": "88b5f53a69759264f60cb227e7d4fdb25ee185f9c9b9bcf4f6e94c413ace76e1d1dcf72d509670e3d4e89d3dccb326d9c74411909e0a2e0e7e1e18bf3acb6c0c"
                    }
                    },
                    "type": "expiration",
                    "resolution": {}
                },
                "siacoinElement": {
                    "id": "16406893374eb18eeea95e8c0d6b6c325275ecb99cf2fec7a6708b0b8def75bd",
                    "stateElement": {
                    "leafIndex": 391
                    },
                    "siacoinOutput": {
                    "value": "10000000000000000000000000000",
                    "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                    },
                    "maturityHeight": 334
                },
                "missed": true
                },
                "maturityHeight": 334,
                "timestamp": "2024-11-15T19:41:06Z",
                "relevant": [
                "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                ]
            }
        "#;

        let _event = serde_json::from_str::<Event>(j).unwrap();
    }

    fn test_serde_event_v2_contract_resolution_renewal() {
        let json_str = r#"
            {
              "id": "3edfd4c248fafa5e748f731053f5fbc1bd83639495c6b7abcb03d1dcf4c097c7",
              "index": {
                  "height": 529206,
                  "id": "000000000000000064b0bd5ebf133acf200e581de35456a5a1e2a75c806ee400"
              },
              "confirmations": 12,
              "type": "v2ContractResolution",
              "data": {
                  "resolution": {
                      "parent": {
                          "id": "4568c44b84aad06b03522d8de8b279939e2256ca3acbf08156377a81b505e2a9",
                          "stateElement": {
                              "leafIndex": 73053567
                          },
                          "v2FileContract": {
                              "capacity": 63514345472,
                              "filesize": 63514345472,
                              "fileMerkleRoot": "480d8a1e8d741122a1b975dd429fa454b5a27cf3d08e9600b2b183d42d22593e",
                              "proofHeight": 536952,
                              "expirationHeight": 537096,
                              "renterOutput": {
                                  "value": "6327972277082229661619200",
                                  "address": "b708246a0afc0643d853210e2a0060c910fad443131e11e43bf7b8b897b77cbf5f87fad52010"
                              },
                              "hostOutput": {
                                  "value": "377357734175943549249134800",
                                  "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                              },
                              "missedHostValue": "6335836251212147929683472",
                              "totalCollateral": "248607850108104194948634128",
                              "renterPublicKey": "ed25519:2272bcf65a700be3495a1609ea8b6b198584267aa5c017d1e40d487e178f9fdd",
                              "hostPublicKey": "ed25519:36c8b07e61548a57e16dfabdfcc07dc157974a75010ab1684643d933e83fa7b1",
                              "revisionNumber": 0,
                              "renterSignature": "4cdec1d7bd28e7d7d85bee263f0a54f8c3423825cdb3b80aadd2c13a855b7f959aa5af02b9ec1f74ad72591ce822a856eedfc3c21552e89da30f95e84d2e0404",
                              "hostSignature": "709157d090960cc50a70eeef2e52d48acb931b0d0219c70944cf26f8df80bd7d17812a3d69d42490136b31476318d60d7d63e19b23beb1cfdabb78f19b0d3d0d"
                          }
                      },
                      "type": "renewal",
                      "resolution": {
                          "finalRenterOutput": {
                              "value": "0",
                              "address": "b708246a0afc0643d853210e2a0060c910fad443131e11e43bf7b8b897b77cbf5f87fad52010"
                          },
                          "finalHostOutput": {
                              "value": "0",
                              "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                          },
                          "renterRollover": "3166655994568310918300672",
                          "hostRollover": "380519050458457467992453328",
                          "newContract": {
                              "capacity": 65246593024,
                              "filesize": 65246593024,
                              "fileMerkleRoot": "515c425aa2858b24f3a869c1e077643ded8eb9473bee6dba99693cfbf90b0089",
                              "proofHeight": 536952,
                              "expirationHeight": 537096,
                              "renterOutput": {
                                  "value": "6333311989136621836601344",
                                  "address": "b708246a0afc0643d853210e2a0060c910fad443131e11e43bf7b8b897b77cbf5f87fad52010"
                              },
                              "hostOutput": {
                                  "value": "387052362447593831740552934",
                                  "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                              },
                              "missedHostValue": "6346797147533652559207462",
                              "totalCollateral": "254941162097240558696733734",
                              "renterPublicKey": "ed25519:2272bcf65a700be3495a1609ea8b6b198584267aa5c017d1e40d487e178f9fdd",
                              "hostPublicKey": "ed25519:36c8b07e61548a57e16dfabdfcc07dc157974a75010ab1684643d933e83fa7b1",
                              "revisionNumber": 0,
                              "renterSignature": "e69e4559ac8ba2d51ea8a3e54b1edfa4dc047765f7b9fc928137ad5906031e14eab2dbdba714ee69dc0dd2d5583e289eb7dec54668c637890a5783768ee9fe0f",
                              "hostSignature": "380153bae8d813b14c8c59e5db8b268ee3f6b399e4c4145f66ce9fd248f518ffaa0fd7c86350f02d93d8f05f7c7e1d185f55047f0fb342b3996b908235108009"
                          },
                          "renterSignature": "007a82d04bf9714f15ef83ce624c7ccbb728c084872af0e0358c6747a442d589cf7317a324988c7418aec8ce18ccc9c20a5112f9747628f4b23352dc7cb50004",
                          "hostSignature": "1f17cea3c98df8dbb6fda248c027b996484addd418bcada4e176d676d9e9fd15b7bfbbf9692ecb5063e77ccf4e89e3a33367dc44d3d440d2c238562f7d4ced00"
                      }
                  },
                  "siacoinElement": {
                      "id": "3edfd4c248fafa5e748f731053f5fbc1bd83639495c6b7abcb03d1dcf4c097c7",
                      "stateElement": {
                          "leafIndex": 73054828
                      },
                      "siacoinOutput": {
                          "value": "0",
                          "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                      },
                      "maturityHeight": 529350
                  },
                  "missed": false
              },
              "maturityHeight": 529350,
              "timestamp": "2025-06-25T19:03:22Z",
              "relevant": [
                  "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
              ]
          }
        "#;

        let _event = serde_json::from_str::<Event>(json_str).unwrap();
    }

    fn test_serde_event_v2_contract_resolution_expiration() {
        let j = json!(
            {
                "id": "16406893374eb18eeea95e8c0d6b6c325275ecb99cf2fec7a6708b0b8def75bd",
                "index": {
                  "height": 190,
                  "id": "22693d8885ad7b5e2abf22fe838fd6ae9856142f898607ffd2ddb8dd3d7ca67b"
                },
                "confirmations": 17,
                "type": "v2ContractResolution",
                "data": {
                  "resolution": {
                    "parent": {
                      "id": "e5adb3e8e49d9bd29e54966e809cc652f08dfca2183fad00f3da29df83f65091",
                      "stateElement": {
                        "leafIndex": 351
                      },
                      "v2FileContract": {
                        "capacity": 0,
                        "filesize": 0,
                        "fileMerkleRoot": "0000000000000000000000000000000000000000000000000000000000000000",
                        "proofHeight": 179,
                        "expirationHeight": 189,
                        "renterOutput": {
                          "value": "10000000000000000000000000000",
                          "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                        },
                        "hostOutput": {
                          "value": "0",
                          "address": "000000000000000000000000000000000000000000000000000000000000000089eb0d6a8a69"
                        },
                        "missedHostValue": "0",
                        "totalCollateral": "0",
                        "renterPublicKey": "ed25519:cecc1507dc1ddd7295951c290888f095adb9044d1b73d696e6df065d683bd4fc",
                        "hostPublicKey": "ed25519:cecc1507dc1ddd7295951c290888f095adb9044d1b73d696e6df065d683bd4fc",
                        "revisionNumber": 0,
                        "renterSignature": "88b5f53a69759264f60cb227e7d4fdb25ee185f9c9b9bcf4f6e94c413ace76e1d1dcf72d509670e3d4e89d3dccb326d9c74411909e0a2e0e7e1e18bf3acb6c0c",
                        "hostSignature": "88b5f53a69759264f60cb227e7d4fdb25ee185f9c9b9bcf4f6e94c413ace76e1d1dcf72d509670e3d4e89d3dccb326d9c74411909e0a2e0e7e1e18bf3acb6c0c"
                      }
                    },
                    "type": "expiration",
                    "resolution": {}
                  },
                  "siacoinElement": {
                    "id": "16406893374eb18eeea95e8c0d6b6c325275ecb99cf2fec7a6708b0b8def75bd",
                    "stateElement": {
                      "leafIndex": 391
                    },
                    "siacoinOutput": {
                      "value": "10000000000000000000000000000",
                      "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                    },
                    "maturityHeight": 334
                  },
                  "missed": true
                },
                "maturityHeight": 334,
                "timestamp": "2024-11-15T19:41:06Z",
                "relevant": [
                  "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                ]
              }
        );

        let _event = serde_json::from_value::<Event>(j).unwrap();
    }

    fn test_serde_event_v2_transaction() {
        let j = json!(
            {
                "id": "3203cda6aa67faca699fc9fd1e75d46cfa0ee080ddaf5485fad9bc42282a04b9",
                "index": {
                  "height": 169,
                  "id": "d4b10532623709b888fb6f2a2c6d865dc3d21f4d768f83c7f43814c29acf5b2b"
                },
                "confirmations": 38,
                "type": "v2Transaction",
                "data": {
                  "siacoinInputs": [
                    {
                      "parent": {
                        "id": "a97dab89d5ba12e2c3ea852021e3be6b4472e55fc5408497d38fbfd05fd98362",
                        "stateElement": {
                          "leafIndex": 302,
                          "merkleProof": [
                            "98c9f7eee6105d146b9374c9d7e28d8cec7ffcf95e71a33630510b90ef3b4fbb",
                            "e00568ef169225bb1e049e8c6435809396bee2da99595f870d834d3deb436df9",
                            "cd725e13fac773e43b5492ea5ffae6003ff7e3cacc4505689080fd657558a983",
                            "5dc34e64ffe5fdc537bc1021fbb9469e970b5a362a93acd2025215a894d1ee7f",
                            "9e033c9bf3664f59c573336d0d6dbf8c8a20bdf73d0ed2ce63b8cf835836ee8a",
                            "98fd662dfa09c67642a468d5f2d7da6a8a13a3aac74ef24a42461ec61a0f498d"
                          ]
                        },
                        "siacoinOutput": {
                          "value": "288594172736732570239334030000",
                          "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                        },
                        "maturityHeight": 0
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
                          "53752750b684cab9c8c1d091f53f4dea9d1b3ab72d12d97ff73088594aa9b62198a1ed5b2fb33328075bb10f5f4a8ff14488787fc7238a174d2bc62bc96f9d07"
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
                      "address": "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                    }
                  ],
                  "minerFee": "0"
                },
                "maturityHeight": 169,
                "timestamp": "2024-11-15T19:41:06Z",
                "relevant": [
                  "f7843ac265b037658b304468013da4fd0f304a1b73df0dc68c4273c867bfa38d01a7661a187f"
                ]
              }
        );
        test_serde!(Event, j);
    }

    fn test_v2_transaction_serde_basic_send() {
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
                            ],
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

        let j2 = serde_json::to_value(&tx).unwrap().to_string();
        let tx2 = serde_json::from_str::<V2Transaction>(&j2).unwrap();
        assert_eq!(tx, tx2);
    }

    fn test_v2_transaction_serde_with_arbitrary_data() {
        let j = r#"
        {
            "siacoinInputs": [
                {
                    "parent": {
                        "id": "3d91fd82c8b197f7c63a803a5b6b15feeeb55f03a861036832137a2cd43175c5",
                        "stateElement": {
                            "leafIndex": 445,
                            "merkleProof": [
                                "550179f319d4702c067891cc412ac3adfdbaab268285ad9e94ec8d523bda769a",
                                "7b0a8ab5a6fe6466ef509fc645583d128a6a20b5cff710dc7092647c9833240e",
                                "02f359ce3b8c053e336afbc53c1b5e11bee75b73c40d809802ab0c454b8bf556",
                                "d3ec699dff3207f0dc458b715b01e60a51561820ad3260a0f5857fb24d238137",
                                "16a007608da1f582c2901eb9b49e3a00456ce4f1c3290ad0125e59be83de19bd",
                                "94cdee3fd437d058d14fa43faedb8a2c6d260c8da968cee5f4293e0296f9ddfc",
                                "549e313b749f317300a7bb7b92545c9fb929ba5b9466f56c3151f0c180a9d7f6",
                                "cd0b57da0c99b37bba96270250c0c2e883655b41443d88e0e6780b420e8c5616",
                                "d709b55266c068cac2ca645c39a1ecce14fc30677d5855074a290e9e435b39a5"
                            ]
                        },
                        "siacoinOutput": {
                            "value": "300000000000000000000000000000",
                            "address": "8b7201e203c8b4a58e2f68d29e6cb8973706f1578a074f5b29b1ae1f4136da85ae7f7667a714"
                        },
                        "maturityHeight": 229
                    },
                    "satisfiedPolicy": {
                        "policy": {
                            "type": "pk",
                            "policy": "ed25519:32f8eb30eab7b9c8ec8be44f37ac0a2b0f7fa1f7c9540abb6e21267b1995024f"
                        },
                        "signatures": [
                            "4639d5738969c4870307d6bae35e16a1e38417a66e041aea639cdfb97ae250c5c583967e6367c5eb976c3e036792a6e0a2bcb053bfef86789a35b468bfc01b0c"
                        ]
                    }
                }
            ],
            "siacoinOutputs": [
                {
                    "value": "12870012870012870012",
                    "address": "0125788b383a1dd122cd511386dd2668c62e54610cc743307d7a8ba17161a175f4b03c40b656"
                },
                {
                    "value": "299999999977129987129987129988",
                    "address": "8b7201e203c8b4a58e2f68d29e6cb8973706f1578a074f5b29b1ae1f4136da85ae7f7667a714"
                }
            ],
            "arbitraryData": "hss62dS2RraC7PbMFY6ETQ==",
            "minerFee": "10000000000000000000"
        }"#;

        let tx = serde_json::from_str::<V2Transaction>(j).unwrap();
        let j2 = serde_json::to_value(&tx).unwrap().to_string();
        let tx2 = serde_json::from_str::<V2Transaction>(&j2).unwrap();
        assert_eq!(tx, tx2);
    }

    }
}

#[test]
fn test_event_serde() {
    use crate::types::Event;
    let event_str = r#"
         {
        "id": "ff337547d944911c1bd0789c5ffe7a003a03cc1d64270b8de62d924511ff9071",
        "index": {
            "height": 525118,
            "id": "0000000000000000bd5221b164eae97587575c2558d21676058da66ba8853953"
        },
        "confirmations": 4100,
        "type": "v1Transaction",
        "data": {
            "transaction": {
                "id": "ff337547d944911c1bd0789c5ffe7a003a03cc1d64270b8de62d924511ff9071",
                "siacoinOutputs": [
                    {
                        "id": "aebb5f0a3befd2acdcc440e6089c473a38436d1a093c039825be40dfb4add9de",
                        "value": "4822738094961677886697130952",
                        "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                    }
                ],
                "siacoinInputs": [
                    {
                        "parentID": "4015a54ecf52a3d1f6ea42545ea60e73393b5efea39dc1282b41ab3097458f78",
                        "unlockConditions": {
                            "timelock": 0,
                            "publicKeys": [
                                "ed25519:0b85023147f4b5b83ae8a0348abce8cc873b6f012a64f09f37d849022efd129e"
                            ],
                            "signaturesRequired": 1
                        },
                        "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                    }
                ],
                "minerFees": [
                    "10000000000000000000000"
                ],
                "arbitraryData": [
                    "SG9zdEFubm91bmNlbWVudBIAAAAAAAAANjYuMjMuMTkzLjI0NDo5OTgyZWQyNTUxOQAAAAAAAAAAACAAAAAAAAAANsiwfmFUilfhbfq9/MB9wVeXSnUBCrFoRkPZM+g/p7GGoKpZY399BknF8v5LA2AblJEploM/L+SxAK8/4kpgwDaNn3BdUBiCPHVDHMj48e6AOh5D9A/ly4fpyJ6LcCoG"
                ],
                "signatures": [
                    {
                        "parentID": "4015a54ecf52a3d1f6ea42545ea60e73393b5efea39dc1282b41ab3097458f78",
                        "publicKeyIndex": 0,
                        "coveredFields": {
                            "wholeTransaction": true
                        },
                        "signature": "7S8X5HlDXS4fj1tvyIcGvbiPiVpcKRPnc9J9l1oE2zpuofwczLjVNnshTzIJj696GZniK/TtQYMyHGIG0EdhCA=="
                    }
                ]
            },
            "spentSiacoinElements": [
                {
                    "id": "4015a54ecf52a3d1f6ea42545ea60e73393b5efea39dc1282b41ab3097458f78",
                    "stateElement": {
                        "leafIndex": 72827402
                    },
                    "siacoinOutput": {
                        "value": "4822748094961677886697130952",
                        "address": "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
                    },
                    "maturityHeight": 0
                }
            ]
        },
        "maturityHeight": 525118,
        "timestamp": "2025-05-31T07:14:25Z",
        "relevant": [
            "4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196"
        ]
    }"#;

    let _event: Event = serde_json::from_str(&event_str).expect("Failed to deserialize JSON into Vec<Event>");
}
