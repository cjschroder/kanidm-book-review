//! These contain the server "cores". These are able to startup the server
//! (bootstrap) to a running state and then execute tasks. This is where modules
//! are logically ordered based on their depenedncies for execution. Some of these
//! are task-only i.e. reindexing, and some of these launch the server into a
//! fully operational state (https, ldap, etc).
//!
//! Generally, this is the "entry point" where the server begins to run, and
//! the entry point for all client traffic which is then directed to the
//! various `actors`.

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
#[macro_use]
extern crate kanidm;

mod https;
mod ldaps;
use libc::umask;

// use crossbeam::channel::unbounded;
use kanidm::prelude::*;
use std::sync::Arc;

use kanidm::config::Configuration;

// SearchResult
// use self::ctx::ServerCtx;
use kanidm::actors::v1_read::QueryServerReadV1;
use kanidm::actors::v1_write::QueryServerWriteV1;
use kanidm::be::{Backend, BackendConfig, BackendTransaction, FsType};
use kanidm::crypto::setup_tls;
use kanidm::idm::server::{IdmServer, IdmServerDelayed};
use kanidm::interval::IntervalActor;
use kanidm::ldap::LdapServer;
use kanidm::schema::Schema;
use kanidm::status::StatusActor;
use kanidm::utils::{duration_from_epoch_now, touch_file_or_quit};

use kanidm_proto::v1::OperationError;

use async_std::task;
use compact_jwt::JwsSigner;

// === internal setup helpers

fn setup_backend(config: &Configuration, schema: &Schema) -> Result<Backend, OperationError> {
    setup_backend_vacuum(config, schema, false)
}

fn setup_backend_vacuum(
    config: &Configuration,
    schema: &Schema,
    vacuum: bool,
) -> Result<Backend, OperationError> {
    // Limit the scope of the schema txn.
    // let schema_txn = task::block_on(schema.write());
    let schema_txn = schema.write();
    let idxmeta = schema_txn.reload_idxmeta();

    let pool_size: u32 = config.threads as u32;
    let fstype: FsType = if config
        .db_fs_type
        .as_ref()
        .map(|s| s == "zfs")
        .unwrap_or(false)
    {
        FsType::Zfs
    } else {
        FsType::Generic
    };

    let cfg = BackendConfig::new(
        config.db_path.as_str(),
        pool_size,
        fstype,
        config.db_arc_size,
    );

    Backend::new(cfg, idxmeta, vacuum)
}

// TODO #54: We could move most of the be/schema/qs setup and startup
// outside of this call, then pass in "what we need" in a cloneable
// form, this way we could have seperate Idm vs Qs threads, and dedicated
// threads for write vs read
fn setup_qs_idms(
    be: Backend,
    schema: Schema,
    config: &Configuration,
) -> Result<(QueryServer, IdmServer, IdmServerDelayed), OperationError> {
    // Create a query_server implementation
    let query_server = QueryServer::new(be, schema, config.domain.clone());

    // TODO #62: Should the IDM parts be broken out to the IdmServer?
    // What's important about this initial setup here is that it also triggers
    // the schema and acp reload, so they are now configured correctly!
    // Initialise the schema core.
    //
    // Now search for the schema itself, and validate that the system
    // in memory matches the BE on disk, and that it's syntactically correct.
    // Write it out if changes are needed.
    query_server.initialise_helper(duration_from_epoch_now())?;

    // We generate a SINGLE idms only!

    let (idms, idms_delayed) = IdmServer::new(query_server.clone(), &config.origin)?;

    Ok((query_server, idms, idms_delayed))
}

fn setup_qs(
    be: Backend,
    schema: Schema,
    config: &Configuration,
) -> Result<QueryServer, OperationError> {
    // Create a query_server implementation
    let query_server = QueryServer::new(be, schema, config.domain.clone());

    // TODO #62: Should the IDM parts be broken out to the IdmServer?
    // What's important about this initial setup here is that it also triggers
    // the schema and acp reload, so they are now configured correctly!
    // Initialise the schema core.
    //
    // Now search for the schema itself, and validate that the system
    // in memory matches the BE on disk, and that it's syntactically correct.
    // Write it out if changes are needed.
    query_server.initialise_helper(duration_from_epoch_now())?;

    Ok(query_server)
}

macro_rules! dbscan_setup_be {
    (
        $config:expr
    ) => {{
        let schema = match Schema::new() {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to setup in memory schema: {:?}", e);
                std::process::exit(1);
            }
        };

        match setup_backend($config, &schema) {
            Ok(be) => be,
            Err(e) => {
                error!("Failed to setup BE: {:?}", e);
                return;
            }
        }
    }};
}

