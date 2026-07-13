# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getchaintips.py

Functional tests for `getchaintips`. Verifies the RPC returns correct chain tip
information for both a single-tip chain and after a chain reorganization that
produces multiple tips.
"""

import pytest
from test_framework.constants import GENESIS_BLOCK_HASH

MINE_BLOCKS = 10


@pytest.mark.rpc
def test_get_chain_tips_single_tip(
    node_manager, florestad_bitcoind_utreexod_with_chain
):
    """
    After mining blocks on a single chain (no forks), getchaintips should
    return exactly one tip with status "active" and branchlen 0.
    """
    florestad, _bitcoind, _utreexod = florestad_bitcoind_utreexod_with_chain(
        MINE_BLOCKS
    )
    node_manager.wait_for_sync_nodes()

    tips = florestad.rpc.get_chain_tips()

    assert isinstance(tips, list)
    assert len(tips) == 1

    active_tip = tips[0]
    assert active_tip["status"] == "active"
    assert active_tip["branchlen"] == 0
    assert active_tip["height"] == florestad.rpc.get_block_count()
    assert active_tip["hash"] == florestad.rpc.get_bestblockhash()


@pytest.mark.rpc
def test_get_chain_tips_after_reorg(setup_logging, florestad_utreexod, node_manager):
    """
    Trigger a chain reorganization and verify that getchaintips reports
    multiple tips: the active tip and at least one fork tip with
    status "valid-headers" and branchlen > 0.
    """
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    # Mine initial chain
    log.info(f"Mining {MINE_BLOCKS} blocks on the initial chain")
    utreexod.rpc.generate(MINE_BLOCKS)
    node_manager.wait_for_sync_nodes(is_finished_ibd=False)

    old_best = florestad.rpc.get_bestblockhash()

    # Invalidate a block to create a fork point
    count_invalid = 5
    height_invalid = utreexod.rpc.get_block_count() - count_invalid
    log.info(f"Invalidating block at height {height_invalid}")
    utreexod.rpc.invalidate_block(utreexod.rpc.get_blockhash(height_invalid))

    # Mine a longer alternative chain to trigger reorg
    new_blocks = count_invalid + 5
    log.info(f"Mining {new_blocks} blocks on the alternative chain")
    utreexod.rpc.generate(new_blocks)
    node_manager.wait_for_sync_nodes(is_finished_ibd=False)

    # The best block should have changed
    assert florestad.rpc.get_bestblockhash() != old_best

    tips = florestad.rpc.get_chain_tips()
    assert isinstance(tips, list)
    assert len(tips) >= 2

    # Exactly one tip should be active
    active_tips = [t for t in tips if t["status"] == "active"]
    assert len(active_tips) == 1
    assert active_tips[0]["branchlen"] == 0
    assert active_tips[0]["hash"] == florestad.rpc.get_bestblockhash()
    assert active_tips[0]["hash"] == utreexod.rpc.get_bestblockhash()

    # At least one fork tip should exist with valid-headers status
    fork_tips = [t for t in tips if t["status"] == "valid-headers"]
    assert len(fork_tips) >= 1

    for fork_tip in fork_tips:
        assert fork_tip["branchlen"] > 0
        assert fork_tip["height"] > 0


@pytest.mark.rpc
def test_get_chain_tips_at_genesis(florestad_node):
    """
    At genesis (no blocks mined), getchaintips should return a single
    active tip at height 0 with the genesis block hash.
    """
    tips = florestad_node.rpc.get_chain_tips()

    assert isinstance(tips, list)
    assert len(tips) == 1

    tip = tips[0]
    assert tip["status"] == "active"
    assert tip["branchlen"] == 0
    assert tip["height"] == 0
    assert tip["hash"] == GENESIS_BLOCK_HASH
