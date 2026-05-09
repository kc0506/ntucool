use anyhow::{Context, Result};
use cool_api::auth;
use cool_api::credentials::Credentials;
use cool_api::session::Session;

pub async fn run(opts: &super::GlobalOpts) -> Result<()> {
    let verbose = opts.verbose;
    // Try saved credentials first
    match auth::login_with_saved_credentials_verbose(verbose).await {
        Ok(session) => {
            save_session(&session)?;
            eprintln!("Login successful (from saved credentials).");
            return Ok(());
        }
        Err(_) => {
            eprintln!("No saved credentials found. Please enter your NTU credentials.");
        }
    }

    // Interactive prompt
    let username: String = dialoguer::Input::new()
        .with_prompt("NTU Username")
        .interact_text()
        .context("failed to read username")?;

    let password = rpassword::prompt_password("Password: ").context("failed to read password")?;

    let session = if verbose {
        auth::saml_login_verbose(&username, &password).await?
    } else {
        auth::saml_login(&username, &password).await?
    };

    save_session(&session)?;
    eprintln!("Login successful! Session valid for ~24h.");
    eprintln!();

    // Save credentials so cool-mcp can recover from session expiry without
    // a human at the keyboard. Three options, defaulting to plaintext: the
    // file is 0600 anyway, so vs "cat ~/.secrets/ntucool" the security
    // profile is identical without the extra setup. Power users who want a
    // password manager (pass, op, secret-tool) pick option C.
    let choice = dialoguer::Select::new()
        .with_prompt("Save credentials for non-interactive re-login?")
        .items(&[
            "Yes — store password in credentials.json (mode 0600)",
            "Yes — configure a password_cmd (pass / op / cat / ...)",
            "No — always prompt interactively",
        ])
        .default(0)
        .interact()
        .unwrap_or(2);

    match choice {
        0 => save_password_plaintext(&username, &password)?,
        1 => save_password_cmd(&username)?,
        _ => eprintln!("(no credentials saved — `cool login` will always prompt)"),
    }
    Ok(())
}

fn save_password_plaintext(username: &str, password: &str) -> Result<()> {
    let creds = Credentials {
        username: username.to_string(),
        password: Some(password.to_string()),
        password_cmd: None,
    };
    let creds_path = creds.save().context("failed to save credentials")?;
    eprintln!("Saved {} (mode 0600).", creds_path.display());
    Ok(())
}

fn save_password_cmd(username: &str) -> Result<()> {
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pass show ntucool/password");
    eprintln!("  op read 'op://Personal/NTU COOL/password'");
    eprintln!("  cat ~/.secrets/ntucool");
    let pw_cmd: String = dialoguer::Input::new()
        .with_prompt("password_cmd")
        .interact_text()
        .context("failed to read password_cmd")?;
    let pw_cmd = pw_cmd.trim();
    if pw_cmd.is_empty() {
        anyhow::bail!("password_cmd is empty — refusing to save");
    }

    // Verify the command actually works before persisting. Catches typos
    // before the next session expiry forces a re-login at an inconvenient
    // moment. Same gates as Credentials::resolve_password applies later.
    let probe = std::process::Command::new("sh")
        .arg("-c")
        .arg(pw_cmd)
        .output()
        .context("password_cmd: failed to spawn shell")?;
    if !probe.status.success() {
        anyhow::bail!(
            "password_cmd exited with {} (stderr: {})",
            probe.status,
            String::from_utf8_lossy(&probe.stderr).trim()
        );
    }
    if probe.stdout.is_empty() {
        anyhow::bail!("password_cmd succeeded but produced no output — refusing to save");
    }

    let creds = Credentials {
        username: username.to_string(),
        password: None,
        password_cmd: Some(pw_cmd.to_string()),
    };
    let creds_path = creds.save().context("failed to save credentials")?;
    eprintln!("Saved {} (mode 0600).", creds_path.display());
    Ok(())
}

fn save_session(session: &Session) -> Result<()> {
    let path = Session::default_path();
    session.save(&path).context("failed to save session")?;
    Ok(())
}
