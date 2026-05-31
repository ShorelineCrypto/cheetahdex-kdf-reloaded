//! # Purpose
//! `prost`-generated protobuf message definitions for the Iris
//! `irismod.htlc` module (`MsgCreateHTLC`, `MsgClaimHTLC`, `Htlc` state).
//!
//! # External binding
//! Mechanical 1:1 reflection of the upstream `.proto` schema. Field
//! tag numbers, names, and types are wire invariants. Do not edit by
//! hand for stylistic reasons; if regeneration ever becomes possible
//! upstream, prefer that over manual edits.
//! Source of truth: <https://github.com/irisnet/irismod/tree/master/proto/irismod/htlc>.

use crate::tendermint::htlc::HtlcState;

#[derive(prost::Message)]
pub(crate) struct IrisCreateHtlcProto {
    #[prost(string, tag = "1")]
    pub(crate) sender: prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub(crate) to: prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub(crate) receiver_on_other_chain: prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub(crate) sender_on_other_chain: prost::alloc::string::String,
    #[prost(message, repeated, tag = "5")]
    pub(crate) amount: prost::alloc::vec::Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>,
    #[prost(string, tag = "6")]
    pub(crate) hash_lock: prost::alloc::string::String,
    #[prost(uint64, tag = "7")]
    pub(crate) timestamp: u64,
    #[prost(uint64, tag = "8")]
    pub(crate) time_lock: u64,
    #[prost(bool, tag = "9")]
    pub(crate) transfer: bool,
}

#[derive(prost::Message)]
pub(crate) struct IrisClaimHtlcProto {
    #[prost(string, tag = "1")]
    pub(crate) sender: prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub(crate) id: prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub(crate) secret: prost::alloc::string::String,
}

#[derive(prost::Message)]
pub struct IrisHtlcProto {
    #[prost(string, tag = "1")]
    pub(crate) id: prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub(crate) sender: prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub(crate) to: prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub(crate) receiver_on_other_chain: prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub(crate) sender_on_other_chain: prost::alloc::string::String,
    #[prost(message, repeated, tag = "6")]
    pub(crate) amount: prost::alloc::vec::Vec<cosmrs::proto::cosmos::base::v1beta1::Coin>,
    #[prost(string, tag = "7")]
    pub(crate) hash_lock: prost::alloc::string::String,
    #[prost(string, tag = "8")]
    pub(crate) secret: prost::alloc::string::String,
    #[prost(uint64, tag = "9")]
    pub(crate) timestamp: u64,
    #[prost(uint64, tag = "10")]
    pub(crate) expiration_height: u64,
    #[prost(enumeration = "HtlcState", tag = "11")]
    pub(crate) state: i32,
    #[prost(uint64, tag = "12")]
    pub(crate) closed_block: u64,
    #[prost(bool, tag = "13")]
    pub(crate) transfer: bool,
}

#[derive(prost::Message)]
pub(crate) struct IrisQueryHtlcResponseProto {
    #[prost(message, tag = "1")]
    pub(crate) htlc: Option<IrisHtlcProto>,
}
