//! Protocol compatibility. Every released protocol version keeps its samples
//! here forever; newer code must still decode them to the same values.
//! Regenerate samples for the current version with `LANPILOT_BLESS=1` (exactly `1`).

use lanpilot_core::proto::v1::*;
use prost::Message;
use std::ffi::OsStr;
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Samples are rewritten only when `LANPILOT_BLESS` is exactly `1`.
fn should_bless(var: Option<&OsStr>) -> bool {
    var == Some(OsStr::new("1"))
}

#[test]
fn bless_needs_exactly_one() {
    assert!(should_bless(Some(OsStr::new("1"))));
    for other in ["0", "", "true", "yes"] {
        assert!(!should_bless(Some(OsStr::new(other))), "{other:?}");
    }
    assert!(!should_bless(None));
}

fn check<M: Message + Default + PartialEq + std::fmt::Debug>(name: &str, expected: M) {
    let path = golden_dir().join(format!("{name}.bin"));
    if should_bless(std::env::var_os("LANPILOT_BLESS").as_deref()) {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, expected.encode_to_vec()).unwrap();
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("missing golden {name}: {e}; run with LANPILOT_BLESS=1"));
    let decoded = M::decode(bytes.as_slice()).unwrap();
    assert_eq!(
        decoded, expected,
        "golden sample {name} decodes differently"
    );
}

#[test]
fn v1_hello() {
    check(
        "v1_hello",
        StreamOpen {
            kind: Some(stream_open::Kind::Hello(Hello {
                proto_min: 1,
                proto_max: 1,
                app_version: "0.1.0".into(),
                capabilities: vec!["text".into()],
                device_name: "iPhone".into(),
                os: Os::Ios as i32,
            })),
        },
    );
}

#[test]
fn v1_pointer_datagram() {
    check(
        "v1_pointer_datagram",
        PointerDatagram {
            gesture: Some(GestureState {
                gesture_id: 7,
                seq: 42,
                total_dx: 12.5,
                total_dy: -3.25,
                total_scroll_x: 0.0,
                total_scroll_y: 1.5,
            }),
        },
    );
}

#[test]
fn v1_run_command() {
    check(
        "v1_run_command",
        ClientMessage {
            request_id: 9,
            body: Some(client_message::Body::RunCommand(RunCommand {
                command_id: "open-vscode".into(),
            })),
        },
    );
}

#[test]
fn v1_command_result() {
    check(
        "v1_command_result",
        ServerMessage {
            request_id: 9,
            body: Some(server_message::Body::CommandResult(CommandResult {
                ok: true,
                error: String::new(),
                exit_code: Some(0),
                output: "done".into(),
            })),
        },
    );
}

#[test]
fn v1_pairing_invite() {
    check(
        "v1_pairing_invite",
        PairingInvite {
            addrs: vec!["192.168.1.20".into()],
            port: 45810,
            server_public_key: vec![1u8; 32],
            token: vec![2u8; 16],
            server_name: "Desk".into(),
        },
    );
}

#[test]
fn unknown_fields_are_ignored() {
    // A newer peer adds field 99 to Hello. Old code must still decode it.
    let mut bytes = Hello {
        proto_min: 1,
        proto_max: 1,
        ..Default::default()
    }
    .encode_to_vec();
    // tag = (99 << 3) | 2 = 794 = varint 0x9a 0x06; length 3; "new"
    bytes.extend_from_slice(&[0x9a, 0x06, 0x03, b'n', b'e', b'w']);
    let decoded = Hello::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded.proto_max, 1);
}
