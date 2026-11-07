use super::*;

fn answer() -> PackAnswer {
    PackAnswer {
        pack_id: [9; 32],
        attachment: Attachment {
            key: [1; 32],
            plaintext_sha256: [2; 32],
            blob_sha256: [3; 32],
            release_secret: [0; 32],
            size: 6289920,
            preview: vec![],
            mime: "application/octet-stream".into(),
            width: 0,
            height: 0,
        },
    }
}

#[test]
fn controls_roundtrip_and_allow_bounded_unknown_extensions() {
    let request = PackRequest { pack_id: [7; 32] };
    let mut bytes = request.encode();
    assert!(PackRequest::decode(&bytes).unwrap() == request);
    bytes.extend([24, 1]);
    assert!(PackRequest::decode(&bytes).unwrap() == request);
    let answer = answer();
    let mut bytes = answer.encode().unwrap();
    assert!(PackAnswer::decode(&bytes).unwrap() == answer);
    bytes.extend([24, 1]);
    assert!(PackAnswer::decode(&bytes).unwrap() == answer);
}

#[test]
fn sealed_six_mib_boundary_and_owner_capability_are_checked_both_ways() {
    let mut answer = answer();
    assert!(answer.encode().is_ok()); // 6,289,920 plaintext + 96 GCM tags = 6 MiB.
    for invalid_size in [0, 6289921, u64::MAX] {
        answer.attachment.size = invalid_size;
        assert!(answer.encode().is_err());
        assert!(PackAnswer::decode(&answer.encode_unchecked()).is_err());
    }
    answer = self::answer();
    answer.attachment.release_secret[0] = 1;
    assert!(answer.encode().is_err());
    assert!(PackAnswer::decode(&answer.encode_unchecked()).is_err());
    answer = self::answer();
    answer.attachment.preview.push(1);
    assert!(answer.encode().is_err());
    assert!(PackAnswer::decode(&answer.encode_unchecked()).is_err());
}

#[test]
fn valid_unknown_payload_at_two_kib_is_accepted_but_one_more_byte_is_refused() {
    let request = PackRequest { pack_id: [7; 32] };
    let mut at_limit = request.encode();
    // Unknown bytes field 3: three-byte header and 2011 bytes after the 34-byte ID.
    at_limit.extend([26, 0xdb, 0x0f]);
    at_limit.extend(vec![0; 2011]);
    assert_eq!(at_limit.len(), 2048);
    assert!(PackRequest::decode(&at_limit).is_ok());
    let mut over = request.encode();
    over.extend([26, 0xdc, 0x0f]);
    over.extend(vec![0; 2012]);
    assert_eq!(over.len(), 2049);
    assert!(PackRequest::decode(&over).is_err());
}

#[test]
fn malformed_lengths_context_and_missing_descriptor_fail_before_admission() {
    for bytes in [vec![], vec![10, 32, 1], vec![0xff], vec![0; 2049]] {
        assert!(PackRequest::decode(&bytes).is_err());
        assert!(PackAnswer::decode(&bytes).is_err());
    }
    for length in [0, 31, 33] {
        assert!(PackRequest::decode(
            &RequestWire {
                pack_id: vec![1; length]
            }
            .encode_to_vec()
        )
        .is_err());
    }
    let bytes = answer().encode().unwrap();
    let mut wire = AnswerWire::decode(bytes.as_slice()).unwrap();
    wire.attachment.as_mut().unwrap().hlc = Some(1);
    assert!(PackAnswer::decode(&wire.encode_to_vec()).is_err());
    wire.attachment.as_mut().unwrap().hlc = Some(0);
    wire.attachment.as_mut().unwrap().message_id = vec![1; 16];
    assert!(PackAnswer::decode(&wire.encode_to_vec()).is_err());
    wire.attachment = None;
    assert!(PackAnswer::decode(&wire.encode_to_vec()).is_err());
    wire = AnswerWire::decode(bytes.as_slice()).unwrap();
    wire.pack_id.pop();
    assert!(PackAnswer::decode(&wire.encode_to_vec()).is_err());
    assert!(PackAnswer::decode(&vec![0; 2048]).is_err());
}

#[test]
fn outer_controls_roundtrip_but_cannot_be_forwarded_or_leak_owner_secret() {
    use crate::chat::Body;
    for body in [
        Body::PackRequest(PackRequest { pack_id: [1; 32] }),
        Body::PackAnswer(Box::new(answer())),
    ] {
        let bytes = body.encode();
        assert!(matches!(bytes[0], 15 | 16));
        assert!(Body::decode(&bytes).unwrap() == body);
        assert!(Body::decode(&Body::Forwarded(Box::new(body)).encode()).is_err());
    }
    let mut invalid = answer();
    invalid.attachment.release_secret[0] = 1;
    assert!(Body::decode(&Body::PackAnswer(Box::new(invalid)).encode()).is_err());
}
