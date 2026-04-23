use std::net::Ipv4Addr;

/// Stable replacement for the unstable `Ipv4Addr::is_global()`.
///
/// Returns `true` if the address is globally routable per IANA registries.
/// Based on RFC 6890 / IANA IPv4 Special-Purpose Address Registry.
pub(crate) fn ipv4_is_global(ip: &Ipv4Addr) -> bool {
    !(ip.is_unspecified()
        || ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        // Shared address space (100.64.0.0/10) - RFC 6598
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xC0) == 64)
        // Protocol assignments (192.0.0.0/24) - RFC 6890
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
        // Benchmarking (198.18.0.0/15) - RFC 2544
        || (ip.octets()[0] == 198 && (ip.octets()[1] & 0xFE) == 18)
        // Reserved for future use (240.0.0.0/4) except broadcast
        || ip.octets()[0] >= 240)
}
