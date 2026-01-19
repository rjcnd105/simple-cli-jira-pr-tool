use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::OnceLock;

static JIRA_KEY_REGEX: OnceLock<Regex> = OnceLock::new();

#[derive(Parser)]
#[command(name = "simple-pr", about = "Jira & Bitbucket PR Automator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Bitbucket(BitbucketArgs),
}

#[derive(clap::Args)]
#[command(subcommand_negates_reqs = true)]
struct BitbucketArgs {
    #[command(subcommand)]
    command: Option<BitbucketSubcommands>,

    // --- Create Mode Arguments ---
    #[arg(long, required = true, num_args = 1..)]
    from: Option<Vec<String>>,
    #[arg(long, required = true)]
    to: Option<String>,
}

#[derive(Subcommand)]
enum BitbucketSubcommands {
    /// Find branches and matching Jira issues
    Find {
        /// Keywords to search branches (e.g. Jira keys)
        #[arg(long, required = true, num_args = 1..)]
        from: Vec<String>,

        /// Optional keyword to filter branch names
        #[arg(long)]
        to: Option<String>,

        /// Output format (md, etc.)
        #[arg(long, default_value = "md")]
        format: String,

        /// Show all matching PRs (default: 1 per keyword)
        #[arg(long, default_value = "false")]
        all: bool,
    },
    /// Create Pull Requests
    Create {
        #[arg(long, required = true, num_args = 1..)]
        from: Vec<String>,
        #[arg(long, required = true)]
        to: String,
    },
}

#[derive(Serialize)]
struct CreatePrPayload<'a> {
    title: &'a str,
    source: BranchRef<'a>,
    destination: BranchRef<'a>,
}

#[derive(Serialize)]
struct BranchRef<'a> {
    branch: BranchName<'a>,
}

#[derive(Serialize)]
struct BranchName<'a> {
    name: &'a str,
}

#[derive(Deserialize)]
struct RepoRefResponse {
    values: Vec<RepoBranch>,
}

#[derive(Deserialize)]
struct RepoBranch {
    name: String,
}

#[derive(Deserialize)]
struct RepoPrResponse {
    values: Vec<RepoPr>,
}

#[derive(Deserialize)]
struct RepoPr {
    state: String,
    links: RepoLinks,
    source: RepoBranchRef,
}

#[derive(Deserialize)]
struct RepoBranchRef {
    branch: RepoBranchName,
}

#[derive(Deserialize)]
struct RepoBranchName {
    name: String,
}

#[derive(Deserialize)]
struct RepoLinks {
    html: RepoLink,
}

#[derive(Deserialize)]
struct RepoLink {
    href: String,
}

#[derive(Deserialize)]
struct JiraIssue {
    fields: JiraFields,
}

#[derive(Deserialize)]
struct JiraFields {
    summary: String,
}

enum Action<'a> {
    Find {
        key: Option<&'a str>,
        summary: Option<&'a str>,
        pr_link: Option<&'a str>,
        branch: &'a str,
        state: Option<&'a str>,
    },
    Create {
        src: &'a str,
        target: &'a str,
        result: &'a Result<()>,
    },
}

// --- App Context ---
struct AppContext {
    client: Client,

    // Bitbucket Config
    bb_workspace: String,
    bb_repo: String,
    bb_token: String,

    // Jira Config
    jira_host: String,
    atlassian_email: String,
    jira_token: String,
}

impl AppContext {
    fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            client: Client::builder()
                .user_agent("pr-tool/1.0")
                .build()?,

            // Bitbucket (Bearer Auth용 토큰만 로드)
            bb_workspace: env::var("BITBUCKET_WORKSPACE").context("Env BITBUCKET_WORKSPACE missing")?,
            bb_repo:      env::var("BITBUCKET_REPO_SLUG").context("Env BITBUCKET_REPO_SLUG missing")?,
            bb_token:     env::var("BITBUCKET_API_TOKEN").context("Env BITBUCKET_API_TOKEN missing")?,

