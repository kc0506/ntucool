#![allow(dead_code)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
#[allow(unused_imports)]
use cool_api::generated::models::{
    Assignment, Course, DiscussionTopic, File, Folder, Module, Page, Quiz, User,
};
use cool_api::CoolClient;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::data::DataService;

/// Which top-level screen is active
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Login,
    Dashboard,
    CourseDetail(i64), // course_id
}

/// Focus within the dashboard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardFocus {
    Courses,
    Deadlines,
}

/// Tabs within a course detail view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CourseTab {
    Assignments,
    Announcements,
    Discussions,
    Files,
    Modules,
    Pages,
    Quizzes,
    Grades,
}

impl CourseTab {
    pub fn all() -> &'static [CourseTab] {
        &[
            Self::Assignments,
            Self::Announcements,
            Self::Discussions,
            Self::Files,
            Self::Modules,
            Self::Pages,
            Self::Quizzes,
            Self::Grades,
        ]
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Assignments => "Assignments",
            Self::Announcements => "Announcements",
            Self::Discussions => "Discussions",
            Self::Files => "Files",
            Self::Modules => "Modules",
            Self::Pages => "Pages",
            Self::Quizzes => "Quizzes",
            Self::Grades => "Grades",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Assignments => "A",
            Self::Announcements => "N",
            Self::Discussions => "D",
            Self::Files => "F",
            Self::Modules => "M",
            Self::Pages => "P",
            Self::Quizzes => "Q",
            Self::Grades => "G",
        }
    }

    pub fn shortcut(&self) -> char {
        match self {
            Self::Assignments => 'a',
            Self::Announcements => 'n',
            Self::Discussions => 'd',
            Self::Files => 'f',
            Self::Modules => 'm',
            Self::Pages => 'p',
            Self::Quizzes => 'q',
            Self::Grades => 'g',
        }
    }
}

