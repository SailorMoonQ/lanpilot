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

#[test]
fn v1_pair_request_qr() {
    check(
        "v1_pair_request_qr",
        StreamOpen {
            kind: Some(stream_open::Kind::Pair(PairRequest {
                device_name: "iPhone".into(),
                os: Os::Ios as i32,
                method: Some(pair_request::Method::Qr(QrPairing {
                    token: vec![2u8; 16],
                })),
            })),
        },
    );
}

#[test]
fn v1_pair_request_password() {
    check(
        "v1_pair_request_password",
        StreamOpen {
            kind: Some(stream_open::Kind::Pair(PairRequest {
                device_name: "Pixel".into(),
                os: Os::Android as i32,
                method: Some(pair_request::Method::Password(PasswordPairing {
                    spake_msg: vec![3u8; 33],
                })),
            })),
        },
    );
}

#[test]
fn v1_pair_challenge() {
    check(
        "v1_pair_challenge",
        PairServerMessage {
            body: Some(pair_server_message::Body::Challenge(PairChallenge {
                spake_msg: vec![4u8; 33],
                server_confirm: vec![5u8; 32],
            })),
        },
    );
}

#[test]
fn v1_pair_result_locked() {
    check(
        "v1_pair_result_locked",
        PairServerMessage {
            body: Some(pair_server_message::Body::Result(PairResult {
                accepted: false,
                reason: PairRejectReason::Locked as i32,
                server_name: String::new(),
                server_os: Os::Unspecified as i32,
                retry_after_secs: 120,
            })),
        },
    );
}

#[test]
fn v1_pointer_button() {
    check(
        "v1_pointer_button",
        ClientMessage {
            request_id: 3,
            body: Some(client_message::Body::PointerButton(PointerButton {
                button: MouseButton::Left as i32,
                down: true,
                gesture: Some(GestureState {
                    gesture_id: 7,
                    seq: 43,
                    total_dx: 14.0,
                    total_dy: -2.5,
                    total_scroll_x: 0.0,
                    total_scroll_y: 0.0,
                }),
            })),
        },
    );
}

#[test]
fn v1_key_chord() {
    check(
        "v1_key_chord",
        ClientMessage {
            request_id: 4,
            // Left Ctrl + C.
            body: Some(client_message::Body::KeyChord(KeyChord {
                usages: vec![0xE0, 0x06],
            })),
        },
    );
}

#[test]
fn v1_text() {
    check(
        "v1_text",
        ClientMessage {
            request_id: 5,
            body: Some(client_message::Body::Text(Text {
                text: "你好, world".into(),
            })),
        },
    );
}

#[test]
fn v1_media() {
    check(
        "v1_media",
        ClientMessage {
            request_id: 6,
            body: Some(client_message::Body::Media(Media {
                action: MediaAction::PlayPause as i32,
            })),
        },
    );
}

#[test]
fn v1_unpair() {
    check(
        "v1_unpair",
        ClientMessage {
            request_id: 7,
            body: Some(client_message::Body::Unpair(Unpair {})),
        },
    );
}

#[test]
fn v1_command_list() {
    check(
        "v1_command_list",
        ServerMessage {
            request_id: 0,
            body: Some(server_message::Body::CommandList(CommandList {
                commands: vec![
                    CommandInfo {
                        id: "open-vscode".into(),
                        name: "Open VS Code".into(),
                        icon: "code".into(),
                        confirm: false,
                    },
                    CommandInfo {
                        id: "shutdown".into(),
                        name: "Shut down".into(),
                        icon: "power".into(),
                        confirm: true,
                    },
                ],
            })),
        },
    );
}

#[test]
fn v1_error() {
    check(
        "v1_error",
        ServerMessage {
            request_id: 8,
            body: Some(server_message::Body::Error(Error {
                code: ErrorCode::NotFound as i32,
                message: "no such command".into(),
            })),
        },
    );
}