pub fn dbscan_list_indexes_core(config: &Configuration) {
    let be = dbscan_setup_be!(config);
    let be_rotxn = be.read();

    match be_rotxn.list_indexes() {
        Ok(mut idx_list) => {
            idx_list.sort_unstable();
            idx_list.iter().for_each(|idx_name| {
                println!("{}", idx_name);
            })
        }
        Err(e) => {
            error!("Failed to retrieve index list: {:?}", e);
        }
    };
}

pub fn dbscan_list_id2entry_core(config: &Configuration) {
    let be = dbscan_setup_be!(config);
    let be_rotxn = be.read();

    match be_rotxn.list_id2entry() {
        Ok(mut id_list) => {
            id_list.sort_unstable_by_key(|k| k.0);
            id_list.iter().for_each(|(id, value)| {
                println!("{:>8}: {}", id, value);
            })
        }
        Err(e) => {
            error!("Failed to retrieve id2entry list: {:?}", e);
        }
    };
}

pub fn dbscan_list_index_analysis_core(config: &Configuration) {
    let _be = dbscan_setup_be!(config);
    // TBD in after slopes merge.
}

pub fn dbscan_list_index_core(config: &Configuration, index_name: &str) {
    let be = dbscan_setup_be!(config);
    let be_rotxn = be.read();

    match be_rotxn.list_index_content(index_name) {
        Ok(mut idx_list) => {
            idx_list.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            idx_list.iter().for_each(|(key, value)| {
                println!("{:>50}: {:?}", key, value);
            })
        }
        Err(e) => {
            error!("Failed to retrieve index list: {:?}", e);
        }
    };
}

pub fn dbscan_get_id2entry_core(config: &Configuration, id: u64) {
    let be = dbscan_setup_be!(config);
    let be_rotxn = be.read();

    match be_rotxn.get_id2entry(id) {
        Ok((id, value)) => println!("{:>8}: {}", id, value),
        Err(e) => {
            error!("Failed to retrieve id2entry value: {:?}", e);
        }
    };
}

pub fn backup_server_core(config: &Configuration, dst_path: &str) {
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    let be = match setup_backend(config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE: {:?}", e);
            return;
        }
    };

    let be_ro_txn = be.read();
    let r = be_ro_txn.backup(dst_path);
    match r {
        Ok(_) => info!("Backup success!"),
        Err(e) => {
            error!("Backup failed: {:?}", e);
            std::process::exit(1);
        }
    };
    // Let the txn abort, even on success.
}

pub fn restore_server_core(config: &Configuration, dst_path: &str) {
    touch_file_or_quit(config.db_path.as_str());

    // First, we provide the in-memory schema so that core attrs are indexed correctly.
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    let be = match setup_backend(config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup backend: {:?}", e);
            return;
        }
    };

    let be_wr_txn = be.write();
    let r = be_wr_txn.restore(dst_path).and_then(|_| be_wr_txn.commit());

    if r.is_err() {
        error!("Failed to restore database: {:?}", r);
        std::process::exit(1);
    }
    info!("Database loaded successfully");

    info!("Attempting to init query server ...");

    let (qs, _idms, _idms_delayed) = match setup_qs_idms(be, schema, config) {
        Ok(t) => t,
        Err(e) => {
            error!("Unable to setup query server or idm server -> {:?}", e);
            return;
        }
    };
    info!("Success!");

    info!("Start reindex phase ...");

    let qs_write = task::block_on(qs.write_async(duration_from_epoch_now()));
    let r = qs_write.reindex().and_then(|_| qs_write.commit());

    match r {
        Ok(_) => info!("Reindex Success!"),
        Err(e) => {
            error!("Restore failed: {:?}", e);
            std::process::exit(1);
        }
    };

    info!("✅ Restore Success!");
}

pub fn reindex_server_core(config: &Configuration) {
    eprintln!("Start Index Phase 1 ...");
    // First, we provide the in-memory schema so that core attrs are indexed correctly.
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    let be = match setup_backend(config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE: {:?}", e);
            return;
        }
    };

    // Reindex only the core schema attributes to bootstrap the process.
    let be_wr_txn = be.write();
    let r = be_wr_txn.reindex().and_then(|_| be_wr_txn.commit());

    // Now that's done, setup a minimal qs and reindex from that.
    if r.is_err() {
        eprintln!("Failed to reindex database: {:?}", r);
        std::process::exit(1);
    }
    eprintln!("Index Phase 1 Success!");

    eprintln!("Attempting to init query server ...");

    let (qs, _idms, _idms_delayed) = match setup_qs_idms(be, schema, config) {
        Ok(t) => t,
        Err(e) => {
            error!("Unable to setup query server or idm server -> {:?}", e);
            return;
        }
    };
    eprintln!("Init Query Server Success!");

    eprintln!("Start Index Phase 2 ...");

    let qs_write = task::block_on(qs.write_async(duration_from_epoch_now()));
    let r = qs_write.reindex().and_then(|_| qs_write.commit());

    match r {
        Ok(_) => eprintln!("Index Phase 2 Success!"),
        Err(e) => {
            eprintln!("Reindex failed: {:?}", e);
            std::process::exit(1);
        }
    };
}

