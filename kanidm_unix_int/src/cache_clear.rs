#![deny(warnings)]
#![warn(unused_extern_crates)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::unreachable)]
#![deny(clippy::await_holding_lock)]
#![deny(clippy::needless_pass_by_value)]
#![deny(clippy::trivially_copy_pass_by_ref)]

#[macro_use]
extern crate tracing;

use structopt::StructOpt;

use futures::executor::block_on;

use kanidm_unix_common::client::call_daemon;
use kanidm_unix_common::unix_config::KanidmUnixdConfig;
use kanidm_unix_common::unix_proto::{ClientRequest, ClientResponse};

include!("./opt/cache_clear.rs");

#[tokio::main]
async fn main() {
    let opt = CacheClearOpt::from_args();
    if opt.debug {
        ::std::env::set_var("RUST_LOG", "kanidm=debug,kanidm_client=debug");
    }
    tracing_subscriber::fmt::init();

    debug!("Starting cache invalidate tool ...");

    let cfg = match KanidmUnixdConfig::new().read_options_from_optional_config("/etc/kanidm/unixd")
    {
        Ok(c) => c,
        Err(_e) => {
            error!("Failed to parse /etc/kanidm/unixd");
            std::process::exit(1);
        }
    };

    if !opt.really {
        error!("Are you sure you want to proceed? If so use --really");
        return;
    }

    let req = ClientRequest::ClearCache;

    match block_on(call_daemon(cfg.sock_path.as_str(), req)) {
        Ok(r) => match r {
            ClientResponse::Ok => info!("success"),
            _ => {
                error!("Error: unexpected response -> {:?}", r);
            }
        },
        Err(e) => {
            error!("Error -> {:?}", e);
        }
    }
}
