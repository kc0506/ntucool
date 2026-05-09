pub mod announcement;
pub mod assignment;
pub mod course;
pub mod discussion;
pub mod file;
pub mod login;
pub mod logout;
pub mod module;
pub mod whoami;

use clap::{Parser, Subcommand};

/// Global flags shared by all subcommands.
#[derive(Parser, Clone, Debug)]
pub struct GlobalOpts {
    /// Output JSON instead of table
    #[arg(long, global = true)]
    pub json: bool,

    /// Verbose debug output
    #[arg(long, short, global = true)]
    pub verbose: bool,
}

#[derive(Parser)]
#[command(name = "cool", about = "NTU COOL CLI")]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Login to NTU COOL
    Login,

    /// Remove saved session (and optionally credentials)
    Logout(logout::LogoutArgs),

    /// Show current user info
    Whoami,

    /// Course operations
    #[command(subcommand)]
    Course(course::CourseCommand),

    /// Assignment operations
    Assignment(assignment::AssignmentArgs),

    /// File operations
    #[command(subcommand)]
    File(file::FileCommand),

    /// Announcement operations
    #[command(subcommand)]
    Announcement(announcement::AnnouncementCommand),

    /// Discussion operations
    #[command(subcommand)]
    Discussion(discussion::DiscussionCommand),

    /// Module operations
    #[command(subcommand)]
    Module(module::ModuleCommand),
}

/// Construct a lazy `CoolClient`. If session.json is missing or expired
/// AND credentials.json is set up, the first authenticated call triggers
/// an automatic saml_login. If neither exists, the call surfaces a clear
/// "No credentials found" error from the canonical path.
pub fn get_client() -> anyhow::Result<cool_api::CoolClient> {
    Ok(cool_api::CoolClient::from_default_session_lazy())
}
