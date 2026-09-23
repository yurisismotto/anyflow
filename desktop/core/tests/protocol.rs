//! Protocol-level tests: serialization, framing, versions, malformed input.

use omnibridge_core::framing::{self, MAX_FRAME_LEN};
use omnibridge_core::session::{negotiate_version, PROTOCOL_VERSION_MAX, PROTOCOL_VERSION_MIN};
use omnibridge_proto::v1;
use omnibridge_proto::Message;

fn sample_envelope() -> v1::Envelope {
    v1::Envelope {
        protocol_version: 1,
        message_id: vec![7u8; 16],
        sequence: 42,
        timestamp_unix_ms: 1_700_000_000_000,
        correlation_id: vec![9u8; 16],
        body: Some(v1::envelope::Body::Hello(v1::Hello {
            device: Some(v1::DeviceInfo {
                device_id: "a1b2c3d4e5f60718".into(),
                device_name: "Galaxy S25".into(),
                platform: v1::Platform::Android as i32,
                identity_fingerprint: "00".repeat(32),
            }),
            min_protocol_version: 1,
            max_protocol_version: 1,
            capabilities: vec!["battery.v1".into()],
        })),
    }
}

#[test]
fn envelope_round_trips() {
    let original = sample_envelope();
    let bytes = original.encode_to_vec();
    let decoded = v1::Envelope::decode(&bytes[..]).expect("decode");
    assert_eq!(original, decoded);
}

#[test]
fn capability_payload_is_opaque_to_the_transport() {
    // The transport must carry arbitrary bytes without interpreting them.
    let payload: Vec<u8> = (0u8..=255).collect();
    let env = v1::Envelope {
        protocol_version: 1,
        message_id: vec![1u8; 16],
        sequence: 1,
        timestamp_unix_ms: 0,
        correlation_id: Vec::new(),
        body: Some(v1::envelope::Body::CapabilityMessage(
            v1::CapabilityMessage {
                capability_id: "battery.v1".into(),
                payload: payload.clone(),
            },
        )),
    };
    let decoded = v1::Envelope::decode(&env.encode_to_vec()[..]).expect("decode");
    match decoded.body {
        Some(v1::envelope::Body::CapabilityMessage(m)) => {
            assert_eq!(m.payload, payload);
            assert_eq!(m.capability_id, "battery.v1");
        }
        other => panic!("wrong body: {other:?}"),
    }
}

#[test]
fn unknown_enum_values_do_not_panic() {
    // A future peer may send an enum value we have never heard of. It must
    // degrade to "unspecified", not crash and not be silently coerced into a
    // meaningful variant.
    assert!(v1::HelloStatus::try_from(9999).is_err());
    let status = v1::HelloStatus::try_from(9999).unwrap_or(v1::HelloStatus::Unspecified);
    assert_eq!(status, v1::HelloStatus::Unspecified);
}

#[test]
fn version_negotiation_picks_the_highest_common_version() {
    assert_eq!(negotiate_version(1, 1), Some(1));
    // Peer supports more than we do: settle on our maximum.
    assert_eq!(negotiate_version(1, 5), Some(PROTOCOL_VERSION_MAX));
}

#[test]
fn version_negotiation_rejects_disjoint_ranges() {
    // Peer is entirely newer than us.
    assert_eq!(
        negotiate_version(PROTOCOL_VERSION_MAX + 1, PROTOCOL_VERSION_MAX + 3),
        None
    );
    // Peer is entirely older than us.
    assert_eq!(
        negotiate_version(0, PROTOCOL_VERSION_MIN.saturating_sub(1)),
        None
    );
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn framing_round_trips() {
    let mut buf = Vec::new();
    let env = sample_envelope();
    framing::write_envelope(&mut buf, &env)
        .await
        .expect("write");

    let mut cursor = std::io::Cursor::new(buf);
    let decoded = framing::read_envelope(&mut cursor).await.expect("read");
    assert_eq!(env, decoded);
}

#[tokio::test]
async fn framing_reads_successive_frames() {
    let mut buf = Vec::new();
    for seq in 1..=3u64 {
        let mut env = sample_envelope();
        env.sequence = seq;
        framing::write_envelope(&mut buf, &env)
            .await
            .expect("write");
    }

    let mut cursor = std::io::Cursor::new(buf);
    for seq in 1..=3u64 {
        let env = framing::read_envelope(&mut cursor).await.expect("read");
        assert_eq!(env.sequence, seq);
    }
}

#[tokio::test]
async fn oversized_length_prefix_is_refused_without_allocating() {
    // A hostile peer claims a 4 GiB frame. We must reject on the header
    // alone, before touching the body.
    let mut buf = Vec::new();
    buf.extend_from_slice(&u32::MAX.to_be_bytes());

    let mut cursor = std::io::Cursor::new(buf);
    let err = framing::read_envelope(&mut cursor)
        .await
        .expect_err("an oversized length prefix must be refused");
    assert!(
        matches!(err, omnibridge_core::Error::FrameTooLarge(len, limit)
            if len == u32::MAX && limit == MAX_FRAME_LEN),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn zero_length_frame_is_refused() {
    let mut cursor = std::io::Cursor::new(0u32.to_be_bytes().to_vec());
    let err = framing::read_envelope(&mut cursor)
        .await
        .expect_err("a zero-length frame must be refused");
    assert!(
        matches!(err, omnibridge_core::Error::Protocol(_)),
        "{err:?}"
    );
}

#[tokio::test]
async fn malformed_protobuf_body_is_refused() {
    // Well-formed framing, garbage payload.
    let garbage = b"\xff\xff\xff\xff not protobuf at all";
    let mut buf = (garbage.len() as u32).to_be_bytes().to_vec();
    buf.extend_from_slice(garbage);

    let mut cursor = std::io::Cursor::new(buf);
    let err = framing::read_envelope(&mut cursor)
        .await
        .expect_err("a malformed protobuf body must be refused");
    assert!(matches!(err, omnibridge_core::Error::Decode(_)), "{err:?}");
}

#[tokio::test]
async fn truncated_frame_reports_closed_not_garbage() {
    // Header promises 100 bytes; only 10 arrive and the peer hangs up.
    let mut buf = 100u32.to_be_bytes().to_vec();
    buf.extend_from_slice(&[0u8; 10]);

    let mut cursor = std::io::Cursor::new(buf);
    let err = framing::read_envelope(&mut cursor)
        .await
        .expect_err("a truncated frame must be refused");
    assert!(matches!(err, omnibridge_core::Error::Closed), "{err:?}");
}

#[tokio::test]
async fn empty_stream_reports_closed() {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let err = framing::read_envelope(&mut cursor)
        .await
        .expect_err("an empty stream must report Closed");
    assert!(matches!(err, omnibridge_core::Error::Closed), "{err:?}");
}
