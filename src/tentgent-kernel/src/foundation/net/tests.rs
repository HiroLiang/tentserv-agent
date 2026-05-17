use super::{format_host_for_url_authority, http_url_from_host_port};

#[test]
fn formats_ipv4_dns_and_ipv6_hosts_for_url_authority() {
    assert_eq!(format_host_for_url_authority("127.0.0.1"), "127.0.0.1");
    assert_eq!(format_host_for_url_authority(" localhost "), "localhost");
    assert_eq!(format_host_for_url_authority("::1"), "[::1]");
    assert_eq!(format_host_for_url_authority("[::1]"), "[::1]");
}

#[test]
fn builds_http_url_from_host_and_port() {
    assert_eq!(
        http_url_from_host_port("127.0.0.1", 8790),
        "http://127.0.0.1:8790"
    );
    assert_eq!(http_url_from_host_port("::1", 8790), "http://[::1]:8790");
}
