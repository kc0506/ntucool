//! User lookup (`users_get`). For arbitrary `user_id` — teachers from
//! `courses_get.teachers`, authors from announcements/discussions, etc.
//!
//! The logged-in user should use `profile::whoami` instead, which hits
//! `/users/self/profile` and returns richer self-only fields like
//! `primary_email`.

use anyhow::Result;

use cool_api::generated::endpoints;
use cool_api::generated::params::ShowUserDetailsParams;
use cool_api::CoolClient;

use crate::types::UserSummary;

pub async fn users_get(client: &CoolClient, user_id: i64) -> Result<UserSummary> {
    let id_str = user_id.to_string();
    let params = ShowUserDetailsParams::default();
    let user = endpoints::show_user_details(client, &id_str, &params).await?;
    Ok(UserSummary {
        id: user.id.unwrap_or(user_id),
        name: user.name.unwrap_or_default(),
        short_name: user.short_name,
        sortable_name: user.sortable_name,
        login_id: user.login_id,
        email: user.email,
        avatar_url: user.avatar_url,
    })
}