            // Jira (Basic Auth용)
            jira_host:    env::var("JIRA_HOST").context("Env JIRA_HOST missing")?,
            atlassian_email:   env::var("ATLASSIAN_EMAIL").context("Env ATLASSIAN_EMAIL missing")?,
            jira_token:   env::var("JIRA_API_TOKEN").context("Env JIRA_API_TOKEN missing")?,
        })
    }

    // --- API Helpers ---
    async fn check_status(&self, resp: Response, url: &str, service: &str) -> Result<Response> {
        if resp.status().is_success() {
            Ok(resp)
        } else {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!(
                "❌ {} API Fail [{}]: {}\n(URL: {})",
                service, status, error_text, url
            ))
        }
    }

    fn bb_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.basic_auth(&self.atlassian_email, Some(&self.bb_token))
    }

    fn jira_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.basic_auth(&self.atlassian_email, Some(&self.jira_token))
    }

    async fn find_branch(&self, keyword: &str) -> Result<String> {
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/refs/branches",
            self.bb_workspace, self.bb_repo
        );

        let resp = self.bb_auth(self.client.get(&url))
            .query(&[("q", format!("name~\"{}\"", keyword))])
            .send()
            .await?;

        let resp = self.check_status(resp, &url, "Bitbucket").await?;
        let resp_json = resp.json::<RepoRefResponse>().await?;

        // 1. 정확히 일치하는 브랜치 우선 검색 (dev 등)
        if let Some(exact_match) = resp_json.values.iter().find(|b| b.name == keyword) {
            return Ok(exact_match.name.clone());
        }

        // 2. 포함 검색 결과 사용
        resp_json.values.first()
            .map(|b| b.name.clone())
            .ok_or_else(|| anyhow::anyhow!("Branch not found for keyword: {}", keyword))
    }

    // async fn search_branches(&self, keyword: &str, filter: Option<&str>) -> Result<Vec<String>> {
    //     let url = format!(
    //         "https://api.bitbucket.org/2.0/repositories/{}/{}/refs/branches",
    //         self.bb_workspace, self.bb_repo
    //     );

    //     let mut q = format!("name~\"{}\"", keyword);
    //     if let Some(f) = filter {
    //         q.push_str(&format!(" AND name~\"{}\"", f));
    //     }

    //     let resp = self.bb_auth(self.client.get(&url))
    //         .query(&[("pagelen", "20"), ("q", q.as_str())])
    //         .send()
    //         .await?;

    //     let resp = self.check_status(resp, &url, "Bitbucket").await?;
    //     let resp_json = resp.json::<RepoRefResponse>().await?;
    //     Ok(resp_json.values.into_iter().map(|b| b.name).collect())
    // }

    /// PR 목록에서 source branch 이름으로 직접 검색합니다. (브랜치가 삭제된 병합 PR도 검색 가능)
    async fn search_prs(&self, keyword: &str, filter_to: Option<&str>) -> Result<Vec<RepoPr>> {
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/pullrequests",
            self.bb_workspace, self.bb_repo
        );

        let mut q = format!("source.branch.name ~ \"{}\"", keyword);
        if let Some(f) = filter_to {
            q.push_str(&format!(" AND destination.branch.name ~ \"{}\"", f));
        }

        let resp = self.bb_auth(self.client.get(&url))
            .query(&[
                ("q", q.as_str()),
                ("state", "ALL"),
                ("sort", "-created_on"),
                ("pagelen", "20")
            ])
            .send()
            .await?;

        let resp = self.check_status(resp, &url, "Bitbucket").await?;
        let resp_json = resp.json::<RepoPrResponse>().await?;
        Ok(resp_json.values)
    }

    async fn get_prs_for_branch(&self, branch_name: &str) -> Result<Vec<RepoPr>> {
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/pullrequests",
            self.bb_workspace, self.bb_repo
        );

        let resp = self.bb_auth(self.client.get(&url))
            .query(&[
                ("q", format!("source.branch.name = \"{}\" AND destination.branch.name != \"dev\"", branch_name)),
                ("state", "ALL".to_string()),
                ("sort", "-created_on".to_string()),
                ("pagelen", "1".to_string())
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let resp_json = resp.json::<RepoPrResponse>().await?;
        Ok(resp_json.values)
    }

    async fn get_jira_summary(&self, jira_key: &str) -> Result<String> {
        if !jira_key.contains("-") {
            return Ok(format!("Update from {}", jira_key));
        }

        let url = format!("{}/rest/api/3/issue/{}", self.jira_host, jira_key);

        let resp = self.jira_auth(self.client.get(&url))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(format!("Task {}", jira_key));
        }

        let issue = resp.json::<JiraIssue>().await?;
        Ok(issue.fields.summary)
    }

    async fn create_pr(&self, src_branch: &str, target_branch: &str, title: &str) -> Result<String> {
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/pullrequests",
            self.bb_workspace, self.bb_repo
        );

        let payload = CreatePrPayload {
            title,
            source: BranchRef { branch: BranchName { name: src_branch } },
            destination: BranchRef { branch: BranchName { name: target_branch } },
        };

        let resp = self.bb_auth(self.client.post(&url))
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(format!("Created: [{}] -> [{}]", src_branch, target_branch))
        } else {
            let err_msg = resp.text().await?;
            Err(anyhow::anyhow!("Bitbucket Error: {}", err_msg))
        }
    }

    fn print_result(&self, action: Action, format: &str, is_print_state: bool) {
        match action {
            Action::Find { key, summary, pr_link, branch, state } => {
                let pr_link_str = match pr_link {
                    Some(url) => format!("[PR]({})", url),
                    None => "(No valid PR)".to_string(),
                };

                let state_str = if is_print_state {
                    match state {
                    Some(s) => {
                        let icon = match s {
                            "OPEN" => "🟢",
                            "MERGED" => "🟣",
                            "DECLINED" => "🔴",
                            "SUPERSEDED" => "⚪",
                            _ => "❓",
                        };
                        format!("{} {}", icon, s)
                    },
                    None => "".to_string(),
                }
                } else {
                    "".to_string()
                };

                match format {
                    // Future formats can be added here
                    "md" | _ => {
                        if let Some(k) = key {
                            let jira_link = format!("{}/browse/{}", self.jira_host, k);
                            let sum = summary.unwrap_or("(Jira Info Failed)");
                            println!("[{}]({}) {} {} {}", k, jira_link, sum, pr_link_str, state_str);
                        } else {
                            println!("{} {} {}", branch, pr_link_str, state_str);
                        }
                    }
                }
            }
            Action::Create { src, target, result } => {
                match result {
                    Ok(_) => println!("✅ Created: [{}] -> [{}]", src, target),
                    Err(e) => println!("❌ Failed: {:#}", e),
                }
            }
        }
    }

    async fn print_branch_info(&self, branch_name: &str, pr_link: Option<&str>, pr_state: Option<&str>, format: &str) -> Result<()> {
        let re = JIRA_KEY_REGEX.get_or_init(|| Regex::new(r"([A-Z]+-\d+)").unwrap());

        let jira_key_opt = re.captures(branch_name)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string());

        // PR 정보가 없으면 직접 조회
        let (final_pr_link, final_state) = if pr_link.is_some() {
            (pr_link.map(|s| s.to_string()), pr_state.map(|s| s.to_string()))
        } else {
            let prs = self.get_prs_for_branch(branch_name).await?;
            let first_pr = prs.first();
            (
                first_pr.map(|pr| pr.links.html.href.clone()),
                first_pr.map(|pr| pr.state.clone())
            )
        };

        let summary = if let Some(key) = &jira_key_opt {
            self.get_jira_summary(key).await.ok()
        } else {
            None
        };

        self.print_result(Action::Find {
            key: jira_key_opt.as_deref(),
            summary: summary.as_deref(),
            pr_link: final_pr_link.as_deref(),
            branch: branch_name,
            state: final_state.as_deref(),
        }, format, true);

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = AppContext::from_env()?;
    let args = Cli::parse();

    match args.command {
        Commands::Bitbucket(bb_args) => {
            let command = bb_args.command.unwrap_or_else(|| BitbucketSubcommands::Create {
                from: bb_args.from.expect("--from is required for create mode"),
                to: bb_args.to.expect("--to is required for create mode"),
            });

            match command {
                // 1. Find Mode (PR 기반 검색 - 브랜치 삭제 여부와 무관하게 검색 가능)
                BitbucketSubcommands::Find { from, to, format, all } => {
                    let filter = to.as_deref();
                    let mode = if all { "all" } else { "latest 1 per keyword" };
                    println!("🔍 Searching PRs for from-branch '{}' (to-branch filter: {:?}, mode: {})...", from.join(", "), filter, mode);

                    for keyword in from {
                        let prs = app.search_prs(&keyword, filter).await?;

                        if prs.is_empty() {
                            println!("📭 No PRs found for source branch '{}'", keyword);
                            continue;
                        }

                        // --all이 없으면 키워드당 최신 1개만 출력
                        let prs_to_show: Vec<_> = if all {
                            prs
                        } else {
                            prs.into_iter().take(1).collect()
                        };

                        for pr in prs_to_show {
                            app.print_branch_info(
                                &pr.source.branch.name,
                                Some(&pr.links.html.href),
                                Some(&pr.state),
                                &format
                            ).await?;
                        }
                    }
                }
                // 2. Create Mode
                BitbucketSubcommands::Create { from, to } => {
                    println!("🔍 Finding target branch for '{}'...", to);
                    let target_branch = app.find_branch(&to).await?;
                    println!("🎯 Target Branch: {}\n", target_branch);
                    println!("Processing {} ... ", from.join(", "));

                    let mut successful_branches = Vec::new();

                    for src_key in from {
                        let result: Result<String> = async {
                            let src_branch = app.find_branch(&src_key).await?;
                            let summary = app.get_jira_summary(&src_key).await?;
                            let pr_title = format!("[{}] {}", src_key, summary);
                            app.create_pr(&src_branch, &target_branch, &pr_title).await?;
                            Ok(src_branch)
                        }.await;

                        match result {
                            Ok(src_branch) => {
                                app.print_result(Action::Create {
                                    src: &src_branch,
                                    target: &target_branch,
                                    result: &Ok(())
                                }, "md", false);
                                successful_branches.push(src_branch);
                            },
                            Err(e) => {
                                app.print_result(Action::Create {
                                    src: &src_key,
                                    target: &target_branch,
                                    result: &Err(e)
                                }, "md", false);
                            }
                        }
                    }

                    if !successful_branches.is_empty() {
                        println!("\n🚀 PR Summary:");
                        for branch in successful_branches {
                            app.print_branch_info(&branch, None, None, "md").await?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
