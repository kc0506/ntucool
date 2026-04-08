use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use reqwest::redirect::Policy;
use serde::Deserialize;

use crate::error::Error;
use crate::session::Session;

pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";
const BASE_URL: &str = "https://cool.ntu.edu.tw";

#[derive(Debug, Deserialize)]
struct Credentials {
    username: String,
    password: Option<String>,
    password_cmd: Option<String>,
}

impl Credentials {
    fn default_path() -> PathBuf {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").expect("HOME not set");
                PathBuf::from(home).join(".config")
            });
        config_home.join("ntucool").join("credentials.json")
    }

    fn load(path: &PathBuf) -> Result<Self, Error> {
        let data = fs::read_to_string(path)
            .map_err(|_| Error::NoCredentials(path.display().to_string()))?;
        let creds: Credentials =
            serde_json::from_str(&data).map_err(|e| Error::Auth(e.to_string()))?;
        Ok(creds)
    }

    fn resolve_password(&self) -> Result<String, Error> {
        if let Some(ref pw) = self.password {
            return Ok(pw.clone());
        }
        if let Some(ref cmd) = self.password_cmd {
            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .map_err(|e| Error::PasswordCmd(e.to_string()))?;
            if !output.status.success() {
                return Err(Error::PasswordCmd(format!(
                    "command `{}` exited with {}",
                    cmd, output.status
                )));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        Err(Error::Auth(
            "credentials.json has neither password nor password_cmd".into(),
        ))
    }
}

/// Perform the full NTU ADFS SAML login flow and return a Session.
pub async fn saml_login(username: &str, password: &str) -> Result<Session, Error> {
    saml_login_inner(username, password, false).await
}

/// Same as `saml_login` but prints debug info for each step.
pub async fn saml_login_verbose(username: &str, password: &str) -> Result<Session, Error> {
    saml_login_inner(username, password, true).await
}

async fn saml_login_inner(username: &str, password: &str, verbose: bool) -> Result<Session, Error> {
    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| Error::Auth(e.to_string()))?;

    // Step 1: GET /login/saml -> capture redirect to ADFS
    if verbose { eprintln!("[debug] Step 1: GET {BASE_URL}/login/saml"); }
    let resp = http
        .get(format!("{BASE_URL}/login/saml"))
        .header("Referer", format!("{BASE_URL}/login/portal"))
        .send()
        .await
        .map_err(|e| Error::Auth(format!("SAML redirect: {e}")))?;

    if verbose { eprintln!("[debug] Step 1: status={}", resp.status()); }

    let adfs_url = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::Auth("No redirect from /login/saml".into()))?
        .to_string();

    if verbose { eprintln!("[debug] Step 1: redirect -> {adfs_url}"); }

    // Step 2: GET ADFS URL -> parse HTML form
    // Enable cookie_store so ADFS session cookies from Step 2 are sent in Step 3
    let http_follow = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .cookie_store(true)
        .build()
        .map_err(|e| Error::Auth(e.to_string()))?;

    if verbose { eprintln!("[debug] Step 2: GET {adfs_url}"); }
    let resp = http_follow
        .get(&adfs_url)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("ADFS page: {e}")))?;

    if verbose { eprintln!("[debug] Step 2: status={}", resp.status()); }

    let html = resp
        .text()
        .await
        .map_err(|e| Error::Auth(format!("ADFS page body: {e}")))?;

    if verbose { eprintln!("[debug] Step 2: HTML length={}", html.len()); }

    let adfs_form = parse_adfs_form(&html)?;

    if verbose {
        eprintln!("[debug] Step 2: form_action={}", adfs_form.action);
        eprintln!("[debug] Step 2: query_params={}", adfs_form.query_params.len());
        eprintln!("[debug] Step 2: fields={}, acc={}, pass={}", adfs_form.hidden_fields.len(), adfs_form.username_field, adfs_form.password_field);
    }

    // Step 3: POST credentials to ADFS
    // Match Python: POST to base URL with query params separate, form data in body
    let mut post_data = adfs_form.hidden_fields;
    post_data.insert(adfs_form.username_field, username.to_string());
    post_data.insert(adfs_form.password_field, password.to_string());

    let post_url = "https://adfs.ntu.edu.tw/adfs/ls/";
    if verbose { eprintln!("[debug] Step 3: POST {post_url} with {} query params", adfs_form.query_params.len()); }
    let resp = http_follow
        .post(post_url)
        .query(&adfs_form.query_params)
        .header("Connection", "keep-alive")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Origin", "https://adfs.ntu.edu.tw")
        .form(&post_data)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("ADFS post: {e}")))?;

    if verbose { eprintln!("[debug] Step 3: status={}", resp.status()); }

    let saml_resp_html = resp
        .text()
        .await
        .map_err(|e| Error::Auth(format!("SAML response body: {e}")))?;

    if verbose {
        eprintln!("[debug] Step 3: response HTML length={}", saml_resp_html.len());
        // Print form field names being sent (not values for security)
        let keys: Vec<&String> = post_data.keys().collect();
        eprintln!("[debug] Step 3: POST fields={keys:?}");
        // Check if ADFS returned an error message in the HTML
        if let Some(idx) = saml_resp_html.find("errorText") {
            let start = idx.saturating_sub(50);
            let end = (idx + 200).min(saml_resp_html.len());
            eprintln!("[debug] Step 3: error in HTML: ...{}...", &saml_resp_html[start..end]);
        }
        // Check for common error spans
        for marker in ["ErrorTextLabel", "error", "alert", "validation"] {
            if saml_resp_html.contains(marker) {
                // Find the surrounding context
                if let Some(pos) = saml_resp_html.find(marker) {
                    let start = pos.saturating_sub(20);
                    let end = (pos + 150).min(saml_resp_html.len());
                    eprintln!("[debug] Step 3: found '{marker}': ...{}...", &saml_resp_html[start..end]);
                }
            }
        }
        // Check if response contains SAMLResponse (= success)
        if saml_resp_html.contains("SAMLResponse") {
            eprintln!("[debug] Step 3: SAMLResponse found in response (login succeeded)");
        } else {
            eprintln!("[debug] Step 3: NO SAMLResponse in response (login likely failed)");
        }
    }

    // Step 4: POST SAML assertion back to Canvas -> get session cookies
    let (assertion_url, assertion_data) = parse_saml_assertion(&saml_resp_html)?;

    if verbose {
        eprintln!("[debug] Step 4: assertion_url={assertion_url}");
        eprintln!("[debug] Step 4: assertion_data keys={:?}", assertion_data.keys().collect::<Vec<_>>());
    }

    let http_no_redirect = reqwest::Client::builder()
        .redirect(Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| Error::Auth(e.to_string()))?;

    let resp = http_no_redirect
        .post(&assertion_url)
        .form(&assertion_data)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("SAML assertion post: {e}")))?;

    if verbose { eprintln!("[debug] Step 4: status={}", resp.status()); }

    let cookies = extract_cookies_from_response(&resp)?;

    if verbose { eprintln!("[debug] Step 4: got {} cookies", cookies.len()); }

    Ok(Session::new(BASE_URL.to_string(), cookies))
}

