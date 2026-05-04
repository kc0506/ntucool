//! User profile (`whoami`).

use anyhow::Result;

pub use cool_api::generated::models::Profile;
use cool_api::CoolClient;

/// Fetch the current user's profile.
pub async fn whoami(client: &CoolClient) -> Result<Profile> {
    let p = cool_api::generated::endpoints::get_user_profile(client, "self").await?;
    Ok(p)
}
