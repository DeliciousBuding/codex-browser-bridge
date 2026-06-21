pub mod browser;
pub mod cli;
pub mod client;
pub mod config;
pub mod discovery;
pub mod doctor;
pub mod error;
pub mod logging;
pub mod mcp;
pub mod pipe;
pub mod protocol;
pub mod security;

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