/// Try to login using saved credentials.
pub async fn login_with_saved_credentials() -> Result<Session, Error> {
    login_with_saved_credentials_inner(false).await
}

/// Try to login using saved credentials (verbose).
pub async fn login_with_saved_credentials_verbose(verbose: bool) -> Result<Session, Error> {
    login_with_saved_credentials_inner(verbose).await
}

async fn login_with_saved_credentials_inner(verbose: bool) -> Result<Session, Error> {
    let creds_path = Credentials::default_path();
    if verbose { eprintln!("[debug] Credentials path: {}", creds_path.display()); }
    let creds = Credentials::load(&creds_path)?;
    let password = creds.resolve_password()?;
    saml_login_inner(&creds.username, &password, verbose).await
}

// ----- HTML parsing helpers -----

struct AdfsForm {
    action: String,
    query_params: Vec<(String, String)>,
    hidden_fields: HashMap<String, String>,
    username_field: String,
    password_field: String,
}

fn parse_adfs_form(html: &str) -> Result<AdfsForm, Error> {
    let document = scraper::Html::parse_document(html);

    let form_sel =
        scraper::Selector::parse("form#MainForm").map_err(|_| Error::Auth("selector".into()))?;
    let input_sel =
        scraper::Selector::parse("input").map_err(|_| Error::Auth("selector".into()))?;

    let form = document
        .select(&form_sel)
        .next()
        .ok_or_else(|| Error::Auth("MainForm not found in ADFS page".into()))?;

    let action = form
        .value()
        .attr("action")
        .ok_or_else(|| Error::Auth("form action not found".into()))?;

    // Reconstruct full URL
    let form_action = if action.starts_with("http") {
        action.to_string()
    } else {
        format!("https://adfs.ntu.edu.tw{action}")
    };

    // Parse query params from form action URL (matching Python's parse_qs behavior)
    let query_params: Vec<(String, String)> = if let Some(q_pos) = form_action.find('?') {
        form_action[q_pos + 1..]
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?;
                let val = parts.next().unwrap_or("");
                Some((
                    urlencoding::decode(key).unwrap_or_else(|_| key.into()).into_owned(),
                    urlencoding::decode(val).unwrap_or_else(|_| val.into()).into_owned(),
                ))
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut fields = HashMap::new();
    let mut acc_name = String::new();
    let mut pass_name = String::new();

    for input in form.select(&input_sel) {
        let input_type = input.value().attr("type").unwrap_or("");
        let name = input.value().attr("name").unwrap_or("").to_string();

        match input_type {
            "text" => acc_name = name,
            "password" => pass_name = name,
            _ => {
                if let Some(val) = input.value().attr("value") {
                    if !name.is_empty() {
                        fields.insert(name, val.to_string());
                    }
                }
            }
        }
    }

    if acc_name.is_empty() || pass_name.is_empty() {
        return Err(Error::Auth(
            "Could not find username/password fields in ADFS form".into(),
        ));
    }

    Ok(AdfsForm {
        action: form_action,
        query_params,
        hidden_fields: fields,
        username_field: acc_name,
        password_field: pass_name,
    })
}

fn parse_saml_assertion(html: &str) -> Result<(String, HashMap<String, String>), Error> {
    let document = scraper::Html::parse_document(html);

    let form_sel = scraper::Selector::parse("form").map_err(|_| Error::Auth("selector".into()))?;
    let input_sel =
        scraper::Selector::parse("input").map_err(|_| Error::Auth("selector".into()))?;

    let form = document
        .select(&form_sel)
        .next()
        .ok_or_else(|| Error::Auth("No form in SAML response (invalid credentials?)".into()))?;

    let action = form
        .value()
        .attr("action")
        .ok_or_else(|| Error::Auth("form action not found in SAML response".into()))?
        .to_string();

    let mut data = HashMap::new();
    for input in form.select(&input_sel) {
        let name = input.value().attr("name").unwrap_or("").to_string();
        let value = input.value().attr("value").unwrap_or("").to_string();
        if !name.is_empty() {
            data.insert(name, value);
        }
    }

    // If the form has no SAMLResponse, ADFS returned the login form again
    if !data.contains_key("SAMLResponse") {
        if html.contains("The user name or password is incorrect") {
            return Err(Error::Auth("Login failed: incorrect username or password".into()));
        }
        return Err(Error::Auth(
            "Login failed: ADFS returned the login form instead of a SAML assertion (unknown error)".into(),
        ));
    }

    Ok((action, data))
}

fn extract_cookies_from_response(
    resp: &reqwest::Response,
) -> Result<HashMap<String, String>, Error> {
    let mut cookies = HashMap::new();

    for cookie_header in resp.headers().get_all("set-cookie") {
        let cookie_str = cookie_header
            .to_str()
            .map_err(|_| Error::Auth("invalid cookie header".into()))?;
        // Parse "name=value; ..." format
        if let Some(kv) = cookie_str.split(';').next() {
            if let Some((name, value)) = kv.split_once('=') {
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
    }

    if cookies.is_empty() {
        return Err(Error::Auth(
            "No cookies received from Canvas (login may have failed)".into(),
        ));
    }

    Ok(cookies)
}
