use protectorate_chatlog::Event;
use serde_json::Value;

/// Watch serves a sleeve's log as ChatEvents; an event that does not decode
/// is lost to every client.
pub fn assert_decodable(events: &[Event]) {
    for event in events {
        let mut payload = event.payload.clone();
        if let Some(user) = payload.get_mut("user").and_then(Value::as_object_mut) {
            user.remove("requestId");
        }
        protectorate_wire::decode::<protectorate_wire::v1::ChatEvent>(
            &serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap_or_else(|error| panic!("event {}: {}", event.seq, error.message));
    }
}
