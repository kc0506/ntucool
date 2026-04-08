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
    eprintln!("Login successful!");

    // Ask to save credentials
    let save = dialoguer::Confirm::new()
        .with_prompt("Save credentials for future use?")
        .default(true)
        .interact()
        .unwrap_or(false);

    if save {
        save_credentials(&username)?;
        eprintln!("Credentials saved.");
    }

    Ok(())
}

fn save_session(session: &Session) -> Result<()> {
    let path = Session::default_path();
    session.save(&path).context("failed to save session")?;
    Ok(())
}

fn save_credentials(username: &str) -> Result<()> {
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
        "password_cmd": format!("echo 'TODO: replace with your password command'")
    });

    std::fs::write(&creds_path, serde_json::to_string_pretty(&creds)?)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds_path, std::fs::Permissions::from_mode(0o600))?;
    }

    eprintln!(
        "Saved to {}. Edit password_cmd to use a secure password manager.",
        creds_path.display()
    );

    Ok(())
}
