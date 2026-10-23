use std::net::{IpAddr, SocketAddr};

const COOKIE: [u8; 4] = [0x21, 0x12, 0xa4, 0x42];

/// Respond only to Tailscale's binding requests with a valid fingerprint.
pub fn stun_response(request: &[u8], source: SocketAddr) -> Option<Vec<u8>> {
    if request.len() < 20
        || request[..2] != [0, 1]
        || request[4..8] != COOKIE
        || usize::from(u16::from_be_bytes([request[2], request[3]])) != request.len() - 20
    {
        return None;
    }
    let mut offset = 20;
    let mut software = false;
    let mut fingerprint = false;
    while offset + 4 <= request.len() {
        let kind = u16::from_be_bytes(request[offset..offset + 2].try_into().ok()?);
        let len = usize::from(u16::from_be_bytes(
            request[offset + 2..offset + 4].try_into().ok()?,
        ));
        let padded = (len + 3) & !3;
        if offset + 4 + padded > request.len() {
            return None;
        }
        if kind == 0x8022 && &request[offset + 4..offset + 4 + len] == b"tailnode" {
            software = true;
        }
        if kind == 0x8028 {
            if len != 4 || offset + 8 != request.len() {
                return None;
            }
            let got = u32::from_be_bytes(request[offset + 4..offset + 8].try_into().ok()?);
            fingerprint = got == (crc32fast::hash(&request[..offset]) ^ 0x5354_554e);
        }
        offset += 4 + padded;
    }
    if offset != request.len() || !software || !fingerprint {
        return None;
    }
    let mut result = vec![0x01, 0x01, 0, 0];
    result.extend_from_slice(&COOKIE);
    result.extend_from_slice(&request[8..20]);
    result.extend_from_slice(&0x0020u16.to_be_bytes());
    match source.ip() {
        IpAddr::V4(ip) => {
            result.extend_from_slice(&8u16.to_be_bytes());
            result.extend_from_slice(&[0, 1]);
            result.extend_from_slice(&(source.port() ^ 0x2112).to_be_bytes());
            for (a, b) in ip.octets().iter().zip(COOKIE) {
                result.push(*a ^ b);
            }
        }
        IpAddr::V6(ip) => {
            result.extend_from_slice(&20u16.to_be_bytes());
            result.extend_from_slice(&[0, 2]);
            result.extend_from_slice(&(source.port() ^ 0x2112).to_be_bytes());
            for (i, a) in ip.octets().iter().enumerate() {
                result.push(*a ^ if i < 4 { COOKIE[i] } else { request[8 + i - 4] });
            }
        }
    }
    let len = (result.len() - 20) as u16;
    result[2..4].copy_from_slice(&len.to_be_bytes());
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_binding_request_returns_xor_mapping() {
        let mut request = vec![0, 1, 0, 20];
        request.extend_from_slice(&COOKIE);
        request.extend_from_slice(&[7; 12]);
        request.extend_from_slice(&0x8022u16.to_be_bytes());
        request.extend_from_slice(&8u16.to_be_bytes());
        request.extend_from_slice(b"tailnode");
        let fp = crc32fast::hash(&request) ^ 0x5354_554e;
        request.extend_from_slice(&0x8028u16.to_be_bytes());
        request.extend_from_slice(&4u16.to_be_bytes());
        request.extend_from_slice(&fp.to_be_bytes());
        let source: SocketAddr = "192.0.2.4:4242".parse().unwrap();
        let response = stun_response(&request, source).unwrap();
        assert_eq!(&response[..2], &[1, 1]);
        assert_eq!(&response[20..22], &0x0020u16.to_be_bytes());
        assert_eq!(&response[26..28], &(4242u16 ^ 0x2112).to_be_bytes());
        assert!(stun_response(&request[..request.len() - 1], source).is_none());
    }
}
