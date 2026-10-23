pub mod browser;
pub mod cli;
pub mod client;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod doctor;
pub mod error;
pub mod export;
pub mod input;
pub mod logging;
pub mod mcp;
pub mod page_assets;
pub mod pipe;
pub mod pointer;
pub mod protocol;
pub mod registry;
pub mod security;
pub mod webmcp;

#[doc(hidden)]
pub mod browser_test_support {
    use crate::error::BridgeError;

    pub fn is_tab_gone_error(err: &BridgeError) -> bool {
        crate::browser::is_tab_gone_error(err)
    }

    pub fn is_transient_load_error(err: &BridgeError) -> bool {
        crate::browser::is_transient_load_error(err)
    }
}