pub fn vacuum_server_core(config: &Configuration) {
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    // The schema doesn't matter here. Vacuum is run as part of db open to avoid
    // locking.
    let r = setup_backend_vacuum(config, &schema, true);

    match r {
        Ok(_) => eprintln!("Vacuum Success!"),
        Err(e) => {
            eprintln!("Vacuum failed: {:?}", e);
            std::process::exit(1);
        }
    };
}

pub fn domain_rename_core(config: &Configuration) {
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    // Start the backend.
    let be = match setup_backend(config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE: {:?}", e);
            return;
        }
    };

    // setup the qs - *with out* init of the migrations and schema.
    let qs = match setup_qs(be, schema, config) {
        Ok(t) => t,
        Err(e) => {
            error!("Unable to setup query server -> {:?}", e);
            return;
        }
    };

    let new_domain_name = config.domain.as_str();

    // make sure we're actually changing the domain name...
    match task::block_on(qs.read_async()).get_db_domain_name() {
        Ok(old_domain_name) => {
            admin_info!(?old_domain_name, ?new_domain_name);
            if &old_domain_name == &new_domain_name {
                admin_info!("Domain name not changing, stopping.");
                return;
            }
        }
        Err(e) => {
            admin_error!("Failed to query domain name, quitting! -> {:?}", e);
            return;
        }
    }

    let qs_write = task::block_on(qs.write_async(duration_from_epoch_now()));
    let r = qs_write.domain_rename().and_then(|_| qs_write.commit());

    match r {
        Ok(_) => info!("Domain Rename Success!"),
        Err(e) => {
            error!("Domain Rename Failed - Rollback has occured: {:?}", e);
            std::process::exit(1);
        }
    };
}

pub fn verify_server_core(config: &Configuration) {
    // setup the qs - without initialise!
    let schema_mem = match Schema::new() {
        Ok(sc) => sc,
        Err(e) => {
            error!("Failed to setup in memory schema: {:?}", e);
            return;
        }
    };
    // Setup the be
    let be = match setup_backend(config, &schema_mem) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE: {:?}", e);
            return;
        }
    };
    let server = QueryServer::new(be, schema_mem, config.domain.clone());

    // Run verifications.
    let r = server.verify();

    if r.is_empty() {
        eprintln!("Verification passed!");
        std::process::exit(0);
    } else {
        for er in r {
            error!("{:?}", er);
        }
        std::process::exit(1);
    }

    // Now add IDM server verifications?
}

