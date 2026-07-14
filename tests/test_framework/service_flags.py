# SPDX-License-Identifier: MIT OR Apache-2.0

"""Helpers for asserting service flag fields in RPC responses."""

# Mirrors floresta_common::service_flags_strings: maps each service name
# Floresta may emit in servicesnames to its corresponding bit.
SERVICE_FLAGS_BY_NAME = {
    "NETWORK": 1 << 0,
    "GETUTXO": 1 << 1,
    "BLOOM": 1 << 2,
    "WITNESS": 1 << 3,
    "COMPACT_FILTERS": 1 << 6,
    "NETWORK_LIMITED": 1 << 10,
    "P2P_V2": 1 << 11,
    "UTREEXO": 1 << 12,
    "UTREEXO_ARCHIVE": 1 << 13,
}


def assert_service_fields_consistent(peer_info):
    """
    Assert that getpeerinfo's services hex and servicesnames array describe the same flags.
    """
    services = peer_info["services"]
    assert isinstance(services, str)
    assert len(services) == 16
    services_bits = int(services, 16)

    services_names = peer_info["servicesnames"]
    assert isinstance(services_names, list)
    assert all(isinstance(name, str) for name in services_names)

    services_names = set(services_names)
    assert services_names <= set(SERVICE_FLAGS_BY_NAME)

    for name, flag in SERVICE_FLAGS_BY_NAME.items():
        assert bool(services_bits & flag) == (name in services_names), name
