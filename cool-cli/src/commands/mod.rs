pub mod announcement;
pub mod assignment;
pub mod course;
pub mod discussion;
pub mod file;
pub mod login;
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

/// Create a CoolClient from the saved session, or bail with a helpful message.
pub fn get_client() -> anyhow::Result<cool_api::CoolClient> {
    cool_api::CoolClient::from_default_session().map_err(|_| {
        anyhow::anyhow!("No valid session found. Please run `cool login` first.")
    })
}
