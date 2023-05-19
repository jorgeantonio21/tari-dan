//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//

use std::convert::TryFrom;

use diesel::sql_types::{Integer, Text};
use serde::{Deserialize, Serialize};
use tari_crypto::tari_utilities::hex::from_hex;
use tari_template_lib::{prelude::ComponentAddress, Hash};

use crate::substate_storage_sqlite::schema::*;

#[derive(Debug, Identifiable, Queryable)]
#[diesel(table_name = events)]
pub struct Event {
    pub id: i32,
    pub component_address: String,
    pub tx_hash: String,
    pub topic: String,
    pub payload: String,
    pub version: i32,
}

#[derive(Debug, Clone, Insertable, AsChangeset)]
#[diesel(table_name = events)]
pub struct NewEvent {
    pub component_address: String,
    pub tx_hash: String,
    pub topic: String,
    pub payload: String,
    pub version: i32,
}

#[derive(Clone, Debug, QueryableByName, Deserialize, Serialize)]
pub struct EventData {
    #[diesel(sql_type = Text)]
    pub component_address: String,
    #[diesel(sql_type = Text)]
    pub tx_hash: String,
    #[diesel(sql_type = Text)]
    pub topic: String,
    #[diesel(sql_type = Text)]
    pub payload: String,
    #[diesel(sql_type = Integer)]
    pub version: i32,
}

impl TryFrom<EventData> for crate::graphql::model::events::Event {
    type Error = anyhow::Error;

    fn try_from(event_data: EventData) -> Result<Self, Self::Error> {
        let mut component_address = [0u8; 32];
        let component_address_buff =
            from_hex(event_data.component_address.as_ref()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        component_address.copy_from_slice(&component_address_buff);

        let mut tx_hash = [0u8; 32];
        let tx_hash_buffer = from_hex(event_data.tx_hash.as_ref()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        tx_hash.copy_from_slice(&tx_hash_buffer);

        let payload = serde_json::from_str(event_data.payload.as_str()).map_err(|e| anyhow::anyhow!(e.to_string()))?;

        Ok(Self {
            component_address,
            tx_hash,
            payload,
            topic: event_data.topic,
        })
    }
}

impl TryFrom<EventData> for tari_engine_types::events::Event {
    type Error = anyhow::Error;

    fn try_from(event_data: EventData) -> Result<Self, Self::Error> {
        let component_address = ComponentAddress::from_hex(event_data.component_address.as_str())
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let tx_hash = Hash::from_hex(event_data.tx_hash.as_str()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let payload = serde_json::from_str(event_data.payload.as_str()).map_err(|e| anyhow::anyhow!(e.to_string()))?;

        Ok(Self::new_with_payload(
            component_address,
            tx_hash,
            event_data.topic.clone(),
            payload,
        ))
    }
}
