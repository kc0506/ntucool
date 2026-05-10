//! `grades_get` — per-course grade summary.
//!
//! Canvas endpoint: `/api/v1/users/self/enrollments?include[]=current_grade&type[]=StudentEnrollment`.
//!
//! We bypass codegen for two reasons:
//! 1. `ListEnrollmentsUsersParams` has `Vec<String>` fields and
//!    `serde_urlencoded` (used by reqwest's `.query()`) can't serialize those —
//!    same trap that hits `list_all` in courses.rs.
//! 2. Codegen `Grade.current_score` / `final_score` are `Option<String>` but
//!    NTU COOL returns numeric `0.0` — a typed deserialize errors out
//!    ("invalid type: floating point, expected a string"). Same family of bug
//!    as task #21 for assignment `points_possible`.
//!
//! So we hand-roll the request and parse with our own `RawEnrollment` shape
//! that types scores as `Option<f64>` directly.
//!
//! `course_id?` filter: applied client-side. Canvas's `course_id` query
//! parameter does not reliably restrict `/users/self/enrollments`, so this is
//! the safer path.

use anyhow::Result;
use serde::Deserialize;

use cool_api::client::PaginatedResponse;
use cool_api::CoolClient;

use crate::courses;
use crate::types::CourseGrade;

#[derive(Debug, Deserialize)]
struct RawEnrollment {
    course_id: Option<i64>,
    grades: Option<RawGrade>,
}

#[derive(Debug, Deserialize)]
struct RawGrade {
    html_url: Option<String>,
    current_grade: Option<String>,
    current_score: Option<f64>,
    final_grade: Option<String>,
    final_score: Option<f64>,
}

pub async fn grades_get(
    client: &CoolClient,
    course_id_filter: Option<i64>,
) -> Result<Vec<CourseGrade>> {
    let query: [(&str, &str); 4] = [
        ("type[]", "StudentEnrollment"),
        ("state[]", "active"),
        ("include[]", "current_grade"),
        ("per_page", "100"),
    ];

    let course_summaries =
        courses::list_summaries(client, courses::ListFilter::Active, None)
            .await
            .unwrap_or_default();

    let mut out = Vec::new();
    let mut next_url: Option<String> = None;
    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/users/self/enrollments");
        let page: PaginatedResponse<RawEnrollment> = if next_url.is_some() {
            client.get_paginated(path, None::<&()>).await?
        } else {
            client.get_paginated(path, Some(&query)).await?
        };

        for en in page.items {
            let cid = match en.course_id {
                Some(id) => id,
                None => continue,
            };
            if let Some(filter_id) = course_id_filter {
                if cid != filter_id {
                    continue;
                }
            }
            let course_name = course_summaries
                .iter()
                .find(|c| c.id == cid)
                .map(|c| c.name.clone());
            let g = en.grades;
            out.push(CourseGrade {
                course_id: cid,
                course_name,
                current_grade: g.as_ref().and_then(|x| x.current_grade.clone()),
                current_score: g.as_ref().and_then(|x| x.current_score),
                final_grade: g.as_ref().and_then(|x| x.final_grade.clone()),
                final_score: g.as_ref().and_then(|x| x.final_score),
                html_url: g.and_then(|x| x.html_url),
            });
        }

        match page.next_url {
            Some(u) => next_url = Some(u),
            None => break,
        }
    }
    Ok(out)
}
