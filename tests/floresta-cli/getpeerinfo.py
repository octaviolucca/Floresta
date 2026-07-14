# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getpeerinfo.py

This functional test cli utility to interact with a Floresta node with `getpeerinfo`
"""

import pytest
from test_framework.util import assert_bitcoind_service_fields


@pytest.mark.rpc
def test_peer_info(florestad_node, bitcoind_node, node_manager):
    """
    Test `getpeerinfo` with a fresh node and its initial state.
    """

    result = florestad_node.rpc.get_peerinfo()

    assert isinstance(result, list)
    assert len(result) == 0

    node_manager.connect_nodes(florestad_node, bitcoind_node)

    result = florestad_node.rpc.get_peerinfo()
    assert isinstance(result, list)
    assert len(result) == 1

    peer_info = result[0]
    assert_bitcoind_service_fields(peer_info)
