use crate::password_prompt;
use crate::{
    AccountCredential, AccountOpt, AccountPerson, AccountPosix, AccountRadius, AccountSsh,
    AccountValidity,
};
use qrcode::{render::unicode, QrCode};
// use std::io;
use kanidm_client::KanidmClient;
use time::OffsetDateTime;

use dialoguer::{theme::ColorfulTheme, Select};
use dialoguer::{Confirm, Input, Password};

// use webauthn_authenticator_rs::{u2fhid::U2FHid, WebauthnAuthenticator};

// use kanidm_client::ClientError;
use kanidm_client::ClientError::Http as ClientErrorHttp;
use kanidm_proto::v1::OperationError::PasswordQuality;
use kanidm_proto::v1::{CUIntentToken, CURegState, CUSessionToken, CUStatus};
use std::fmt;
use std::str::FromStr;

impl AccountOpt {
    pub fn debug(&self) -> bool {
        match self {
            AccountOpt::Credential(acopt) => acopt.debug(),
            AccountOpt::Radius(acopt) => match acopt {
                AccountRadius::Show(aro) => aro.copt.debug,
                AccountRadius::Generate(aro) => aro.copt.debug,
                AccountRadius::Delete(aro) => aro.copt.debug,
            },
            AccountOpt::Posix(apopt) => match apopt {
                AccountPosix::Show(apo) => apo.copt.debug,
                AccountPosix::Set(apo) => apo.copt.debug,
                AccountPosix::SetPassword(apo) => apo.copt.debug,
            },
            AccountOpt::Person(apopt) => match apopt {
                AccountPerson::Extend(apo) => apo.copt.debug,
                AccountPerson::Set(apo) => apo.copt.debug,
            },
            AccountOpt::Ssh(asopt) => match asopt {
                AccountSsh::List(ano) => ano.copt.debug,
                AccountSsh::Add(ano) => ano.copt.debug,
                AccountSsh::Delete(ano) => ano.copt.debug,
            },
            AccountOpt::List(copt) => copt.debug,
            AccountOpt::Get(aopt) => aopt.copt.debug,
            AccountOpt::Delete(aopt) => aopt.copt.debug,
            AccountOpt::Create(aopt) => aopt.copt.debug,
            AccountOpt::Validity(avopt) => match avopt {
                AccountValidity::Show(ano) => ano.copt.debug,
                AccountValidity::ExpireAt(ano) => ano.copt.debug,
                AccountValidity::BeginFrom(ano) => ano.copt.debug,
            },
        }
    }

