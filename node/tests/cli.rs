// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{net::SocketAddr, num::NonZeroU64, path::Path, str::FromStr};

use assert_cmd::Command;
use directories::UserDirs;
use tempfile::TempDir;

use node::{NodeConfigFile, NodeTypeConfigFile, RunOptions, StorageBackendConfigFile};

const BIN_NAME: &str = env!("CARGO_BIN_EXE_node");
const CONFIG_NAME: &str = "config.toml";

// This test is only needed because the node name ix hardcoded here, so if the name is changed we
// get an error that is easy to understand.
#[test]
fn node_path_is_correct() {
    assert!(Path::new(BIN_NAME).is_file());
}

#[test]
fn no_args() {
    Command::new(BIN_NAME).assert().failure();
}

#[test]
fn create_default_config() {
    let data_dir = TempDir::new().unwrap();

    Command::new(BIN_NAME)
        .arg("--datadir")
        .arg(data_dir.path().to_str().unwrap())
        .arg("create-config")
        .assert()
        .success();
    let config_path = data_dir.path().join(CONFIG_NAME);
    assert!(config_path.is_file());

    let options = default_run_options();
    let config = NodeConfigFile::read(&config_path, &None, &options).unwrap();

    assert_eq!(config.datadir, data_dir.path());

    assert_eq!(
        config.chainstate.chainstate_config.max_db_commit_attempts,
        None
    );
    assert_eq!(config.chainstate.chainstate_config.max_orphan_blocks, None);

    assert!(config.p2p.bind_addresses.unwrap_or_default().is_empty());
    assert_eq!(config.p2p.ban_threshold, None);
    assert_eq!(config.p2p.outbound_connection_timeout, None);

    assert_eq!(
        config.rpc.http_bind_address,
        Some(SocketAddr::from_str("127.0.0.1:3030").unwrap())
    );
}

// Check that the config fields are overwritten by the run options.
#[test]
fn read_config_override_values() {
    let data_dir = TempDir::new().unwrap();

    Command::new(BIN_NAME)
        .arg("--datadir")
        .arg(data_dir.path().to_str().unwrap())
        .arg("create-config")
        .assert()
        .success();
    let config_path = data_dir.path().join(CONFIG_NAME);
    assert!(config_path.is_file());

    let max_db_commit_attempts = 1;
    let max_orphan_blocks = 2;
    let p2p_addr = "address";
    let p2p_add_node = "add_node";
    let p2p_ban_threshold = 3;
    let p2p_timeout = NonZeroU64::new(10000).unwrap();
    let p2p_ping_check_period = 30;
    let p2p_ping_timeout = 60;
    let http_rpc_addr = SocketAddr::from_str("127.0.0.1:5432").unwrap();
    let ws_rpc_addr = SocketAddr::from_str("127.0.0.1:5433").unwrap();
    let backend_type = StorageBackendConfigFile::InMemory;
    let node_type = NodeTypeConfigFile::FullNode;
    let max_tip_age = 1000;

    let options = RunOptions {
        storage_backend: Some(backend_type.clone()),
        node_type: Some(node_type),
        max_db_commit_attempts: Some(max_db_commit_attempts),
        max_orphan_blocks: Some(max_orphan_blocks),
        tx_index_enabled: Some(false),
        p2p_addr: Some(vec![p2p_addr.to_owned()]),
        p2p_add_node: Some(vec![p2p_add_node.to_owned()]),
        p2p_ban_threshold: Some(p2p_ban_threshold),
        p2p_outbound_connection_timeout: Some(p2p_timeout),
        p2p_ping_check_period: Some(p2p_ping_check_period),
        p2p_ping_timeout: Some(p2p_ping_timeout),
        max_tip_age: Some(max_tip_age),
        http_rpc_addr: Some(http_rpc_addr),
        http_rpc_enabled: Some(true),
        ws_rpc_addr: Some(ws_rpc_addr),
        ws_rpc_enabled: Some(false),
    };
    let datadir_opt = Some(data_dir.path().into());
    let config = NodeConfigFile::read(&config_path, &datadir_opt, &options).unwrap();

    assert_eq!(config.datadir, data_dir.path());

    assert_eq!(
        config.chainstate.chainstate_config.max_db_commit_attempts,
        Some(max_db_commit_attempts)
    );
    assert_eq!(
        config.chainstate.chainstate_config.max_orphan_blocks,
        Some(max_orphan_blocks)
    );
    assert_eq!(
        config.chainstate.chainstate_config.tx_index_enabled,
        Some(false)
    );
    assert_eq!(
        config.chainstate.chainstate_config.max_tip_age,
        Some(max_tip_age)
    );

    assert_eq!(config.p2p.bind_addresses, Some(vec!(p2p_addr.to_owned())));
    assert_eq!(config.p2p.added_nodes, Some(vec!(p2p_add_node.to_owned())));
    assert_eq!(config.p2p.ban_threshold, Some(p2p_ban_threshold));
    assert_eq!(config.p2p.outbound_connection_timeout, Some(p2p_timeout));
    assert_eq!(config.p2p.ping_check_period, Some(p2p_ping_check_period));
    assert_eq!(config.p2p.ping_timeout, Some(p2p_ping_timeout));
    assert_eq!(config.p2p.node_type, Some(node_type));

    assert_eq!(config.rpc.http_bind_address, Some(http_rpc_addr));
    assert!(config.rpc.http_enabled.unwrap());

    assert_eq!(config.rpc.ws_bind_address, Some(ws_rpc_addr));
    assert!(!config.rpc.ws_enabled.unwrap());

    assert_eq!(config.chainstate.storage_backend, backend_type);
}

// Check that the `--conf` option has the precedence over the default data directory value.
#[test]
fn custom_config_path() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(CONFIG_NAME);

    Command::new(BIN_NAME)
        .arg("--conf")
        .arg(config_path.to_str().unwrap())
        .arg("create-config")
        .assert()
        .success();
    let data_dir = UserDirs::new().unwrap().home_dir().join(".mintlayer");
    assert!(data_dir.is_dir());
    assert!(config_path.is_file());

    let options = default_run_options();
    let config = NodeConfigFile::read(&config_path, &None, &options).unwrap();

    assert_eq!(config.datadir, data_dir);
}

// Check that the `--conf` option has the precedence over the `--datadir` option.
#[test]
fn custom_config_path_and_data_dir() {
    let data_dir = TempDir::new().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(CONFIG_NAME);

    Command::new(BIN_NAME)
        .arg("--datadir")
        .arg(data_dir.path().to_str().unwrap())
        .arg("--conf")
        .arg(config_path.to_str().unwrap())
        .arg("create-config")
        .assert()
        .success();
    assert!(config_path.is_file());

    let options = default_run_options();
    let config = NodeConfigFile::read(&config_path, &None, &options).unwrap();

    assert_eq!(config.datadir, data_dir.path());
}

fn default_run_options() -> RunOptions {
    RunOptions {
        storage_backend: None,
        node_type: None,
        max_db_commit_attempts: None,
        max_orphan_blocks: None,
        tx_index_enabled: None,
        p2p_addr: None,
        p2p_add_node: None,
        p2p_ban_threshold: None,
        p2p_outbound_connection_timeout: None,
        p2p_ping_check_period: None,
        p2p_ping_timeout: None,
        max_tip_age: None,
        http_rpc_addr: None,
        http_rpc_enabled: None,
        ws_rpc_addr: None,
        ws_rpc_enabled: None,
    }
}
