use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use cool_api::client::PaginatedResponse;
use cool_api::generated::endpoints;
use cool_api::generated::models::*;
use cool_api::generated::params::*;
use cool_api::CoolClient;
use tokio::sync::RwLock;

use crate::app::Deadline;

const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

struct CacheEntry<T> {
    data: T,
    fetched_at: Instant,
}

impl<T: Clone> CacheEntry<T> {
    fn is_valid(&self) -> bool {
        self.fetched_at.elapsed() < CACHE_TTL
    }
}

pub struct DataService {
    client: Arc<CoolClient>,
    courses_cache: RwLock<Option<CacheEntry<Vec<Course>>>>,
    assignments_cache: RwLock<HashMap<String, CacheEntry<Vec<Assignment>>>>,
    announcements_cache: RwLock<HashMap<String, CacheEntry<Vec<DiscussionTopic>>>>,
    discussions_cache: RwLock<HashMap<String, CacheEntry<Vec<DiscussionTopic>>>>,
    files_cache: RwLock<HashMap<String, CacheEntry<Vec<File>>>>,
    folders_cache: RwLock<HashMap<String, CacheEntry<Vec<Folder>>>>,
    modules_cache: RwLock<HashMap<String, CacheEntry<Vec<Module>>>>,
    pages_cache: RwLock<HashMap<String, CacheEntry<Vec<Page>>>>,
    quizzes_cache: RwLock<HashMap<String, CacheEntry<Vec<Quiz>>>>,
}