pub fn recover_account_core(config: &Configuration, name: &str) {
    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to setup in memory schema: {:?}", e);
            std::process::exit(1);
        }
    };

    // Start the backend.
    let be = match setup_backend(config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE: {:?}", e);
            return;
        }
    };
    // setup the qs - *with* init of the migrations and schema.
    let (_qs, idms, _idms_delayed) = match setup_qs_idms(be, schema, config) {
        Ok(t) => t,
        Err(e) => {
            error!("Unable to setup query server or idm server -> {:?}", e);
            return;
        }
    };

    // Run the password change.
    let mut idms_prox_write = task::block_on(idms.proxy_write_async(duration_from_epoch_now()));
    let new_pw = match idms_prox_write.recover_account(name, None) {
        Ok(new_pw) => match idms_prox_write.commit() {
            Ok(_) => new_pw,
            Err(e) => {
                error!("A critical error during commit occured {:?}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            error!("Error during password reset -> {:?}", e);
            // abort the txn
            std::mem::drop(idms_prox_write);
            std::process::exit(1);
        }
    };
    eprintln!("Success - password reset to -> {}", new_pw);
}

pub async fn create_server_core(config: Configuration, config_test: bool) -> Result<(), ()> {
    // Until this point, we probably want to write to the log macro fns.

    if config.integration_test_config.is_some() {
        warn!("RUNNING IN INTEGRATION TEST MODE.");
        warn!("IF YOU SEE THIS IN PRODUCTION YOU MUST CONTACT SUPPORT IMMEDIATELY.");
    }

    info!(
        "Starting kanidm with configuration: {} {}",
        if config_test { "TEST" } else { "" },
        config
    );
    // Setup umask, so that every we touch or create is secure.
    let _ = unsafe { umask(0o0027) };

    // Similar, create a stats task which aggregates statistics from the
    // server as they come in.
    let status_ref = StatusActor::start(config.log_level);

    // Setup TLS (if any)
    let _opt_tls_params = match setup_tls(&config) {
        Ok(opt_tls_params) => opt_tls_params,
        Err(e) => {
            error!("Failed to configure TLS parameters -> {:?}", e);
            return Err(());
        }
    };

    let schema = match Schema::new() {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to setup in memory schema: {:?}", e);
            return Err(());
        }
    };

    // Setup the be for the qs.
    let be = match setup_backend(&config, &schema) {
        Ok(be) => be,
        Err(e) => {
            error!("Failed to setup BE -> {:?}", e);
            return Err(());
        }
    };
    // Start the IDM server.
    let (_qs, idms, mut idms_delayed) = match setup_qs_idms(be, schema, &config) {
        Ok(t) => t,
        Err(e) => {
            error!("Unable to setup query server or idm server -> {:?}", e);
            return Err(());
        }
    };

    // Extract any configuration from the IDMS that we may need.
    // For now we just do this per run, but we need to extract this from the db later.
    let jws_signer = match JwsSigner::generate_hs256() {
        Ok(k) => k,
        Err(e) => {
            error!("Unable to setup jws signer -> {:?}", e);
            return Err(());
        }
    };

    // Any pre-start tasks here.
    match &config.integration_test_config {
        Some(itc) => {
            let mut idms_prox_write =
                task::block_on(idms.proxy_write_async(duration_from_epoch_now()));
            match idms_prox_write.recover_account("admin", Some(&itc.admin_password)) {
                Ok(_) => {}
                Err(e) => {
                    error!(
                        "Unable to configure INTERGATION TEST admin account -> {:?}",
                        e
                    );
                    return Err(());
                }
            };
            match idms_prox_write.commit() {
                Ok(_) => {}
                Err(e) => {
                    error!("Unable to commit INTERGATION TEST setup -> {:?}", e);
                    return Err(());
                }
            }
        }
        None => {}
    }

    let ldap = match LdapServer::new(&idms) {
        Ok(l) => l,
        Err(e) => {
            error!("Unable to start LdapServer -> {:?}", e);
            return Err(());
        }
    };

    // Arc the idms and ldap
    let idms_arc = Arc::new(idms);
    let ldap_arc = Arc::new(ldap);

    // Pass it to the actor for threading.
    // Start the read query server with the given be path: future config
    let server_read_ref =
        QueryServerReadV1::start_static(config.log_level, idms_arc.clone(), ldap_arc.clone());

    // Create the server async write entry point.
    let server_write_ref = QueryServerWriteV1::start_static(config.log_level, idms_arc.clone());

    tokio::spawn(async move {
        idms_delayed.process_all(server_write_ref).await;
    });

    // Setup timed events associated to the write thread
    IntervalActor::start(server_write_ref);
    // Setup timed events associated to the read thread
    match &config.online_backup {
        Some(cfg) => {
            IntervalActor::start_online_backup(server_read_ref, cfg)?;
        }
        None => {
            debug!("Online backup not requested, skipping");
        }
    };

    // If we have been requested to init LDAP, configure it now.
    match &config.ldapaddress {
        Some(la) => {
            let opt_ldap_tls_params = match setup_tls(&config) {
                Ok(t) => t,
                Err(e) => {
                    error!("Failed to configure LDAP TLS parameters -> {:?}", e);
                    return Err(());
                }
            };
            if !config_test {
                // ⚠️  only start the sockets and listeners in non-config-test modes.
                ldaps::create_ldap_server(la.as_str(), opt_ldap_tls_params, server_read_ref)
                    .await?;
            }
        }
        None => {
            debug!("LDAP not requested, skipping");
        }
    }

    // TODO: Remove these when we go to auth bearer!
    // Copy the max size
    let _secure_cookies = config.secure_cookies;
    // domain will come from the qs now!
    let cookie_key: [u8; 32] = config.cookie_key;

    if config_test {
        admin_info!("this config rocks! 🪨 ");
    } else {
        // ⚠️  only start the sockets and listeners in non-config-test modes.
        self::https::create_https_server(
            config.address,
            // opt_tls_params,
            config.tls_config.as_ref(),
            config.role,
            &cookie_key,
            jws_signer,
            status_ref,
            server_write_ref,
            server_read_ref,
        )?;

        admin_info!("ready to rock! 🪨 ");
    }

    Ok(())
}
