use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub created: DateTime<Utc>,
    pub status: TaskStatus,
    pub origin_prompt: String,
    pub goal: String,
    pub config_commits: Vec<String>,
    pub journal: Vec<JournalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Open,
    Resolved,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StateFile {
    next_id: u32,
}

#[derive(Clone)]
pub struct TaskStore {
    base: PathBuf,
    uid: Option<u32>,
    gid: Option<u32>,
}

impl TaskStore {
    pub fn new(config_repo: &Path, username: &str) -> Self {
        Self {
            base: config_repo.join("lares").join("tasks").join(username),
            uid: None,
            gid: None,
        }
    }

    pub fn with_ownership(config_repo: &Path, username: &str, uid: u32, gid: u32) -> Self {
        Self {
            base: config_repo.join("lares").join("tasks").join(username),
            uid: Some(uid),
            gid: Some(gid),
        }
    }

    pub fn create(&self, prompt: &str, goal: &str) -> Result<Task> {
        fs::create_dir_all(&self.base).context("creating tasks dir")?;
        self.chown_path(&self.base)?;

        let next_id = self.next_id()?;
        let id = format!("{:03}", next_id);
        let slug = slugify(goal);
        let task = Task {
            id: id.clone(),
            created: Utc::now(),
            status: TaskStatus::Open,
            origin_prompt: prompt.into(),
            goal: goal.into(),
            config_commits: vec![],
            journal: vec![JournalEntry {
                timestamp: Utc::now(),
                kind: "opened".into(),
                text: prompt.into(),
            }],
        };
        self.save_state(next_id + 1)?;
        let path = self.base.join(format!("{id}-{slug}.md"));
        let md = task_to_markdown(&task);
        fs::write(&path, md).with_context(|| format!("writing task {}", path.display()))?;
        self.chown_path(&path)?;
        Ok(task)
    }

    pub fn save(&self, task: &Task) -> Result<()> {
        let pattern = format!("{}-", task.id);
        let path = self.find_task_file(&pattern)?;
        let md = task_to_markdown(task);
        fs::write(&path, md).with_context(|| format!("saving task {}", path.display()))?;
        self.chown_path(&path)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Task> {
        let pattern = format!("{id}-");
        let path = self.find_task_file(&pattern)?;
        let text = fs::read_to_string(&path)?;
        parse_task_markdown(&text)
    }

    pub fn list(&self) -> Result<Vec<Task>> {
        if !self.base.exists() {
            return Ok(vec![]);
        }
        let mut tasks = vec![];
        for entry in fs::read_dir(&self.base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") {
                let text = fs::read_to_string(entry.path())?;
                if let Ok(t) = parse_task_markdown(&text) {
                    tasks.push(t);
                }
            }
        }
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(tasks)
    }

    fn next_id(&self) -> Result<u32> {
        let state_path = self.base.parent().unwrap().join("state.json");
        if state_path.exists() {
            let text = fs::read_to_string(&state_path)?;
            let state: StateFile = serde_json::from_str(&text)?;
            Ok(state.next_id)
        } else {
            Ok(1)
        }
    }

    fn save_state(&self, next_id: u32) -> Result<()> {
        let state_path = self.base.parent().unwrap().join("state.json");
        let state = StateFile { next_id };
        fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
        self.chown_path(&state_path)?;
        Ok(())
    }

    /// Change ownership of a file/directory to the user (if uid/gid are set)
    fn chown_path(&self, path: &Path) -> Result<()> {
        if let (Some(uid), Some(gid)) = (self.uid, self.gid) {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;

                let path_cstr = CString::new(path.as_os_str().as_bytes())
                    .context("converting path to CString")?;

                unsafe {
                    if libc::chown(path_cstr.as_ptr(), uid, gid) != 0 {
                        let err = std::io::Error::last_os_error();
                        anyhow::bail!("chown failed for {}: {}", path.display(), err);
                    }
                }
            }
        }
        Ok(())
    }

    fn find_task_file(&self, prefix: &str) -> Result<PathBuf> {
        for entry in fs::read_dir(&self.base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".md") {
                return Ok(entry.path());
            }
        }
        anyhow::bail!("task file matching {prefix}* not found")
    }
}

impl Task {
    pub fn add_journal(&mut self, kind: &str, text: &str) {
        self.journal.push(JournalEntry {
            timestamp: Utc::now(),
            kind: kind.into(),
            text: text.into(),
        });
    }
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(40)
        .collect()
}

fn task_to_markdown(task: &Task) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: \"{}\"\n", task.id));
    out.push_str(&format!("created: \"{}\"\n", task.created.to_rfc3339()));
    out.push_str(&format!("status: \"{}\"\n", status_str(&task.status)));
    out.push_str(&format!(
        "origin_prompt: \"{}\"\n",
        task.origin_prompt.replace('"', "\\\"")
    ));
    out.push_str(&format!("goal: \"{}\"\n", task.goal.replace('"', "\\\"")));
    if !task.config_commits.is_empty() {
        out.push_str("config_commits:\n");
        for c in &task.config_commits {
            out.push_str(&format!("  - \"{c}\"\n"));
        }
    }
    out.push_str("---\n\n");

    for entry in &task.journal {
        out.push_str(&format!(
            "## [{}] {}\n\n{}\n\n",
            entry.kind,
            entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            entry.text
        ));
    }
    out
}

fn parse_task_markdown(text: &str) -> Result<Task> {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("---\n") else {
        anyhow::bail!("missing frontmatter");
    };
    let Some(split) = rest.find("\n---") else {
        anyhow::bail!("missing frontmatter end");
    };
    let frontmatter = &rest[..split];
    let body = &rest[split + 4..];

    let id = extract_field(frontmatter, "id")?;
    let created = extract_field(frontmatter, "created")?;
    let status = extract_field(frontmatter, "status")?;
    let origin_prompt = extract_field(frontmatter, "origin_prompt")?;
    let goal = extract_field(frontmatter, "goal")?;

    let created: DateTime<Utc> = created.parse().context("parsing created date")?;
    let status = match status.as_str() {
        "open" => TaskStatus::Open,
        "resolved" => TaskStatus::Resolved,
        "failed" => TaskStatus::Failed,
        other => anyhow::bail!("unknown status: {other}"),
    };

    let mut journal = vec![];
    for section in body.split("\n## ").filter(|s| !s.trim().is_empty()) {
        let section = section.trim();
        // Format: [kind] timestamp\n\ntext
        if let Some(bracket_end) = section.find(']') {
            let kind = section[1..bracket_end].to_string();
            let rest = &section[bracket_end + 1..];
            // Find first blank line
            let (timestamp_str, text) = if let Some(blank) = rest.find("\n\n") {
                (rest[..blank].trim(), rest[blank + 2..].trim())
            } else {
                (rest.trim(), "")
            };
            let timestamp = timestamp_str
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            journal.push(JournalEntry {
                timestamp,
                kind,
                text: text.into(),
            });
        }
    }

    Ok(Task {
        id,
        created,
        status,
        origin_prompt,
        goal,
        config_commits: vec![],
        journal,
    })
}

fn extract_field(frontmatter: &str, field: &str) -> Result<String> {
    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{field}:")) {
            let val = rest.trim().trim_matches('"').to_string();
            return Ok(val);
        }
    }
    anyhow::bail!("missing field: {field}")
}

fn status_str(s: &TaskStatus) -> &'static str {
    match s {
        TaskStatus::Open => "open",
        TaskStatus::Resolved => "resolved",
        TaskStatus::Failed => "failed",
    }
}