impl DataService {
    pub fn new(client: Arc<CoolClient>) -> Self {
        Self {
            client,
            courses_cache: RwLock::new(None),
            assignments_cache: RwLock::new(HashMap::new()),
            announcements_cache: RwLock::new(HashMap::new()),
            discussions_cache: RwLock::new(HashMap::new()),
            files_cache: RwLock::new(HashMap::new()),
            folders_cache: RwLock::new(HashMap::new()),
            modules_cache: RwLock::new(HashMap::new()),
            pages_cache: RwLock::new(HashMap::new()),
            quizzes_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Collect all pages from a paginated endpoint
    async fn collect_pages<T: Clone + serde::de::DeserializeOwned>(
        &self,
        first_page: PaginatedResponse<T>,
    ) -> Vec<T> {
        let mut all = first_page.items;
        let mut next_url = first_page.next_url;

        while let Some(url) = next_url {
            match self
                .client
                .get_paginated::<T, ()>(&url, None::<&()>)
                .await
            {
                Ok(page) => {
                    all.extend(page.items);
                    next_url = page.next_url;
                }
                Err(_) => break,
            }
        }

        all
    }

    pub async fn get_courses(&self) -> anyhow::Result<Vec<Course>> {
        // Check cache
        {
            let cache = self.courses_cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListYourCoursesParams {
            enrollment_state: Some("active".to_string()),
            ..Default::default()
        };
        let page = endpoints::list_your_courses_page(&self.client, &params, None).await?;
        let courses = self.collect_pages(page).await;

        // Sort by name
        let mut courses = courses;
        courses.sort_by(|a, b| {
            a.name
                .as_deref()
                .unwrap_or("")
                .cmp(b.name.as_deref().unwrap_or(""))
        });

        // Cache
        {
            let mut cache = self.courses_cache.write().await;
            *cache = Some(CacheEntry {
                data: courses.clone(),
                fetched_at: Instant::now(),
            });
        }

        Ok(courses)
    }

    pub async fn get_assignments(&self, course_id: &str) -> anyhow::Result<Vec<Assignment>> {
        // Check cache
        {
            let cache = self.assignments_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListAssignmentsAssignmentsParams {
            order_by: Some("due_at".to_string()),
            ..Default::default()
        };
        let page =
            endpoints::list_assignments_assignments_page(&self.client, course_id, &params, None)
                .await?;
        let assignments = self.collect_pages(page).await;

        // Cache
        {
            let mut cache = self.assignments_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: assignments.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(assignments)
    }

    pub async fn get_announcements(&self, course_id: &str) -> anyhow::Result<Vec<DiscussionTopic>> {
        {
            let cache = self.announcements_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListAnnouncementsParams {
            context_codes: Some(vec![format!("course_{course_id}")]),
            ..Default::default()
        };
        let page = endpoints::list_announcements_page(&self.client, &params, None).await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.announcements_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_discussions(&self, course_id: &str) -> anyhow::Result<Vec<DiscussionTopic>> {
        {
            let cache = self.discussions_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListDiscussionTopicsCoursesParams::default();
        let page = endpoints::list_discussion_topics_courses_page(
            &self.client,
            course_id,
            &params,
            None,
        )
        .await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.discussions_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_files(&self, course_id: &str) -> anyhow::Result<Vec<File>> {
        {
            let cache = self.files_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListFilesCoursesParams::default();
        let page =
            endpoints::list_files_courses_page(&self.client, course_id, &params, None).await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.files_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_folders(&self, course_id: &str) -> anyhow::Result<Vec<Folder>> {
        {
            let cache = self.folders_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        // Get course root folder, then list its children
        let root_path = format!("/api/v1/courses/{course_id}/folders/root");
        let root: Folder = self.client.get(&root_path, None::<&()>).await?;

        if let Some(root_id) = root.id {
            let page =
                endpoints::list_folders_page(&self.client, &root_id.to_string(), None).await?;
            let items = self.collect_pages(page).await;

            {
                let mut cache = self.folders_cache.write().await;
                cache.insert(
                    course_id.to_string(),
                    CacheEntry {
                        data: items.clone(),
                        fetched_at: Instant::now(),
                    },
                );
            }

            Ok(items)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_folder_files(&self, folder_id: &str) -> anyhow::Result<Vec<File>> {
        let params = ListFilesFoldersParams::default();
        let page =
            endpoints::list_files_folders_page(&self.client, folder_id, &params, None).await?;
        Ok(self.collect_pages(page).await)
    }

    pub async fn get_subfolders(&self, folder_id: &str) -> anyhow::Result<Vec<Folder>> {
        let page = endpoints::list_folders_page(&self.client, folder_id, None).await?;
        Ok(self.collect_pages(page).await)
    }

    pub async fn get_modules(&self, course_id: &str) -> anyhow::Result<Vec<Module>> {
        {
            let cache = self.modules_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListModulesParams::default();
        let page =
            endpoints::list_modules_page(&self.client, course_id, &params, None).await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.modules_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_pages(&self, course_id: &str) -> anyhow::Result<Vec<Page>> {
        {
            let cache = self.pages_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListPagesCoursesParams::default();
        let page =
            endpoints::list_pages_courses_page(&self.client, course_id, &params, None).await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.pages_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_quizzes(&self, course_id: &str) -> anyhow::Result<Vec<Quiz>> {
        {
            let cache = self.quizzes_cache.read().await;
            if let Some(entry) = cache.get(course_id) {
                if entry.is_valid() {
                    return Ok(entry.data.clone());
                }
            }
        }

        let params = ListQuizzesInCourseParams::default();
        let page =
            endpoints::list_quizzes_in_course_page(&self.client, course_id, &params, None).await?;
        let items = self.collect_pages(page).await;

        {
            let mut cache = self.quizzes_cache.write().await;
            cache.insert(
                course_id.to_string(),
                CacheEntry {
                    data: items.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    pub async fn get_current_user(&self) -> anyhow::Result<User> {
        let params = ShowUserDetailsParams::default();
        let user = endpoints::show_user_details(&self.client, "self", &params).await?;
        Ok(user)
    }

    pub async fn get_upcoming_deadlines(
        &self,
        courses: &[Course],
    ) -> anyhow::Result<Vec<Deadline>> {
        let now = Utc::now();
        let mut deadlines = Vec::new();

        for course in courses {
            let course_id = match course.id {
                Some(id) => id,
                None => continue,
            };
            let course_name = course.name.clone().unwrap_or_default();

            match self.get_assignments(&course_id.to_string()).await {
                Ok(assignments) => {
                    for a in assignments {
                        if let Some(due_at) = a.due_at {
                            if due_at > now {
                                deadlines.push(Deadline {
                                    course_name: course_name.clone(),
                                    course_id,
                                    assignment_name: a.name.clone().unwrap_or_default(),
                                    assignment_id: a.id.unwrap_or(0),
                                    due_at,
                                    submitted: a
                                        .has_submitted_submissions
                                        .unwrap_or(false),
                                    points_possible: a.points_possible,
                                });
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        // Sort by due date
        deadlines.sort_by_key(|d| d.due_at);

        Ok(deadlines)
    }
}