    pub async fn exec(&self) {
        match self {
            // id/cred/primary/set
            AccountOpt::Credential(acopt) => acopt.exec().await,
            AccountOpt::Radius(aropt) => match aropt {
                AccountRadius::Show(aopt) => {
                    let client = aopt.copt.to_client().await;

                    let rcred = client
                        .idm_account_radius_credential_get(aopt.aopts.account_id.as_str())
                        .await;

                    match rcred {
                        Ok(Some(s)) => println!("Radius secret: {}", s),
                        Ok(None) => println!("NO Radius secret"),
                        Err(e) => {
                            error!("Error -> {:?}", e);
                        }
                    }
                }
                AccountRadius::Generate(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_radius_credential_regenerate(aopt.aopts.account_id.as_str())
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
                AccountRadius::Delete(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_radius_credential_delete(aopt.aopts.account_id.as_str())
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
            }, // end AccountOpt::Radius
            AccountOpt::Posix(apopt) => match apopt {
                AccountPosix::Show(aopt) => {
                    let client = aopt.copt.to_client().await;
                    match client
                        .idm_account_unix_token_get(aopt.aopts.account_id.as_str())
                        .await
                    {
                        Ok(token) => println!("{}", token),
                        Err(e) => {
                            error!("Error -> {:?}", e);
                        }
                    }
                }
                AccountPosix::Set(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_unix_extend(
                            aopt.aopts.account_id.as_str(),
                            aopt.gidnumber,
                            aopt.shell.as_deref(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
                AccountPosix::SetPassword(aopt) => {
                    let client = aopt.copt.to_client().await;
                    let password = match password_prompt("Enter new posix (sudo) password: ") {
                        Some(v) => v,
                        None => {
                            println!("Passwords do not match");
                            return;
                        }
                    };

                    if let Err(e) = client
                        .idm_account_unix_cred_put(
                            aopt.aopts.account_id.as_str(),
                            password.as_str(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
            }, // end AccountOpt::Posix
            AccountOpt::Person(apopt) => match apopt {
                AccountPerson::Extend(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_person_extend(
                            aopt.aopts.account_id.as_str(),
                            aopt.mail.as_deref(),
                            aopt.legalname.as_deref(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
                AccountPerson::Set(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_person_set(
                            aopt.aopts.account_id.as_str(),
                            aopt.mail.as_deref(),
                            aopt.legalname.as_deref(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
            }, // end AccountOpt::Person
            AccountOpt::Ssh(asopt) => match asopt {
                AccountSsh::List(aopt) => {
                    let client = aopt.copt.to_client().await;

                    match client
                        .idm_account_get_ssh_pubkeys(aopt.aopts.account_id.as_str())
                        .await
                    {
                        Ok(pkeys) => pkeys.iter().for_each(|pkey| println!("{}", pkey)),
                        Err(e) => {
                            error!("Error -> {:?}", e);
                        }
                    }
                }
                AccountSsh::Add(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_post_ssh_pubkey(
                            aopt.aopts.account_id.as_str(),
                            aopt.tag.as_str(),
                            aopt.pubkey.as_str(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
                AccountSsh::Delete(aopt) => {
                    let client = aopt.copt.to_client().await;
                    if let Err(e) = client
                        .idm_account_delete_ssh_pubkey(
                            aopt.aopts.account_id.as_str(),
                            aopt.tag.as_str(),
                        )
                        .await
                    {
                        error!("Error -> {:?}", e);
                    }
                }
            }, // end AccountOpt::Ssh
            AccountOpt::List(copt) => {
                let client = copt.to_client().await;
                match client.idm_account_list().await {
                    Ok(r) => r.iter().for_each(|ent| println!("{}", ent)),
                    Err(e) => error!("Error -> {:?}", e),
                }
            }
            AccountOpt::Get(aopt) => {
                let client = aopt.copt.to_client().await;
                match client.idm_account_get(aopt.aopts.account_id.as_str()).await {
                    Ok(Some(e)) => println!("{}", e),
                    Ok(None) => println!("No matching entries"),
                    Err(e) => error!("Error -> {:?}", e),
                }
            }
            AccountOpt::Delete(aopt) => {
                let client = aopt.copt.to_client().await;
                if let Err(e) = client
                    .idm_account_delete(aopt.aopts.account_id.as_str())
                    .await
                {
                    error!("Error -> {:?}", e)
                }
            }
            AccountOpt::Create(acopt) => {
                let client = acopt.copt.to_client().await;
                if let Err(e) = client
                    .idm_account_create(
                        acopt.aopts.account_id.as_str(),
                        acopt.display_name.as_str(),
                    )
                    .await
                {
                    error!("Error -> {:?}", e)
                }
            }
            AccountOpt::Validity(avopt) => match avopt {
                AccountValidity::Show(ano) => {
                    let client = ano.copt.to_client().await;

                    let ex = match client
                        .idm_account_get_attr(ano.aopts.account_id.as_str(), "account_expire")
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Error -> {:?}", e);
                            return;
                        }
                    };

                    let vf = match client
                        .idm_account_get_attr(ano.aopts.account_id.as_str(), "account_valid_from")
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Error -> {:?}", e);
                            return;
                        }
                    };

                    if let Some(t) = vf {
                        // Convert the time to local timezone.
                        let t = OffsetDateTime::parse(&t[0], time::Format::Rfc3339)
                            .map(|odt| {
                                odt.to_offset(
                                    time::UtcOffset::try_current_local_offset()
                                        .unwrap_or(time::UtcOffset::UTC),
                                )
                                .format(time::Format::Rfc3339)
                            })
                            .unwrap_or_else(|_| "invalid timestamp".to_string());

                        println!("valid after: {}", t);
                    } else {
                        println!("valid after: any time");
                    }

                    if let Some(t) = ex {
                        let t = OffsetDateTime::parse(&t[0], time::Format::Rfc3339)
                            .map(|odt| {
                                odt.to_offset(
                                    time::UtcOffset::try_current_local_offset()
                                        .unwrap_or(time::UtcOffset::UTC),
                                )
                                .format(time::Format::Rfc3339)
                            })
                            .unwrap_or_else(|_| "invalid timestamp".to_string());
                        println!("expire: {}", t);
                    } else {
                        println!("expire: never");
                    }
                }
                AccountValidity::ExpireAt(ano) => {
                    let client = ano.copt.to_client().await;
                    if matches!(ano.datetime.as_str(), "never" | "clear") {
                        // Unset the value
                        match client
                            .idm_account_purge_attr(ano.aopts.account_id.as_str(), "account_expire")
                            .await
                        {
                            Err(e) => error!("Error -> {:?}", e),
                            _ => println!("Success"),
                        }
                    } else {
                        if let Err(e) =
                            OffsetDateTime::parse(ano.datetime.as_str(), time::Format::Rfc3339)
                        {
                            error!("Error -> {:?}", e);
                            return;
                        }

                        match client
                            .idm_account_set_attr(
                                ano.aopts.account_id.as_str(),
                                "account_expire",
                                &[ano.datetime.as_str()],
                            )
                            .await
                        {
                            Err(e) => error!("Error -> {:?}", e),
                            _ => println!("Success"),
                        }
                    }
                }
                AccountValidity::BeginFrom(ano) => {
                    let client = ano.copt.to_client().await;
                    if matches!(ano.datetime.as_str(), "any" | "clear" | "whenever") {
                        // Unset the value
                        match client
                            .idm_account_purge_attr(
                                ano.aopts.account_id.as_str(),
                                "account_valid_from",
                            )
                            .await
                        {
                            Err(e) => error!("Error -> {:?}", e),
                            _ => println!("Success"),
                        }
                    } else {
                        // Attempt to parse and set
                        if let Err(e) =
                            OffsetDateTime::parse(ano.datetime.as_str(), time::Format::Rfc3339)
                        {
                            error!("Error -> {:?}", e);
                            return;
                        }

                        match client
                            .idm_account_set_attr(
                                ano.aopts.account_id.as_str(),
                                "account_valid_from",
                                &[ano.datetime.as_str()],
                            )
                            .await
                        {
                            Err(e) => error!("Error -> {:?}", e),
                            _ => println!("Success"),
                        }
                    }
                }
            }, // end AccountOpt::Validity
        }
    }
}

impl AccountCredential {
    pub fn debug(&self) -> bool {
        match self {
            AccountCredential::Update(aopt) => aopt.copt.debug,
            AccountCredential::Reset(aopt) => aopt.copt.debug,
            AccountCredential::CreateResetLink(aopt) => aopt.copt.debug,
        }
    }

    pub async fn exec(&self) {
        match self {
            AccountCredential::Update(aopt) => {
                let client = aopt.copt.to_client().await;
                match client
                    .idm_account_credential_update_begin(aopt.aopts.account_id.as_str())
                    .await
                {
                    Ok(cusession_token) => credential_update_exec(cusession_token, client).await,
                    Err(e) => {
                        error!("Error starting credential update -> {:?}", e);
                    }
                }
            }
            AccountCredential::Reset(aopt) => {
                let client = aopt.copt.to_unauth_client();
                let cuintent_token = CUIntentToken {
                    intent_token: aopt.token.clone(),
                };

                match client
                    .idm_account_credential_update_exchange(cuintent_token)
                    .await
                {
                    Ok(cusession_token) => credential_update_exec(cusession_token, client).await,
                    Err(e) => {
                        error!("Error starting credential reset -> {:?}", e);
                    }
                }
            }
            AccountCredential::CreateResetLink(aopt) => {
                let client = aopt.copt.to_client().await;
                match client
                    .idm_account_credential_update_intent(aopt.aopts.account_id.as_str())
                    .await
                {
                    Ok(cuintent_token) => {
                        println!("success!");
                        println!("Send the person the following command");
                        println!("");
                        println!(
                            "kanidm account credential reset link {}",
                            cuintent_token.intent_token
                        );
                    }
                    Err(e) => {
                        error!("Error starting credential reset -> {:?}", e);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum CUAction {
    Help,
    Status,
    Password,
    Totp,
    TotpRemove,
    BackupCodes,
    Remove,
    End,
    Commit,
}

impl fmt::Display for CUAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"
help (h, ?) - Display this help
status (ls, st) - Show the status of the credential
password (passwd, pass, pw) - Set a new password
totp - Generate a new totp, requires a password to be set
totp remove (totp rm, trm) - Remove the TOTP of this account
backup codes (bcg, bcode) - (Re)generate backup codes for this account
remove (rm) - Remove only the primary credential
end (quit, exit, x, q) - End, without saving any changes
commit (save) - Commit the changes to the credential
"#
        )
    }
}

impl FromStr for CUAction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match s.as_str() {
            "help" | "h" | "?" => Ok(CUAction::Help),
            "status" | "ls" | "st" => Ok(CUAction::Status),
            "password" | "passwd" | "pass" | "pw" => Ok(CUAction::Password),
            "totp" => Ok(CUAction::Totp),
            "totp remove" | "totp rm" | "trm" => Ok(CUAction::TotpRemove),
            "backup codes" | "bcode" | "bcg" => Ok(CUAction::BackupCodes),
            "remove" | "rm" => Ok(CUAction::Remove),
            "end" | "quit" | "exit" | "x" | "q" => Ok(CUAction::End),
            "commit" | "save" => Ok(CUAction::Commit),
            _ => Err(()),
        }
    }
}

async fn totp_enroll_prompt(session_token: &CUSessionToken, client: &KanidmClient) {
    // First, submit the server side gen.
    let totp_secret = match client
        .idm_account_credential_update_init_totp(session_token)
        .await
    {
        Ok(CUStatus {
            mfaregstate: CURegState::TotpCheck(totp_secret),
            ..
        }) => totp_secret,
        Ok(status) => {
            debug!(?status);
            eprintln!("An error occured -> InvalidState");
            return;
        }
        Err(e) => {
            eprintln!("An error occured -> {:?}", e);
            return;
        }
    };

    // gen the qr
    eprintln!("Scan the following QR code with your OTP app.");

    let code = match QrCode::new(totp_secret.to_uri().as_str()) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to generate QR code -> {:?}", e);
            return;
        }
    };
    let image = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();
    eprintln!("{}", image);

    eprintln!("Alternatively, you can manually enter the following OTP details:");
    println!("Account Name: {}", totp_secret.accountname);
    println!("Issuer: {}", totp_secret.issuer);
    println!("Algorithm: {}", totp_secret.algo);
    println!("Period/Step: {}", totp_secret.step);
    println!("Secret: {}", totp_secret.get_secret());

    // prompt for the totp.
    eprintln!("--------------------------------------------------------------");
    eprintln!("Enter a TOTP from your authenticator to complete registration:");

    // Up to three attempts
    let mut attempts = 3;
    while attempts > 0 {
        attempts -= 1;
        // prompt for it. OR cancel.
        let input: String = Input::new()
            .with_prompt("TOTP")
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.to_lowercase().starts_with('c') || input.trim().parse::<u32>().is_ok() {
                    Ok(())
                } else {
                    Err("Must be a number (123456) or cancel to end")
                }
            })
            .interact_text()
            .unwrap();

        // cancel, submit the reg cancel.
        let totp_chal = match input.trim().parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("Cancelling TOTP registration ...");
                if let Err(e) = client
                    .idm_account_credential_update_cancel_mfareg(session_token)
                    .await
                {
                    eprintln!("An error occured -> {:?}", e);
                } else {
                    println!("success");
                }
                return;
            }
        };
        trace!(%totp_chal);

        // Submit and see what we get.
        match client
            .idm_account_credential_update_check_totp(session_token, totp_chal)
            .await
        {
            Ok(CUStatus {
                mfaregstate: CURegState::None,
                ..
            }) => {
                println!("success");
                break;
            }
            Ok(CUStatus {
                mfaregstate: CURegState::TotpTryAgain,
                ..
            }) => {
                // Wrong code! Try again.
                eprintln!("Incorrect TOTP code entered. Please try again.");
                continue;
            }
            Ok(CUStatus {
                mfaregstate: CURegState::TotpInvalidSha1,
                ..
            }) => {
                // Sha 1 warning.
                eprintln!("⚠️  WARNING - It appears your authenticator app may be broken ⚠️  ");
                eprintln!(" The TOTP authenticator you are using is forcing the use of SHA1\n");
                eprintln!(
                    " SHA1 is a deprecated and potentially insecure cryptographic algorithm\n"
                );

                let items = vec!["Cancel", "I am sure"];
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .items(&items)
                    .default(0)
                    .interact()
                    .unwrap();

                match selection {
                    1 => {
                        if let Err(e) = client
                            .idm_account_credential_update_accept_sha1_totp(session_token)
                            .await
                        {
                            eprintln!("An error occured -> {:?}", e);
                        } else {
                            println!("success");
                        }
                    }
                    _ => {
                        println!("Cancelling TOTP registration ...");
                        if let Err(e) = client
                            .idm_account_credential_update_cancel_mfareg(session_token)
                            .await
                        {
                            eprintln!("An error occured -> {:?}", e);
                        } else {
                            println!("success");
                        }
                    }
                }
                return;
            }
            Ok(status) => {
                debug!(?status);
                eprintln!("An error occured -> InvalidState");
                return;
            }
            Err(e) => {
                eprintln!("An error occured -> {:?}", e);
                return;
            }
        }
    }
    // Done!
}

// For webauthn later

/*
    AccountCredential::RegisterWebauthn(acsopt) => {
        let client = acsopt.copt.to_client().await;

        let (session, chal) = match client
            .idm_account_primary_credential_register_webauthn(
                acsopt.aopts.account_id.as_str(),
                acsopt.tag.as_str(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Error Starting Registration -> {:?}", e);
                return;
            }
        };

        let mut wa = WebauthnAuthenticator::new(U2FHid::new());

        eprintln!("Your authenticator will now flash for you to interact with.");

        let rego = match wa.do_registration(client.get_origin(), chal) {
            Ok(rego) => rego,
            Err(e) => {
                error!("Error Signing -> {:?}", e);
                return;
            }
        };

        match client
            .idm_account_primary_credential_complete_webuthn_registration(
                acsopt.aopts.account_id.as_str(),
                rego,
                session,
            )
            .await
        {
            Ok(()) => {
                println!("Webauthn token registration success.");
            }
            Err(e) => {
                error!("Error Completing -> {:?}", e);
            }
        }
    }
*/

async fn credential_update_exec(session_token: CUSessionToken, client: KanidmClient) {
    trace!("started credential update exec");
    loop {
        // Display Prompt
        let input: String = Input::new()
            .with_prompt("\ncred update (? for help) # ")
            .validate_with(|input: &String| -> Result<(), &str> {
                if CUAction::from_str(input).is_ok() {
                    Ok(())
                } else {
                    Err("This is not a valid command. See help for valid options (?)")
                }
            })
            .interact_text()
            .unwrap();

        // Get action
        let action = match CUAction::from_str(&input) {
            Ok(a) => a,
            Err(_) => continue,
        };

        trace!(?action);

        match action {
            CUAction::Help => {
                print!("{}", action);
            }
            CUAction::Status => {
                match client
                    .idm_account_credential_update_status(&session_token)
                    .await
                {
                    Ok(status) => {
                        if let Some(cred_detail) = status.primary {
                            println!("Primary Credential:");
                            print!("{}", cred_detail);
                        } else {
                            println!("Primary Credential:");
                            println!("  not set");
                        }

                        // We may need to be able to display if there are dangling
                        // curegstates, but the cli ui statemachine can match the
                        // server so it may not be needed?

                        println!("Can Commit: {}", status.can_commit);
                    }
                    Err(e) => {
                        eprintln!("An error occured -> {:?}", e);
                    }
                }
            }
            CUAction::Password => {
                let password_a = Password::new()
                    .with_prompt("New password")
                    .interact()
                    .unwrap();
                let password_b = Password::new()
                    .with_prompt("Confirm password")
                    .interact()
                    .unwrap();

                if password_a != password_b {
                    eprintln!("Passwords do not match");
                } else {
                    if let Err(e) = client
                        .idm_account_credential_update_set_password(&session_token, &password_a)
                        .await
                    {
                        match e {
                            ClientErrorHttp(_, Some(PasswordQuality(feedback)), _) => {
                                for fb_item in feedback.iter() {
                                    eprintln!("{:?}", fb_item)
                                }
                            }
                            _ => eprintln!("An error occured -> {:?}", e),
                        }
                    } else {
                        println!("success");
                    }
                }
            }
            CUAction::Totp => totp_enroll_prompt(&session_token, &client).await,
            CUAction::TotpRemove => {
                if Confirm::new()
                    .with_prompt("Do you want to remove your totp?")
                    .interact()
                    .unwrap()
                {
                    if let Err(e) = client
                        .idm_account_credential_update_remove_totp(&session_token)
                        .await
                    {
                        eprintln!("An error occured -> {:?}", e);
                    } else {
                        println!("success");
                    }
                } else {
                    println!("Totp was NOT removed");
                }
            }
            CUAction::BackupCodes => {
                match client
                    .idm_account_credential_update_backup_codes_generate(&session_token)
                    .await
                {
                    Ok(CUStatus {
                        mfaregstate: CURegState::BackupCodes(codes),
                        ..
                    }) => {
                        println!("Please store these Backup codes in a safe place");
                        println!("They will only be displayed ONCE");
                        for code in codes {
                            println!("  {}", code)
                        }
                    }
                    Ok(status) => {
                        debug!(?status);
                        eprintln!("An error occured -> InvalidState");
                    }
                    Err(e) => {
                        eprintln!("An error occured -> {:?}", e);
                    }
                }
            }
            CUAction::Remove => {
                if Confirm::new()
                    .with_prompt("Do you want to remove your primary credential?")
                    .interact()
                    .unwrap()
                {
                    if let Err(e) = client
                        .idm_account_credential_update_primary_remove(&session_token)
                        .await
                    {
                        eprintln!("An error occured -> {:?}", e);
                    } else {
                        println!("success");
                    }
                } else {
                    println!("Primary credential was NOT removed");
                }
            }
            CUAction::End => {
                println!("Changes were NOT saved.");
                break;
            }
            CUAction::Commit => {
                if Confirm::new()
                    .with_prompt("Do you want to commit your changes?")
                    .interact()
                    .unwrap()
                {
                    if let Err(e) = client
                        .idm_account_credential_update_commit(&session_token)
                        .await
                    {
                        eprintln!("An error occured -> {:?}", e);
                    } else {
                        println!("success");
                    }
                    break;
                } else {
                    println!("Changes have NOT been saved.");
                }
            }
        }
    }
    trace!("ended credential update exec");
}
