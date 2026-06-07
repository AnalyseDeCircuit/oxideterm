use super::{impl_packet_for, impl_request_id, Packet, RequestId};
use crate::{de::data_deserialize, ser::data_serialize};

/// Implementation for `SSH_FXP_EXTENDED`
#[derive(Debug, Serialize, Deserialize)]
pub struct Extended {
    pub id: u32,
    pub request: String,
    #[serde(serialize_with = "data_serialize")]
    #[serde(deserialize_with = "data_deserialize")]
    pub data: Vec<u8>,
}

impl_request_id!(Extended);
impl_packet_for!(Extended);

/// Implementation for `SSH_FXP_EXTENDED_REPLY`
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtendedReply {
    pub id: u32,
    #[serde(serialize_with = "data_serialize")]
    #[serde(deserialize_with = "data_deserialize")]
    pub data: Vec<u8>,
}

impl_request_id!(ExtendedReply);
impl_packet_for!(ExtendedReply);
