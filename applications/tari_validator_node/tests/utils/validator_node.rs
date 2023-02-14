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

use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use reqwest::Url;
use tari_common::configuration::{CommonConfig, StringList};
use tari_comms::multiaddr::Multiaddr;
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_p2p::{Network, PeerSeedsConfig, TransportType};
use tari_shutdown::Shutdown;
use tari_validator_node::{run_validator_node, ApplicationConfig, ValidatorNodeConfig};
use tari_validator_node_client::ValidatorNodeClient;
use tempfile::tempdir;
use tokio::task;

use crate::{
    utils::{helpers::get_os_assigned_ports, logging::get_base_dir},
    TariWorld,
};

#[derive(Debug)]
pub struct ValidatorNodeProcess {
    pub name: String,
    pub public_key: String,
    pub port: u16,
    pub json_rpc_port: u16,
    pub http_ui_port: u16,
    pub base_node_grpc_port: u16,
    pub handle: task::JoinHandle<()>,
    pub temp_dir_path: PathBuf,
    pub shutdown: Shutdown,
}

impl ValidatorNodeProcess {
    pub async fn create_client(&self) -> ValidatorNodeClient {
        get_vn_client(self.json_rpc_port).await
    }

    pub async fn save_database(&self, database_name: String, to: &Path) {
        dbg!(to);
        fs::create_dir_all(to).expect("Could not create directory");
        let from = &self.temp_dir_path.join(format!("{}.db", database_name));
        dbg!(&from);
        fs::copy(from, to.join(format!("{}.sqlite", database_name))).expect("Could not copy file");
    }
}

pub async fn spawn_validator_node(
    world: &mut TariWorld,
    validator_node_name: String,
    base_node_name: String,
    wallet_name: String,
) {
    // each spawned VN will use different ports
    let (port, json_rpc_port) = get_os_assigned_ports();
    let http_ui_port = 21000;
    // let (port, json_rpc_port, http_ui_port) = match world.validator_nodes.values().last() {
    //     Some(v) => (v.port + 1, v.json_rpc_port + 1, v.http_ui_port + 1),
    //     None => (20000, 20500, 21000), // default ports if it's the first vn to be spawned
    // };
    let base_node_grpc_port = world.base_nodes.get(&base_node_name).unwrap().grpc_port;
    let wallet_grpc_port = world.wallets.get(&wallet_name).unwrap().grpc_port;
    let name = validator_node_name.clone();

    // let temp_dir = tempdir().unwrap();
    // let temp_dir = tempdir.path().join(validator_node_name.clone());
    // let temp_dir_path = temp_dir.display().to_string();
    let peer_seeds: Vec<String> = world
        .validator_nodes
        .values()
        .map(|vn| format!("{}::/ip4/127.0.0.1/tcp/{}", vn.public_key, vn.port))
        .collect();

    let shutdown = Shutdown::new();
    let shutdown_signal = shutdown.to_signal();
    let temp_dir = get_base_dir().join("validator_nodes").join(validator_node_name.clone());
    let temp_dir_path = temp_dir.clone();
    let handle = task::spawn(async move {
        let mut config = ApplicationConfig {
            common: CommonConfig::default(),
            validator_node: ValidatorNodeConfig::default(),
            peer_seeds: PeerSeedsConfig::default(),
            network: Network::LocalNet,
        };

        // temporal folder for the VN files (e.g. sqlite file, json files, etc.)

        // let temp_dir = tempdir().unwrap().path().join(validator_node_name.clone());
        println!("Using validator_node temp_dir: {}", temp_dir.display());
        config.validator_node.data_dir = temp_dir.to_path_buf();
        config.validator_node.shard_key_file = temp_dir.join("shard_key.json");
        config.validator_node.identity_file = temp_dir.join("validator_node_id.json");
        config.validator_node.tor_identity_file = temp_dir.join("validator_node_tor_id.json");
        config.validator_node.base_node_grpc_address =
            Some(format!("127.0.0.1:{}", base_node_grpc_port).parse().unwrap());
        config.validator_node.wallet_grpc_address = Some(format!("127.0.0.1:{}", wallet_grpc_port).parse().unwrap());

        config.validator_node.p2p.transport.transport_type = TransportType::Tcp;
        config.validator_node.p2p.transport.tcp.listener_address =
            Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
        config.validator_node.p2p.public_address =
            Some(config.validator_node.p2p.transport.tcp.listener_address.clone());
        config.validator_node.public_address = Some(config.validator_node.p2p.transport.tcp.listener_address.clone());
        config.validator_node.p2p.datastore_path = temp_dir.to_path_buf().join("peer_db/vn");
        config.validator_node.p2p.dht = DhtConfig {
            // Not all platforms support sqlite memory connection urls
            database_url: DbConnectionUrl::File(temp_dir.join("dht.sqlite")),
            ..DhtConfig::default_local_test()
        };
        config.validator_node.json_rpc_address = Some(format!("127.0.0.1:{}", json_rpc_port).parse().unwrap());
        config.validator_node.http_ui_address = Some(format!("127.0.0.1:{}", http_ui_port).parse().unwrap());

        // The VNS will try to auto register upon startup
        config.validator_node.auto_register = false;

        // Add all other VNs as peer seeds
        config.peer_seeds.peer_seeds = StringList::from(peer_seeds);

        let result = run_validator_node(&config, shutdown_signal).await;
        if let Err(e) = result {
            panic!("{:?}", e);
        }
    });

    // We need to give it time for the VN to startup
    // TODO: it would be better to scan the VN to detect when it has started
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check if the inner thread panicked
    if handle.is_finished() {
        handle.await.unwrap();
        return;
    }

    // get the public key of the VN
    let public_key = get_vn_identity(json_rpc_port).await;

    // make the new vn able to be referenced by other processes
    let validator_node_process = ValidatorNodeProcess {
        name: name.clone(),
        public_key,
        port,
        base_node_grpc_port,
        http_ui_port,
        handle,
        json_rpc_port,
        temp_dir_path,
        shutdown,
    };
    world.validator_nodes.insert(name, validator_node_process);
}

pub async fn get_vn_client(port: u16) -> ValidatorNodeClient {
    let endpoint: Url = Url::parse(&format!("http://localhost:{}", port)).unwrap();
    ValidatorNodeClient::connect(endpoint).unwrap()
}

async fn get_vn_identity(jrpc_port: u16) -> String {
    // send the JSON RPC "get_identity" request to the VN
    let mut client = get_vn_client(jrpc_port).await;
    let resp = client.get_identity().await.unwrap();

    assert!(!resp.public_key.is_empty());
    resp.public_key
}
