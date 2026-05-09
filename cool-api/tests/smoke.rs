//! Smoke test: login -> list_courses -> verify response.
//!
//! Requires valid credentials at $XDG_CONFIG_HOME/ntucool/credentials.json
//! (defaults to ~/.config/ntucool/credentials.json on Linux).
//! Run with: cargo test --test smoke -- --ignored

use cool_api::auth;
use cool_api::client::CoolClient;
use cool_api::generated::endpoints;
use cool_api::session::Session;
use futures::StreamExt;

#[tokio::test]
#[ignore] // Requires real credentials and network
async fn smoke_login_and_list_courses() {
    // Step 1: Login
    let session = auth::login_with_saved_credentials()
        .await
        .expect("login failed — check $XDG_CONFIG_HOME/ntucool/credentials.json");

    assert_eq!(session.base_url, "https://cool.ntu.edu.tw");
    assert!(!session.cookies.is_empty(), "no cookies returned");

    // Save session
    let session_path = Session::default_path();
    session.save(&session_path).expect("failed to save session");

    // Step 2: List courses
    let client = CoolClient::new(session, session_path);
    let courses: Vec<_> = endpoints::list_your_courses(&client, &Default::default())
        .take(5)
        .collect()
        .await;

    assert!(!courses.is_empty(), "no courses returned");

    for result in &courses {
        let course = result.as_ref().expect("course fetch failed");
        println!(
            "Course: id={:?}, name={:?}",
            course.id, course.name
        );
    }
}
