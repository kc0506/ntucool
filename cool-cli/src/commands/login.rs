use anyhow::{Context, Result};
use cool_api::auth;
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

    // Optionally save credentials for non-interactive re-login. We DON'T
    // store the plaintext password — the user supplies a shell command
    // which prints the password (e.g. `pass show ntucool`, `op read ...`,
    // or even `cat ~/.secrets/ntucool` if they're OK with that). Pressing
    // enter at the prompt skips credential saving entirely; previously the
    // confirm-prompt wrote a TODO placeholder that pretended to work.
    eprintln!();
    eprintln!("Optional: save a password command so non-interactive re-logins work");
    eprintln!("(needed when MCP tools detect an expired session). Examples:");
    eprintln!("  pass show ntucool/password");
    eprintln!("  op read 'op://Personal/NTU COOL/password'");
    eprintln!("  cat ~/.secrets/ntu-cool");
    eprintln!("Leave blank to skip — `cool login` will then always prompt.");
    let pw_cmd: String = dialoguer::Input::new()
        .with_prompt("password_cmd (blank to skip)")
        .allow_empty(true)
        .interact_text()
        .unwrap_or_default();
    let pw_cmd = pw_cmd.trim();
    if pw_cmd.is_empty() {
        eprintln!("(no credentials saved)");
        return Ok(());
    }

    // Verify the command actually works before persisting it. Prevents the
    // user from saving a typo and discovering it next time their session
    // expires.
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

    let creds_path = save_credentials(&username, pw_cmd)?;
    eprintln!("Saved {} (mode 0600).", creds_path.display());
    Ok(())
}

fn save_session(session: &Session) -> Result<()> {
    let path = Session::default_path();
    session.save(&path).context("failed to save session")?;
    Ok(())
}

fn save_credentials(username: &str, password_cmd: &str) -> Result<std::path::PathBuf> {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            std::path::PathBuf::from(home).join(".config")
        });
    let creds_path = config_home.join("ntucool").join("credentials.json");

    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let creds = serde_json::json!({
        "username": username,
        "password_cmd": password_cmd,
    });
    std::fs::write(&creds_path, serde_json::to_string_pretty(&creds)?)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(creds_path)
}
