use anyhow::Result;

use crate::output::OutputFormat;

pub async fn run(opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let profile = cool_tools::profile::whoami(&client).await?;

    let fmt = OutputFormat::from_flag(opts.json);

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&profile)?);
        }
        OutputFormat::Table => {
            println!(
                "Name:  {}",
                profile.name.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "Login: {}",
                profile.login_id.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "Email: {}",
                profile.primary_email.as_deref().unwrap_or("(unknown)")
            );
        }
    }

    Ok(())
}