/// An overlay modal that appears on top of the current screen
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Help,
    Search(SearchState),
    AssignmentDetail(AssignmentDetailState),
    TopicDetail(TopicDetailState),
    FilePreview(FilePreviewState),
    Confirm(ConfirmState),
    Notification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicDetailState {
    pub course_id: i64,
    pub topic_idx: usize,
    pub is_announcement: bool,
    pub scroll: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchState {
    pub query: String,
    pub cursor: usize,
    pub results: Vec<SearchResult>,
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub label: String,
    pub category: String, // "Course", "Assignment", "File", etc.
    pub course_id: Option<i64>,
    pub item_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentDetailState {
    pub course_id: i64,
    pub assignment_idx: usize,
    pub scroll: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreviewState {
    pub course_id: i64,
    pub file_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmState {
    pub message: String,
    pub selected_yes: bool,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Download(i64, String), // file_id, filename
    Quit,
}

/// Course detail view state
#[derive(Debug, Clone)]
pub struct CourseViewState {
    pub course_id: i64,
    pub course_name: String,
    pub active_tab: CourseTab,
    pub tab_selected: std::collections::HashMap<CourseTab, usize>,
    pub assignments: Vec<Assignment>,
    pub announcements: Vec<DiscussionTopic>,
    pub discussions: Vec<DiscussionTopic>,
    pub files: Vec<File>,
    pub folders: Vec<Folder>,
    pub current_folder_id: Option<i64>,
    pub folder_path: Vec<(i64, String)>, // breadcrumb: (folder_id, name)
    pub modules: Vec<Module>,
    pub pages: Vec<Page>,
    pub quizzes: Vec<Quiz>,
    pub loading: bool,
}

impl CourseViewState {
    pub fn new(course_id: i64, course_name: String) -> Self {
        let mut tab_selected = std::collections::HashMap::new();
        for tab in CourseTab::all() {
            tab_selected.insert(*tab, 0);
        }
        Self {
            course_id,
            course_name,
            active_tab: CourseTab::Assignments,
            tab_selected,
            assignments: Vec::new(),
            announcements: Vec::new(),
            discussions: Vec::new(),
            files: Vec::new(),
            folders: Vec::new(),
            current_folder_id: None,
            folder_path: Vec::new(),
            modules: Vec::new(),
            pages: Vec::new(),
            quizzes: Vec::new(),
            loading: true,
        }
    }

    pub fn selected(&self) -> usize {
        *self.tab_selected.get(&self.active_tab).unwrap_or(&0)
    }

    pub fn set_selected(&mut self, idx: usize) {
        self.tab_selected.insert(self.active_tab, idx);
    }

    pub fn item_count(&self) -> usize {
        match self.active_tab {
            CourseTab::Assignments => self.assignments.len(),
            CourseTab::Announcements => self.announcements.len(),
            CourseTab::Discussions => self.discussions.len(),
            CourseTab::Files => self.files.len() + self.folders.len(),
            CourseTab::Modules => self.modules.len(),
            CourseTab::Pages => self.pages.len(),
            CourseTab::Quizzes => self.quizzes.len(),
            CourseTab::Grades => 0,
        }
    }
}

/// Deadline info combining course + assignment
#[derive(Debug, Clone)]
pub struct Deadline {
    pub course_name: String,
    pub course_id: i64,
    pub assignment_name: String,
    pub assignment_id: i64,
    pub due_at: DateTime<Utc>,
    pub submitted: bool,
    pub points_possible: Option<f64>,
}

pub struct App {
    pub should_quit: bool,
    pub screen: Screen,
    pub modal: Option<Modal>,

    // Login
    pub login_username: String,
    pub login_password: String,
    pub login_field: usize, // 0 = username, 1 = password
    pub login_error: Option<String>,
    pub login_loading: bool,

    // Dashboard
    pub courses: Vec<Course>,
    pub deadlines: Vec<Deadline>,
    pub dashboard_focus: DashboardFocus,
    pub course_selected: usize,
    pub deadline_selected: usize,
    pub loading: bool,
    pub status_message: Option<String>,
    pub last_sync: Option<DateTime<Utc>>,

    // Course detail
    pub course_views: std::collections::HashMap<i64, CourseViewState>,

    // Navigation history
    pub history: Vec<Screen>,

    // User info
    pub current_user: Option<User>,

    // Data service
    pub data: Option<Arc<DataService>>,
    pub client: Option<Arc<CoolClient>>,

    // Recent courses for quick access
    pub recent_courses: Vec<i64>,

    // Notifications
    pub notifications: Vec<NotificationItem>,
    pub unread_count: usize,
}

#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub course_name: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
    pub notification_type: NotificationType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationType {
    Assignment,
    Announcement,
    Discussion,
    Grade,
    System,
}

impl App {
    pub fn new() -> Self {
        // Try loading existing session
        let (client, data) = match CoolClient::from_default_session() {
            Ok(c) => {
                let client = Arc::new(c);
                let data = Arc::new(DataService::new(client.clone()));
                (Some(client), Some(data))
            }
            Err(_) => (None, None),
        };

        let initial_screen = if client.is_some() {
            Screen::Dashboard
        } else {
            Screen::Login
        };

        Self {
            should_quit: false,
            screen: initial_screen,
            modal: None,
            login_username: String::new(),
            login_password: String::new(),
            login_field: 0,
            login_error: None,
            login_loading: false,
            courses: Vec::new(),
            deadlines: Vec::new(),
            dashboard_focus: DashboardFocus::Courses,
            course_selected: 0,
            deadline_selected: 0,
            loading: false,
            status_message: None,
            last_sync: None,
            course_views: std::collections::HashMap::new(),
            history: Vec::new(),
            current_user: None,
            data,
            client,
            recent_courses: Vec::new(),
            notifications: Vec::new(),
            unread_count: 0,
        }
    }

    pub async fn on_tick(&mut self) {
        // Auto-load on first tick if we have a session
        if self.screen == Screen::Dashboard && self.courses.is_empty() && !self.loading {
            self.load_dashboard().await;
        }
    }

    pub async fn on_key(&mut self, key: KeyEvent) {
        // Global shortcuts first
        if self.handle_global_keys(key).await {
            return;
        }

        // Modal takes priority
        if self.modal.is_some() {
            self.handle_modal_keys(key).await;
            return;
        }

        // Screen-specific keys
        match &self.screen {
            Screen::Login => self.handle_login_keys(key).await,
            Screen::Dashboard => self.handle_dashboard_keys(key).await,
            Screen::CourseDetail(cid) => {
                let cid = *cid;
                self.handle_course_keys(key, cid).await;
            }
        }
    }

    async fn handle_global_keys(&mut self, key: KeyEvent) -> bool {
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.should_quit = true;
                true
            }
            (KeyModifiers::CONTROL, KeyCode::Char('p')) if self.modal.is_none() => {
                self.open_search();
                true
            }
            (KeyModifiers::NONE, KeyCode::Char('?')) if self.modal.is_none() && self.screen != Screen::Login => {
                self.modal = Some(Modal::Help);
                true
            }
            (KeyModifiers::NONE, KeyCode::Char('!')) if self.modal.is_none() && self.screen != Screen::Login => {
                self.modal = Some(Modal::Notification);
                true
            }
            _ => false,
        }
    }

    // ─── Login ───

    async fn handle_login_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.login_field = (self.login_field + 1) % 2;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.login_field = if self.login_field == 0 { 1 } else { 0 };
            }
            KeyCode::Enter => {
                if !self.login_loading {
                    self.do_login().await;
                }
            }
            KeyCode::Char(c) => {
                if self.login_field == 0 {
                    self.login_username.push(c);
                } else {
                    self.login_password.push(c);
                }
            }
            KeyCode::Backspace => {
                if self.login_field == 0 {
                    self.login_username.pop();
                } else {
                    self.login_password.pop();
                }
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    async fn do_login(&mut self) {
        self.login_loading = true;
        self.login_error = None;
        self.status_message = Some("Authenticating via SAML...".to_string());

        match cool_api::auth::saml_login(&self.login_username, &self.login_password).await {
            Ok(session) => {
                let path = cool_api::Session::default_path();
                if let Err(e) = session.save(&path) {
                    self.login_error = Some(format!("Failed to save session: {e}"));
                    self.login_loading = false;
                    return;
                }
                // Also save credentials
                if let Err(e) = save_credentials(&self.login_username, &self.login_password) {
                    // Non-fatal
                    self.status_message = Some(format!("Warning: couldn't save credentials: {e}"));
                }

                let client = Arc::new(CoolClient::new(session, path));
                let data = Arc::new(DataService::new(client.clone()));
                self.client = Some(client);
                self.data = Some(data);
                self.screen = Screen::Dashboard;
                self.login_loading = false;
                self.status_message = Some("Login successful!".to_string());
            }
            Err(e) => {
                self.login_error = Some(format!("Login failed: {e}"));
                self.login_loading = false;
                self.status_message = None;
            }
        }
    }

    // ─── Dashboard ───

    async fn handle_dashboard_keys(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            (_, KeyCode::Tab) | (_, KeyCode::Char('l')) if self.dashboard_focus == DashboardFocus::Courses => {
                self.dashboard_focus = DashboardFocus::Deadlines;
            }
            (_, KeyCode::BackTab) | (_, KeyCode::Char('h')) if self.dashboard_focus == DashboardFocus::Deadlines => {
                self.dashboard_focus = DashboardFocus::Courses;
            }
            (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        if !self.courses.is_empty() {
                            self.course_selected = (self.course_selected + 1).min(self.courses.len() - 1);
                        }
                    }
                    DashboardFocus::Deadlines => {
                        if !self.deadlines.is_empty() {
                            self.deadline_selected = (self.deadline_selected + 1).min(self.deadlines.len() - 1);
                        }
                    }
                }
            }
            (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        self.course_selected = self.course_selected.saturating_sub(1);
                    }
                    DashboardFocus::Deadlines => {
                        self.deadline_selected = self.deadline_selected.saturating_sub(1);
                    }
                }
            }
            (_, KeyCode::Char('G')) => {
                // Go to last item
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        if !self.courses.is_empty() {
                            self.course_selected = self.courses.len() - 1;
                        }
                    }
                    DashboardFocus::Deadlines => {
                        if !self.deadlines.is_empty() {
                            self.deadline_selected = self.deadlines.len() - 1;
                        }
                    }
                }
            }
            (_, KeyCode::Char('g')) => {
                // Go to first item (gg in vim, simplified to single g here)
                match self.dashboard_focus {
                    DashboardFocus::Courses => self.course_selected = 0,
                    DashboardFocus::Deadlines => self.deadline_selected = 0,
                }
            }
            (_, KeyCode::Enter) => {
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        if let Some(course) = self.courses.get(self.course_selected) {
                            if let Some(id) = course.id {
                                let name = course.name.clone().unwrap_or_default();
                                self.navigate_to_course(id, name).await;
                            }
                        }
                    }
                    DashboardFocus::Deadlines => {
                        if let Some(deadline) = self.deadlines.get(self.deadline_selected) {
                            let cid = deadline.course_id;
                            let name = deadline.course_name.clone();
                            self.navigate_to_course(cid, name).await;
                        }
                    }
                }
            }
            (_, KeyCode::Char('r')) => {
                self.load_dashboard().await;
            }
            (_, KeyCode::Char('/')) => {
                self.open_search();
            }
            // Half-page scroll
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        if !self.courses.is_empty() {
                            self.course_selected = (self.course_selected + 10).min(self.courses.len() - 1);
                        }
                    }
                    DashboardFocus::Deadlines => {
                        if !self.deadlines.is_empty() {
                            self.deadline_selected = (self.deadline_selected + 10).min(self.deadlines.len() - 1);
                        }
                    }
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                match self.dashboard_focus {
                    DashboardFocus::Courses => {
                        self.course_selected = self.course_selected.saturating_sub(10);
                    }
                    DashboardFocus::Deadlines => {
                        self.deadline_selected = self.deadline_selected.saturating_sub(10);
                    }
                }
            }
            (_, KeyCode::Char('o')) => {
                // Open in browser
                if let Some(course) = self.courses.get(self.course_selected) {
                    if let Some(id) = course.id {
                        let _ = open::that(format!("https://cool.ntu.edu.tw/courses/{id}"));
                    }
                }
            }
            _ => {}
        }
    }

    // ─── Course Detail ───

    async fn handle_course_keys(&mut self, key: KeyEvent, course_id: i64) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) if key.modifiers.is_empty() => {
                // If in a subfolder, go up; otherwise go back
                if self.try_navigate_folder_up(course_id).await {
                    return;
                }
                self.go_back();
            }
            (_, KeyCode::Backspace) if key.modifiers.is_empty() => {
                if self.try_navigate_folder_up(course_id).await {
                    return;
                }
                self.go_back();
            }
            (_, KeyCode::Char('q')) => {
                self.go_back();
            }

            // Tab switching
            (_, KeyCode::Char('1')) => self.set_course_tab(course_id, CourseTab::Assignments),
            (_, KeyCode::Char('2')) => self.set_course_tab(course_id, CourseTab::Announcements),
            (_, KeyCode::Char('3')) => self.set_course_tab(course_id, CourseTab::Discussions),
            (_, KeyCode::Char('4')) => self.set_course_tab(course_id, CourseTab::Files),
            (_, KeyCode::Char('5')) => self.set_course_tab(course_id, CourseTab::Modules),
            (_, KeyCode::Char('6')) => self.set_course_tab(course_id, CourseTab::Pages),
            (_, KeyCode::Char('7')) => self.set_course_tab(course_id, CourseTab::Quizzes),
            (_, KeyCode::Char('8')) => self.set_course_tab(course_id, CourseTab::Grades),

            // Shortcut keys for tabs (NONE modifier only to avoid conflicts with Ctrl+D etc.)
            (KeyModifiers::NONE, KeyCode::Char('a')) => self.set_course_tab(course_id, CourseTab::Assignments),
            (KeyModifiers::NONE, KeyCode::Char('n')) => self.set_course_tab(course_id, CourseTab::Announcements),
            (KeyModifiers::NONE, KeyCode::Char('d')) => self.set_course_tab(course_id, CourseTab::Discussions),
            (KeyModifiers::NONE, KeyCode::Char('f')) => self.set_course_tab(course_id, CourseTab::Files),
            (KeyModifiers::NONE, KeyCode::Char('m')) => self.set_course_tab(course_id, CourseTab::Modules),
            (KeyModifiers::NONE, KeyCode::Char('p')) => self.set_course_tab(course_id, CourseTab::Pages),

            // Tab cycle
            (_, KeyCode::Tab) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let tabs = CourseTab::all();
                    let idx = tabs.iter().position(|t| *t == view.active_tab).unwrap_or(0);
                    view.active_tab = tabs[(idx + 1) % tabs.len()];
                }
            }
            (_, KeyCode::BackTab) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let tabs = CourseTab::all();
                    let idx = tabs.iter().position(|t| *t == view.active_tab).unwrap_or(0);
                    view.active_tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
                }
            }

            // Navigation within list
            (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let count = view.item_count();
                    if count > 0 {
                        let sel = view.selected();
                        view.set_selected((sel + 1).min(count - 1));
                    }
                }
            }
            (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let sel = view.selected();
                    view.set_selected(sel.saturating_sub(1));
                }
            }
            (_, KeyCode::Char('G')) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let count = view.item_count();
                    if count > 0 {
                        view.set_selected(count - 1);
                    }
                }
            }
            (_, KeyCode::Char('g')) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    view.set_selected(0);
                }
            }

            // Enter: open detail / navigate into folder
            (_, KeyCode::Enter) => {
                self.handle_course_enter(course_id).await;
            }

            // Refresh
            (_, KeyCode::Char('r')) => {
                self.load_course_data(course_id).await;
            }

            // Open in browser - contextual
            (_, KeyCode::Char('o')) => {
                if let Some(view) = self.course_views.get(&course_id) {
                    let base = format!("https://cool.ntu.edu.tw/courses/{course_id}");
                    match view.active_tab {
                        CourseTab::Assignments => {
                            if let Some(a) = view.assignments.get(view.selected()) {
                                if let Some(url) = &a.html_url {
                                    let _ = open::that(url);
                                } else {
                                    let _ = open::that(format!("{base}/assignments"));
                                }
                            }
                        }
                        CourseTab::Announcements => {
                            if let Some(a) = view.announcements.get(view.selected()) {
                                if let Some(url) = &a.html_url {
                                    let _ = open::that(url);
                                }
                            }
                        }
                        CourseTab::Discussions => {
                            if let Some(d) = view.discussions.get(view.selected()) {
                                if let Some(url) = &d.html_url {
                                    let _ = open::that(url);
                                }
                            }
                        }
                        CourseTab::Files => {
                            let _ = open::that(format!("{base}/files"));
                        }
                        CourseTab::Modules => {
                            let _ = open::that(format!("{base}/modules"));
                        }
                        CourseTab::Pages => {
                            if let Some(p) = view.pages.get(view.selected()) {
                                if let Some(slug) = &p.url {
                                    let _ = open::that(format!("{base}/pages/{slug}"));
                                }
                            }
                        }
                        CourseTab::Quizzes => {
                            if let Some(q) = view.quizzes.get(view.selected()) {
                                if let Some(url) = &q.html_url {
                                    let _ = open::that(url);
                                }
                            }
                        }
                        CourseTab::Grades => {
                            let _ = open::that(format!("{base}/grades"));
                        }
                    }
                }
            }

            // Half-page scroll
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let count = view.item_count();
                    if count > 0 {
                        let sel = view.selected();
                        view.set_selected((sel + 10).min(count - 1));
                    }
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                if let Some(view) = self.course_views.get_mut(&course_id) {
                    let sel = view.selected();
                    view.set_selected(sel.saturating_sub(10));
                }
            }

            // File download
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.download_selected_file(course_id).await;
            }

            _ => {}
        }
    }

    async fn handle_course_enter(&mut self, course_id: i64) {
        let view = match self.course_views.get(&course_id) {
            Some(v) => v,
            None => return,
        };

        match view.active_tab {
            CourseTab::Assignments => {
                let idx = view.selected();
                if idx < view.assignments.len() {
                    self.modal = Some(Modal::AssignmentDetail(AssignmentDetailState {
                        course_id,
                        assignment_idx: idx,
                        scroll: 0,
                    }));
                }
            }
            CourseTab::Files => {
                let sel = view.selected();
                let folder_count = view.folders.len();
                if sel < folder_count {
                    // Navigate into folder
                    if let Some(folder) = view.folders.get(sel) {
                        let folder_id = folder.id.unwrap_or(0);
                        let folder_name = folder.name.clone().unwrap_or_default();
                        self.navigate_into_folder(course_id, folder_id, folder_name).await;
                    }
                } else {
                    // Open file preview
                    let file_idx = sel - folder_count;
                    if file_idx < view.files.len() {
                        self.modal = Some(Modal::FilePreview(FilePreviewState {
                            course_id,
                            file_idx,
                        }));
                    }
                }
            }
            CourseTab::Announcements => {
                let idx = view.selected();
                if idx < view.announcements.len() {
                    self.modal = Some(Modal::TopicDetail(TopicDetailState {
                        course_id,
                        topic_idx: idx,
                        is_announcement: true,
                        scroll: 0,
                    }));
                }
            }
            CourseTab::Discussions => {
                let idx = view.selected();
                if idx < view.discussions.len() {
                    self.modal = Some(Modal::TopicDetail(TopicDetailState {
                        course_id,
                        topic_idx: idx,
                        is_announcement: false,
                        scroll: 0,
                    }));
                }
            }
            _ => {}
        }
    }

    // ─── Modal ───

    async fn handle_modal_keys(&mut self, key: KeyEvent) {
        let modal = self.modal.clone();
        match modal {
            Some(Modal::Help) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                        self.modal = None;
                    }
                    _ => {}
                }
            }
            Some(Modal::Search(ref state)) => {
                self.handle_search_keys(key, state.clone()).await;
            }
            Some(Modal::AssignmentDetail(ref state)) => {
                let state = state.clone();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.modal = None;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let Some(Modal::AssignmentDetail(ref mut s)) = self.modal {
                            s.scroll = s.scroll.saturating_add(1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let Some(Modal::AssignmentDetail(ref mut s)) = self.modal {
                            s.scroll = s.scroll.saturating_sub(1);
                        }
                    }
                    KeyCode::Char('o') => {
                        // Open in browser
                        if let Some(view) = self.course_views.get(&state.course_id) {
                            if let Some(assignment) = view.assignments.get(state.assignment_idx) {
                                if let Some(url) = &assignment.html_url {
                                    let _ = open::that(url);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some(Modal::TopicDetail(ref state)) => {
                let state = state.clone();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.modal = None;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let Some(Modal::TopicDetail(ref mut s)) = self.modal {
                            s.scroll = s.scroll.saturating_add(1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let Some(Modal::TopicDetail(ref mut s)) = self.modal {
                            s.scroll = s.scroll.saturating_sub(1);
                        }
                    }
                    KeyCode::Char('o') => {
                        let topics = if state.is_announcement {
                            self.course_views.get(&state.course_id).map(|v| &v.announcements)
                        } else {
                            self.course_views.get(&state.course_id).map(|v| &v.discussions)
                        };
                        if let Some(topics) = topics {
                            if let Some(topic) = topics.get(state.topic_idx) {
                                if let Some(url) = &topic.html_url {
                                    let _ = open::that(url);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some(Modal::FilePreview(_)) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.modal = None;
                    }
                    KeyCode::Char('d') | KeyCode::Enter => {
                        // Download
                        if let Some(Modal::FilePreview(ref state)) = self.modal {
                            let course_id = state.course_id;
                            self.modal = None;
                            self.download_selected_file(course_id).await;
                        }
                    }
                    KeyCode::Char('o') => {
                        // Open URL in browser
                        if let Some(Modal::FilePreview(ref state)) = self.modal {
                            if let Some(view) = self.course_views.get(&state.course_id) {
                                if let Some(file) = view.files.get(state.file_idx) {
                                    if let Some(url) = &file.url {
                                        let _ = open::that(url);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some(Modal::Confirm(ref state)) => {
                let state = state.clone();
                match key.code {
                    KeyCode::Esc => {
                        self.modal = None;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if let Some(Modal::Confirm(ref mut s)) = self.modal {
                            s.selected_yes = !s.selected_yes;
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        if let Some(Modal::Confirm(ref mut s)) = self.modal {
                            s.selected_yes = !s.selected_yes;
                        }
                    }
                    KeyCode::Enter => {
                        if state.selected_yes {
                            match state.action {
                                ConfirmAction::Download(file_id, ref filename) => {
                                    self.do_download(file_id, filename.clone()).await;
                                }
                                ConfirmAction::Quit => {
                                    self.should_quit = true;
                                }
                            }
                        }
                        self.modal = None;
                    }
                    _ => {}
                }
            }
            Some(Modal::Notification) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.modal = None;
                    }
                    KeyCode::Char('c') => {
                        // Clear all notifications
                        self.notifications.clear();
                        self.unread_count = 0;
                        self.modal = None;
                    }
                    KeyCode::Char('r') => {
                        // Mark all read
                        for n in &mut self.notifications {
                            n.read = true;
                        }
                        self.unread_count = 0;
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }

    async fn handle_search_keys(&mut self, key: KeyEvent, state: SearchState) {
        match key.code {
            KeyCode::Esc => {
                self.modal = None;
            }
            KeyCode::Enter => {
                if let Some(result) = state.results.get(state.selected) {
                    if let Some(course_id) = result.course_id {
                        let name = result.label.clone();
                        self.modal = None;
                        self.navigate_to_course(course_id, name).await;
                    }
                }
            }
            KeyCode::Up => {
                if let Some(Modal::Search(ref mut s)) = self.modal {
                    s.selected = s.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(Modal::Search(ref mut s)) = self.modal {
                    if !s.results.is_empty() {
                        s.selected = (s.selected + 1).min(s.results.len() - 1);
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(Modal::Search(ref mut s)) = self.modal {
                    s.query.push(c);
                    s.selected = 0;
                    self.update_search_results();
                }
            }
            KeyCode::Backspace => {
                if let Some(Modal::Search(ref mut s)) = self.modal {
                    s.query.pop();
                    s.selected = 0;
                    self.update_search_results();
                }
            }
            _ => {}
        }
    }

    // ─── Navigation ───

    pub async fn navigate_to_course(&mut self, course_id: i64, course_name: String) {
        self.history.push(self.screen.clone());
        self.screen = Screen::CourseDetail(course_id);

        // Add to recent
        self.recent_courses.retain(|&id| id != course_id);
        self.recent_courses.insert(0, course_id);
        if self.recent_courses.len() > 10 {
            self.recent_courses.truncate(10);
        }

        if !self.course_views.contains_key(&course_id) {
            self.course_views.insert(course_id, CourseViewState::new(course_id, course_name));
            self.load_course_data(course_id).await;
        }
    }

    pub fn go_back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.screen = prev;
        }
    }

    fn set_course_tab(&mut self, course_id: i64, tab: CourseTab) {
        if let Some(view) = self.course_views.get_mut(&course_id) {
            view.active_tab = tab;
        }
    }

    // ─── Search ───

    fn open_search(&mut self) {
        self.modal = Some(Modal::Search(SearchState {
            query: String::new(),
            cursor: 0,
            results: self.build_search_index(),
            selected: 0,
        }));
    }

    fn build_search_index(&self) -> Vec<SearchResult> {
        let mut results = Vec::new();
        for course in &self.courses {
            let id = course.id.unwrap_or(0);
            let name = course.name.clone().unwrap_or_default();
            let code = course.course_code.clone().unwrap_or_default();
            results.push(SearchResult {
                label: name.clone(),
                category: "Course".to_string(),
                course_id: Some(id),
                item_id: None,
            });
            // Also add by course code
            if !code.is_empty() {
                results.push(SearchResult {
                    label: format!("{code} - {name}"),
                    category: "Course".to_string(),
                    course_id: Some(id),
                    item_id: None,
                });
            }
        }
        // Add deadlines as searchable
        for deadline in &self.deadlines {
            results.push(SearchResult {
                label: format!("{} ({})", deadline.assignment_name, deadline.course_name),
                category: "Assignment".to_string(),
                course_id: Some(deadline.course_id),
                item_id: Some(deadline.assignment_id),
            });
        }
        results
    }

    fn update_search_results(&mut self) {
        let query = match &self.modal {
            Some(Modal::Search(s)) => s.query.clone(),
            _ => return,
        };

        let all = self.build_search_index();

        let results = if query.is_empty() {
            all
        } else {
            use fuzzy_matcher::skim::SkimMatcherV2;
            use fuzzy_matcher::FuzzyMatcher;

            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, SearchResult)> = all
                .into_iter()
                .filter_map(|r| {
                    matcher
                        .fuzzy_match(&r.label, &query)
                        .map(|score| (score, r))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, r)| r).collect()
        };

        if let Some(Modal::Search(ref mut state)) = self.modal {
            state.results = results;
        }
    }

    // ─── Data Loading ───

    async fn load_dashboard(&mut self) {
        let data = match &self.data {
            Some(d) => d.clone(),
            None => return,
        };

        self.loading = true;
        self.status_message = Some("Loading courses...".to_string());

        // Load courses
        match data.get_courses().await {
            Ok(courses) => {
                self.courses = courses;
                self.status_message = Some(format!("Loaded {} courses", self.courses.len()));
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
                self.loading = false;
                return;
            }
        }

        // Load upcoming deadlines
        self.status_message = Some("Loading deadlines...".to_string());
        match data.get_upcoming_deadlines(&self.courses).await {
            Ok(deadlines) => {
                self.deadlines = deadlines;
            }
            Err(e) => {
                self.status_message = Some(format!("Error loading deadlines: {e}"));
            }
        }

        // Load user info
        if let Ok(user) = data.get_current_user().await {
            self.current_user = Some(user);
        }

        // Generate notifications from urgent deadlines
        self.notifications.clear();
        for deadline in &self.deadlines {
            let hours_left = (deadline.due_at - Utc::now()).num_hours();
            if hours_left < 24 && !deadline.submitted {
                self.notifications.push(NotificationItem {
                    id: deadline.assignment_id,
                    title: format!("Due soon: {}", deadline.assignment_name),
                    body: format!("Due in {}h", hours_left.max(0)),
                    course_name: Some(deadline.course_name.clone()),
                    timestamp: deadline.due_at,
                    read: false,
                    notification_type: NotificationType::Assignment,
                });
            }
        }
        self.unread_count = self.notifications.iter().filter(|n| !n.read).count();

        self.last_sync = Some(Utc::now());
        self.loading = false;
        self.status_message = Some(format!(
            "{} courses, {} upcoming deadlines",
            self.courses.len(),
            self.deadlines.len()
        ));
    }

    async fn load_course_data(&mut self, course_id: i64) {
        let data = match &self.data {
            Some(d) => d.clone(),
            None => return,
        };

        if let Some(view) = self.course_views.get_mut(&course_id) {
            view.loading = true;
        }

        let cid = course_id.to_string();

        // Load all data in parallel
        let (assignments, announcements, discussions, files, folders, modules, pages, quizzes) = tokio::join!(
            data.get_assignments(&cid),
            data.get_announcements(&cid),
            data.get_discussions(&cid),
            data.get_files(&cid),
            data.get_folders(&cid),
            data.get_modules(&cid),
            data.get_pages(&cid),
            data.get_quizzes(&cid),
        );

        if let Some(view) = self.course_views.get_mut(&course_id) {
            view.assignments = assignments.unwrap_or_default();
            view.announcements = announcements.unwrap_or_default();
            view.discussions = discussions.unwrap_or_default();
            view.files = files.unwrap_or_default();
            view.folders = folders.unwrap_or_default();
            view.modules = modules.unwrap_or_default();
            view.pages = pages.unwrap_or_default();
            view.quizzes = quizzes.unwrap_or_default();
            view.loading = false;
        }

        self.last_sync = Some(Utc::now());
    }

    async fn try_navigate_folder_up(&mut self, course_id: i64) -> bool {
        let view = match self.course_views.get(&course_id) {
            Some(v) => v,
            None => return false,
        };

        if view.active_tab != CourseTab::Files || view.folder_path.is_empty() {
            return false;
        }

        let data = match &self.data {
            Some(d) => d.clone(),
            None => return false,
        };

        // Pop current folder from path
        let view = self.course_views.get_mut(&course_id).unwrap();
        view.folder_path.pop();
        view.set_selected(0);

        if let Some((parent_id, _)) = view.folder_path.last() {
            // Navigate to parent folder
            let pid = parent_id.to_string();
            view.current_folder_id = Some(*parent_id);
            let (files, folders) = tokio::join!(
                data.get_folder_files(&pid),
                data.get_subfolders(&pid),
            );
            let view = self.course_views.get_mut(&course_id).unwrap();
            view.files = files.unwrap_or_default();
            view.folders = folders.unwrap_or_default();
        } else {
            // Back to root
            view.current_folder_id = None;
            let cid = course_id.to_string();
            let (files, folders) = tokio::join!(
                data.get_files(&cid),
                data.get_folders(&cid),
            );
            let view = self.course_views.get_mut(&course_id).unwrap();
            view.files = files.unwrap_or_default();
            view.folders = folders.unwrap_or_default();
        }

        true
    }

    async fn navigate_into_folder(&mut self, course_id: i64, folder_id: i64, folder_name: String) {
        let data = match &self.data {
            Some(d) => d.clone(),
            None => return,
        };

        if let Some(view) = self.course_views.get_mut(&course_id) {
            view.folder_path.push((folder_id, folder_name));
            view.current_folder_id = Some(folder_id);
            view.set_selected(0);
        }

        let fid = folder_id.to_string();
        let (files, folders) = tokio::join!(
            data.get_folder_files(&fid),
            data.get_subfolders(&fid),
        );

        if let Some(view) = self.course_views.get_mut(&course_id) {
            view.files = files.unwrap_or_default();
            view.folders = folders.unwrap_or_default();
        }
    }

    async fn download_selected_file(&mut self, course_id: i64) {
        let view = match self.course_views.get(&course_id) {
            Some(v) => v,
            None => return,
        };

        if view.active_tab != CourseTab::Files {
            return;
        }

        let sel = view.selected();
        let folder_count = view.folders.len();
        if sel < folder_count {
            return; // Can't download a folder
        }

        let file_idx = sel - folder_count;
        if let Some(file) = view.files.get(file_idx) {
            let filename = file.display_name.clone().or(file.filename.clone()).unwrap_or_default();
            let _file_id = file.id.unwrap_or(0);
            if let Some(url) = &file.url {
                self.do_download_url(url.clone(), filename).await;
            }
        }
    }

    async fn do_download(&mut self, _file_id: i64, filename: String) {
        self.status_message = Some(format!("Downloading {filename}..."));
        // Actual download would go here
        self.status_message = Some(format!("Downloaded {filename}"));
    }

    async fn do_download_url(&mut self, url: String, filename: String) {
        self.status_message = Some(format!("Downloading {filename}..."));

        // Download to ~/Downloads/
        let download_dir = dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
        let dest = download_dir.join(&filename);

        // Direct download via reqwest (the file URL is pre-signed)
        let http = reqwest::Client::new();
        let send_result: Result<reqwest::Response, reqwest::Error> = http.get(&url).send().await;
        match send_result {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&dest, &bytes) {
                        Ok(_) => {
                            self.status_message = Some(format!("Downloaded: {}", dest.display()));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Write failed: {e}"));
                        }
                    }
                }
                Err(e) => {
                    self.status_message = Some(format!("Download failed: {e}"));
                }
            },
            Err(e) => {
                self.status_message = Some(format!("Download failed: {e}"));
            }
        }
    }
}

fn save_credentials(username: &str, _password: &str) -> anyhow::Result<()> {
    let config_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("ntucool");
    std::fs::create_dir_all(&config_dir)?;
    let cred_path = config_dir.join("credentials.json");
    let creds = serde_json::json!({
        "username": username,
    });
    std::fs::write(&cred_path, serde_json::to_string_pretty(&creds)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cred_path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
