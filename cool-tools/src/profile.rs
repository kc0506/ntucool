//! User profile (`whoami`).

use anyhow::Result;

pub use cool_api::generated::models::Profile;
use cool_api::CoolClient;

use crate::types::ProfileSummary;

/// Fetch the current user's raw profile.
pub async fn whoami(client: &CoolClient) -> Result<Profile> {
    let p = cool_api::generated::endpoints::get_user_profile(client, "self").await?;
    Ok(p)
}

/// Contract-shape whoami: id, name, login_id, primary_email only.
pub async fn whoami_summary(client: &CoolClient) -> Result<ProfileSummary> {
    let p = whoami(client).await?;
    Ok(ProfileSummary {
        id: p.id.unwrap_or(0),
        name: p.name.unwrap_or_default(),
        login_id: p.login_id,
        primary_email: p.primary_email,
    })
}
