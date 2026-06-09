mod error {
    use thiserror::Error;

    #[allow(dead_code)]
    #[derive(Debug, Error)]
    pub enum BridgeError {
        #[error("discover pipes: {0}")]
        Discovery(String),

        #[error("{0}")]
        User(String),
    }

    pub type Result<T> = std::result::Result<T, BridgeError>;
}

#[allow(dead_code)]
#[path = "../src/discovery.rs"]
mod discovery;

use discovery::{extract_uuid, parse_pipe_list, pipe_path, PipeInfo};

#[test]
fn extract_uuid_matches_go_discovery_cases() {
    let cases = [
        (
            "codex-browser-use-abc12345-6789-4def-9abc-123456789abc",
            Some("abc12345-6789-4def-9abc-123456789abc"),
        ),
        (
            r"codex-browser-use\abc12345-6789-4def-9abc-123456789abc",
            Some("abc12345-6789-4def-9abc-123456789abc"),
        ),
        ("codex-browser-use", None),
        ("codex-browser-use-x", Some("x")),
    ];

    for (input, expected) in cases {
        assert_eq!(extract_uuid(input).as_deref(), expected);
    }
}

#[test]
fn parse_pipe_list_matches_go_discovery_and_ignores_unrelated_pipes() {
    let output = "InputPipe_1\r\n\
codex-browser-use\r\n\
codex-browser-use-abc12345-6789-4def-9abc-123456789abc\r\n\
   codex-browser-use-second-pipe   \r\n\
codex-browser-use\\third-pipe\r\n\
unrelated-pipe\r\n\
codex-browser-extra-foo\r\n";

    let got = parse_pipe_list(output);

    assert_eq!(
        got,
        vec![
            PipeInfo {
                name: "codex-browser-use-abc12345-6789-4def-9abc-123456789abc".into(),
                uuid: "abc12345-6789-4def-9abc-123456789abc".into(),
            },
            PipeInfo {
                name: "codex-browser-use-second-pipe".into(),
                uuid: "second-pipe".into(),
            },
            PipeInfo {
                name: r"codex-browser-use\third-pipe".into(),
                uuid: "third-pipe".into(),
            },
        ]
    );
}

#[test]
fn pipe_path_matches_go_pipe_path() {
    assert_eq!(
        pipe_path("codex-browser-use-foo"),
        r"\\.\pipe\codex-browser-use-foo"
    );
}

#[cfg(not(windows))]
#[tokio::test]
async fn discovery_reports_windows_only_on_non_windows() {
    let err = discovery::discover_codex_pipes().await.unwrap_err();

    assert!(err
        .to_string()
        .contains("only supports Windows named pipes"));
}
