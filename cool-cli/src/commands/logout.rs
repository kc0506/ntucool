use anyhow::Result;
use clap::Parser;
use cool_api::credentials::Credentials;
use cool_api::session::Session;

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    /// Also delete saved credentials.json. Without this, only the session
    /// cookies are cleared — the next `cool login` can re-use the saved
    /// username + password_cmd.
    #[arg(long)]
    pub purge: bool,
}

pub async fn run(args: &LogoutArgs, _opts: &super::GlobalOpts) -> Result<()> {
    Session::delete_default()?;
    eprintln!("Removed session.");

    if args.purge {
        Credentials::delete()?;
        eprintln!("Removed saved credentials.");
    }
    Ok(())
}
