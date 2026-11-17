// Integration test for SigmaOS-networkmanager-openvpn
// Verifies the crate builds and links as a dependency.
#![allow(unused_imports)]

use networkmanager_openvpn as _;

#[test]
fn crate_links_and_runs() {
    assert_eq!(2 + 2, 4);
}
