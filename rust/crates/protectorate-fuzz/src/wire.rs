use protectorate_wire::{
    decode_json, decode_stream_request, encode_envelope, encode_json, Catalog,
};
use std::sync::OnceLock;

const LIMIT: usize = 1 << 20;

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog::embedded().expect("embedded API descriptor"))
}

/// `<procedure>[ out][ stream]\n<payload>`: the payload is decoded as that
/// method's request (or response), as a raw unary body or inside one Connect
/// stream envelope.
pub fn wire_json(data: &[u8]) {
    let (head, payload) = match data.iter().position(|&b| b == b'\n') {
        Some(i) => (&data[..i], &data[i + 1..]),
        None => (data, &[][..]),
    };
    let head = String::from_utf8_lossy(head);
    let (head, stream) = match head.strip_suffix(" stream") {
        Some(head) => (head, true),
        None => (&*head, false),
    };
    let (procedure, output) = match head.strip_suffix(" out") {
        Some(procedure) => (procedure, true),
        None => (head, false),
    };
    let catalog = catalog();
    let method = catalog.method(procedure).unwrap_or_else(|| {
        let count = catalog.methods().count();
        catalog.methods().nth(procedure.len() % count).unwrap()
    });
    let descriptor = if output {
        method.output()
    } else {
        method.input()
    };
    let decoded = if stream {
        let framed = decode_stream_request(descriptor.clone(), payload, LIMIT);
        if payload.len() >= 5 && payload[0] == 0 && payload.len() - 5 <= LIMIT {
            let length = u32::from_be_bytes(payload[1..5].try_into().unwrap()) as usize;
            if length == payload.len() - 5 {
                let plain = decode_json(descriptor.clone(), &payload[5..]);
                assert_eq!(framed.is_ok(), plain.is_ok(), "envelope changed the decode");
            }
        }
        framed
    } else {
        let plain = decode_json(descriptor.clone(), payload);
        if payload.len() <= LIMIT {
            let envelope = encode_envelope(0, payload).unwrap();
            let framed = decode_stream_request(descriptor.clone(), &envelope, LIMIT);
            assert_eq!(framed.is_ok(), plain.is_ok(), "envelope changed the decode");
        }
        plain
    };
    let Ok(message) = decoded else {
        return;
    };
    let encoded = encode_json(&message).expect("a decoded message encodes");
    // Empty messages encode as {}, which the decoder accepts.
    let again = decode_json(descriptor, &encoded)
        .unwrap_or_else(|e| panic!("re-decode of {}: {e}", String::from_utf8_lossy(&encoded)));
    assert_eq!(
        encode_json(&again).unwrap(),
        encoded,
        "JSON round trip changed the message"
    );
}
