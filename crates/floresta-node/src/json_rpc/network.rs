// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module holds all RPC server side methods for interacting with our node's network stack.

use std::collections::BTreeMap;
use core::net::IpAddr;
use core::net::SocketAddr;

use bitcoin::Network;
use corepc_types::v30::GetNetworkInfo;
use corepc_types::v30::GetNetworkInfoNetwork;
use corepc_types::v30::PeerInfo as CorePeerInfo;
use floresta_common::PROTOCOL_VERSION;
use floresta_common::advertised_services;
use floresta_common::service_flags_strings;
use floresta_wire::address_man::ReachableNetworks;
use floresta_wire::node_interface::PeerInfo;
use floresta_wire::TransportProtocol;
use serde_json::Value;
use serde_json::json;

use super::res::JsonRpcError;
use super::server::RpcChain;
use super::server::RpcImpl;

type Result<T> = std::result::Result<T, JsonRpcError>;

/// Encode a `CARGO_PKG_VERSION` string (`"<major>.<minor>.<patch>"`) as Bitcoin Core's
/// numeric `MMmmpp` version. Returns `0` for malformed input.
fn parse_mmmmpp(version: &str) -> usize {
    let mut parts = version.splitn(3, '.');

    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .map(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    major * 10_000 + minor * 100 + patch
}

impl<Blockchain: RpcChain> RpcImpl<Blockchain> {
    pub(crate) async fn ping(&self) -> Result<bool> {
        self.node
            .ping()
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))
    }

    pub(crate) async fn add_node(
        &self,
        node_address: String,
        command: String,
        v2transport: bool,
    ) -> Result<Value> {
        // Try to parse both IP address and port.
        let (addr, port) = if let Ok(socket_addr) = node_address.parse::<SocketAddr>() {
            (socket_addr.ip(), socket_addr.port())
        // Try to parse the IP address only, and append the default P2P port for the network.
        } else {
            let ip = node_address
                .parse::<IpAddr>()
                .map_err(|_| JsonRpcError::InvalidAddress)?;

            // TODO: use `NetworkExt` to append the correct port once
            // https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
            let default_port = match self.network {
                Network::Bitcoin => 8333,
                Network::Signet => 38333,
                Network::Testnet => 18333,
                Network::Testnet4 => 48333,
                Network::Regtest => 18444,
            };

            (ip, default_port)
        };

        let _ = match command.as_str() {
            "add" => self.node.add_peer(addr, port, v2transport).await,
            "remove" => self.node.remove_peer(addr, port).await,
            "onetry" => self.node.onetry_peer(addr, port, v2transport).await,
            _ => return Err(JsonRpcError::InvalidAddnodeCommand),
        };

        Ok(json!(null))
    }

    pub(crate) async fn disconnect_node(
        &self,
        node_address: String,
        node_id: Option<u32>,
    ) -> Result<Value> {
        let (peer_addr, peer_port) = match (node_address.is_empty(), node_id) {
            // Reference the peer by it's IP address and port.
            (false, None) => {
                // Try to parse `node_address` into a `SocketAddr`.
                // This will handle IPv4:port and IPv6:port.
                let socket_addr = node_address
                    .parse::<SocketAddr>()
                    .map_err(|_| JsonRpcError::InvalidAddress)?;

                (socket_addr.ip(), socket_addr.port())
            }
            // Reference the peer by it's ID.
            (true, Some(node_id)) => {
                let peer_info = self
                    .node
                    .get_peer_info()
                    .await
                    .map_err(|e| JsonRpcError::Node(e.to_string()))?;

                let peer = peer_info
                    .iter()
                    .find(|peer| peer.id == node_id)
                    .ok_or(JsonRpcError::PeerNotFound)?;

                (peer.address.ip(), peer.address.port())
            }
            // Both address and ID were provided, or neither was provided.
            _ => {
                return Err(JsonRpcError::InvalidDisconnectNodeCommand);
            }
        };

        let disconnected = self
            .node
            .disconnect_peer(peer_addr, peer_port)
            .await
            .map_err(|e| JsonRpcError::Node(e.to_string()))?;

        if !disconnected {
            return Err(JsonRpcError::PeerNotFound);
        }

        Ok(json!(null))
    }

    pub(crate) async fn get_peer_info(&self) -> Result<Vec<CorePeerInfo>> {
        let peers = self
            .node
            .get_peer_info()
            .await
            .map_err(|_| JsonRpcError::Node("Failed to get peer information".to_string()))?;

        Ok(peers.into_iter().map(floresta_peer_to_core).collect())
    }

    pub(crate) async fn get_connection_count(&self) -> Result<usize> {
        self.node
            .get_connection_count()
            .await
            .map_err(|_| JsonRpcError::Node("Failed to get connection count".to_string()))
    }

    pub(crate) async fn get_network_info(&self) -> Result<GetNetworkInfo> {
        // Floresta does not listen for inbound connections, so every peer is outbound.
        let connections_in = 0;
        let connections_out = self
            .node
            .get_connection_count()
            .await
            .map_err(|_| JsonRpcError::Node("Failed to get connection count".to_string()))?;

        let advertised_services = advertised_services();
        let local_services = format!("{:016x}", advertised_services.to_u64());
        let local_services_names = service_flags_strings(&advertised_services);

        let proxy_str = self.proxy.map(|addr| addr.to_string()).unwrap_or_default();
        let proxy_set = self.proxy.is_some();

        let networks = ReachableNetworks::ALL
            .into_iter()
            .map(|net| {
                let reachable = ReachableNetworks::SUPPORTED.contains(&net);

                GetNetworkInfoNetwork {
                    name: net.to_string(),
                    limited: !reachable,
                    reachable,
                    proxy: proxy_str.clone(),
                    proxy_randomize_credentials: proxy_set,
                }
            })
            .collect();

        let version = parse_mmmmpp(env!("CARGO_PKG_VERSION"));

        Ok(GetNetworkInfo {
            version,
            subversion: self.user_agent.clone(),
            protocol_version: PROTOCOL_VERSION as usize,
            local_services,
            local_services_names,
            local_relay: false,
            time_offset: 0,
            connections: connections_in + connections_out,
            connections_in,
            connections_out,
            network_active: true,
            networks,
            // Since Floresta has no mempool, relay_fee and incremental_fee are hardcoded to 0.
            relay_fee: 0.0,
            incremental_fee: 0.0,
            local_addresses: Vec::new(), // Floresta doesn't track local addresses since it does not accept inbound connections
            warnings: Vec::new(),
        })
    }
}

