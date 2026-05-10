use anyhow::Result;
use clap::{Args, Subcommand};

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum UserCommand {
    /// Look up a user by id
    Get(UserGetArgs),
}

#[derive(Args)]
pub struct UserGetArgs {
    /// Canvas user ID. Use `cool whoami` for the logged-in user.
    pub user_id: i64,
}

pub async fn run(cmd: UserCommand, opts: &super::GlobalOpts) -> Result<()> {
    match cmd {
        UserCommand::Get(args) => get(args, opts).await,
    }
}

async fn get(args: UserGetArgs, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let user = cool_tools::users::users_get(&client, args.user_id).await?;
    let fmt = OutputFormat::from_flag(opts.json);

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&user)?),
        OutputFormat::Table => {
            println!("ID:        {}", user.id);
            println!("Name:      {}", user.name);
            if let Some(s) = &user.short_name {
                println!("Short:     {}", s);
            }
            if let Some(s) = &user.sortable_name {
                println!("Sortable:  {}", s);
            }
            println!(
                "Login:     {}",
                user.login_id.as_deref().unwrap_or("(hidden)")
            );
            println!(
                "Email:     {}",
                user.email.as_deref().unwrap_or("(hidden)")
            );
            if let Some(u) = &user.avatar_url {
                println!("Avatar:    {}", u);
            }
        }
    }
    Ok(())
}