fn floresta_peer_to_core(peer: PeerInfo) -> CorePeerInfo {
    let network = if peer.address.ip().is_loopback() {
        "not_publicly_routable"
    } else {
        match peer.address {
            SocketAddr::V4(_) => "ipv4",
            SocketAddr::V6(_) => "ipv6",
        }
    }
    .to_string();

    let services_u64 = peer.services.to_u64();
    let services = format!("{:016x}", services_u64);
    let services_names = service_flags_strings(&peer.services);

    let connection_type = serde_json::to_value(&peer.kind).ok().and_then(|v| v.as_str().map(String::from));

    let transport_protocol_type = match peer.transport_protocol {
        TransportProtocol::V1 => "v1",
        TransportProtocol::V2 => "v2",
    }
    .to_string();

    CorePeerInfo {
        id: peer.id,
        address: peer.address.to_string(),
        address_bind: None,
        address_local: None,
        network,
        mapped_as: None,
        services,
        services_names,
        relay_transactions: false,
        last_send: 0,
        last_received: 0,
        last_transaction: 0,
        last_block: 0,
        bytes_sent: 0,
        bytes_received: 0,
        connection_time: 0,
        time_offset: 0,
        ping_time: None,
        minimum_ping: None,
        ping_wait: None,
        version: 0,
        subversion: peer.user_agent,
        inbound: false,
        bip152_hb_to: false,
        bip152_hb_from: false,
        add_node: None,
        starting_height: Some(peer.initial_height as i64),
        presynced_headers: None,
        ban_score: None,
        synced_headers: None,
        synced_blocks: None,
        inflight: None,
        addresses_relay_enabled: None,
        addresses_processed: None,
        addresses_rate_limited: None,
        permissions: Vec::new(),
        minimum_fee_filter: 0.0,
        whitelisted: None,
        bytes_sent_per_message: BTreeMap::new(),
        bytes_received_per_message: BTreeMap::new(),
        connection_type,
        transport_protocol_type,
        session_id: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_mmmmpp;

    #[test]
    fn parse_mmmmpp_encodes_semver_correctly() {
        assert_eq!(parse_mmmmpp("0.9.0-rc1"), 900);
        assert_eq!(parse_mmmmpp("23.1.5"), 230_105);
        assert_eq!(parse_mmmmpp("1.2"), 10_200);
        assert_eq!(parse_mmmmpp("1"), 10_000);
    }
}
