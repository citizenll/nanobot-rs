use crate::cron::{CronSchedule, CronService};
use crate::tools::base::Tool;
use crate::utils::{format_timestamp_ms, normalize_timezone_value, parse_timezone};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone};
use serde_json::{Map, Value, json};
use std::future::Future;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct CronContext {
    channel: String,
    chat_id: String,
}

tokio::task_local! {
    static CRON_EXECUTION_GUARD: bool;
}

pub struct CronTool {
    cron: Arc<CronService>,
    context: Mutex<CronContext>,
    default_timezone: String,
}

impl CronTool {
    pub fn new(cron: Arc<CronService>, default_timezone: impl Into<String>) -> Self {
        let default_timezone = default_timezone.into();
        Self {
            cron,
            context: Mutex::new(CronContext::default()),
            default_timezone: normalize_timezone_value(Some(default_timezone.as_str()))
                .unwrap_or_else(|| "UTC".to_string()),
        }
    }

    pub fn set_context(&self, channel: impl Into<String>, chat_id: impl Into<String>) {
        if let Ok(mut guard) = self.context.lock() {
            guard.channel = channel.into();
            guard.chat_id = chat_id.into();
        }
    }

    pub async fn with_cron_execution_guard<F, T>(future: F) -> T
    where
        F: Future<Output = T>,
    {
        if Self::in_cron_execution_context() {
            future.await
        } else {
            CRON_EXECUTION_GUARD.scope(true, future).await
        }
    }

    fn in_cron_execution_context() -> bool {
        CRON_EXECUTION_GUARD
            .try_with(|active| *active)
            .unwrap_or(false)
    }

    pub fn default_timezone(&self) -> &str {
        &self.default_timezone
    }

    fn validate_timezone(&self, timezone: &str) -> Option<String> {
        if parse_timezone(Some(timezone)).is_some() {
            None
        } else {
            Some(format!("Error: unknown timezone '{timezone}'"))
        }
    }

    fn display_timezone(&self, schedule: &CronSchedule) -> String {
        schedule
            .tz
            .clone()
            .unwrap_or_else(|| self.default_timezone.clone())
    }

    fn format_timing(&self, schedule: &CronSchedule) -> String {
        match schedule.kind.as_str() {
            "cron" => {
                let expr = schedule.expr.as_deref().unwrap_or_default();
                let tz_suffix = schedule
                    .tz
                    .as_deref()
                    .map(|tz| format!(" ({tz})"))
                    .unwrap_or_default();
                format!("cron: {expr}{tz_suffix}")
            }
            "every" => match schedule.every_ms.unwrap_or_default() {
                ms if ms >= 3_600_000 && ms % 3_600_000 == 0 => {
                    format!("every {}h", ms / 3_600_000)
                }
                ms if ms >= 60_000 && ms % 60_000 == 0 => format!("every {}m", ms / 60_000),
                ms if ms >= 1_000 && ms % 1_000 == 0 => format!("every {}s", ms / 1_000),
                ms if ms > 0 => format!("every {ms}ms"),
                _ => "every".to_string(),
            },
            "at" => schedule
                .at_ms
                .map(|ms| {
                    format!(
                        "at {}",
                        format_timestamp_ms(ms, Some(&self.display_timezone(schedule)))
                    )
                })
                .unwrap_or_else(|| "at".to_string()),
            other => other.to_string(),
        }
    }

    fn format_state(
        &self,
        state: &crate::cron::CronJobState,
        schedule: &CronSchedule,
    ) -> Vec<String> {
        let display_timezone = self.display_timezone(schedule);
        let mut lines = Vec::new();
        if let Some(last_run_at_ms) = state.last_run_at_ms {
            let mut info = format!(
                "  Last run: {} - {}",
                format_timestamp_ms(last_run_at_ms, Some(&display_timezone)),
                state.last_status.as_deref().unwrap_or("unknown")
            );
            if let Some(last_error) = state.last_error.as_deref() {
                info.push_str(&format!(" ({last_error})"));
            }
            lines.push(info);
        }
        if let Some(next_run_at_ms) = state.next_run_at_ms {
            lines.push(format!(
                "  Next run: {}",
                format_timestamp_ms(next_run_at_ms, Some(&display_timezone))
            ));
        }
        lines
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Schedule reminders and recurring tasks. Actions: add, list, remove."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "list", "remove"] },
                "message": { "type": "string" },
                "every_seconds": { "type": "integer" },
                "cron_expr": { "type": "string" },
                "tz": { "type": "string" },
                "at": { "type": "string" },
                "job_id": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: &Map<String, Value>) -> Result<String> {
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing required string field: action"))?;

        match action {
            "add" => self.add_job(params).await,
            "list" => self.list_jobs().await,
            "remove" => self.remove_job(params).await,
            _ => Ok(format!("Unknown action: {action}")),
        }
    }
}

impl CronTool {
    fn parse_at_ms(raw: &str, default_timezone: Option<&str>) -> Result<i64> {
        if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
            return Ok(dt.timestamp_millis());
        }

        let timezone = parse_timezone(default_timezone).ok_or_else(|| {
            anyhow!(
                "invalid at datetime: unknown timezone '{}'",
                default_timezone.unwrap_or("UTC")
            )
        })?;
        let parse_local = |fmt: &str| -> Option<i64> {
            let naive = NaiveDateTime::parse_from_str(raw, fmt).ok()?;
            let local = timezone.from_local_datetime(&naive).single()?;
            Some(local.timestamp_millis())
        };

        parse_local("%Y-%m-%dT%H:%M:%S")
            .or_else(|| parse_local("%Y-%m-%d %H:%M:%S"))
            .ok_or_else(|| anyhow!("invalid at datetime: expected ISO datetime string"))
    }

    async fn add_job(&self, params: &Map<String, Value>) -> Result<String> {
        if Self::in_cron_execution_context() {
            return Ok("Error: cannot schedule new jobs from within a cron job execution".into());
        }

        let message = params
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if message.is_empty() {
            return Ok("Error: message is required for add".to_string());
        }

        let (channel, chat_id) = {
            let guard = self
                .context
                .lock()
                .map_err(|_| anyhow!("failed to lock cron context"))?;
            (guard.channel.clone(), guard.chat_id.clone())
        };
        if channel.is_empty() || chat_id.is_empty() {
            return Ok("Error: no session context (channel/chat_id)".to_string());
        }

        let every_seconds = params.get("every_seconds").and_then(Value::as_i64);
        let cron_expr = params.get("cron_expr").and_then(Value::as_str);
        let tz = params.get("tz").and_then(Value::as_str);
        let at = params.get("at").and_then(Value::as_str);
        if tz.is_some() && cron_expr.is_none() {
            return Ok("Error: tz can only be used with cron_expr".to_string());
        }
        let mut delete_after_run = false;
        let schedule = if let Some(seconds) = every_seconds {
            CronSchedule {
                kind: "every".to_string(),
                every_ms: Some(seconds * 1000),
                ..Default::default()
            }
        } else if let Some(expr) = cron_expr {
            let effective_tz = tz.unwrap_or(self.default_timezone.as_str());
            if let Some(err) = self.validate_timezone(effective_tz) {
                return Ok(err);
            }
            CronSchedule {
                kind: "cron".to_string(),
                expr: Some(expr.to_string()),
                tz: Some(effective_tz.to_string()),
                ..Default::default()
            }
        } else if let Some(at_raw) = at {
            let at_ms = Self::parse_at_ms(at_raw, Some(&self.default_timezone))?;
            delete_after_run = true;
            CronSchedule {
                kind: "at".to_string(),
                at_ms: Some(at_ms),
                ..Default::default()
            }
        } else {
            return Ok("Error: either every_seconds, cron_expr, or at is required".to_string());
        };

        let job = self
            .cron
            .add_job(
                message.chars().take(30).collect::<String>(),
                schedule,
                message,
                true,
                Some(channel),
                Some(chat_id),
                delete_after_run,
            )
            .await?;
        Ok(format!("Created job '{}' (id: {})", job.name, job.id))
    }

    async fn list_jobs(&self) -> Result<String> {
        let jobs = self.cron.list_jobs(false).await;
        if jobs.is_empty() {
            return Ok("No scheduled jobs.".to_string());
        }
        let lines = jobs
            .iter()
            .map(|job| {
                let mut parts = vec![format!(
                    "- {} (id: {}, {})",
                    job.name,
                    job.id,
                    self.format_timing(&job.schedule)
                )];
                parts.extend(self.format_state(&job.state, &job.schedule));
                parts.join("\n")
            })
            .collect::<Vec<_>>();
        Ok(format!("Scheduled jobs:\n{}", lines.join("\n")))
    }

    async fn remove_job(&self, params: &Map<String, Value>) -> Result<String> {
        let Some(job_id) = params.get("job_id").and_then(Value::as_str) else {
            return Ok("Error: job_id is required for remove".to_string());
        };
        if self.cron.remove_job(job_id).await? {
            Ok(format!("Removed job {job_id}"))
        } else {
            Ok(format!("Job {job_id} not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CronTool;
    use crate::cron::{CronSchedule, CronService};
    use crate::tools::base::Tool;
    use serde_json::{Map, Value};
    use std::sync::Arc;
    use uuid::Uuid;

    #[test]
    fn parse_at_ms_accepts_rfc3339() {
        let ts =
            CronTool::parse_at_ms("2026-02-12T10:30:00+08:00", Some("UTC")).expect("timestamp");
        assert!(ts > 0);
    }

    #[test]
    fn parse_at_ms_rejects_invalid() {
        let err = CronTool::parse_at_ms("not-a-time", Some("UTC")).expect_err("should fail");
        assert!(err.to_string().contains("invalid at datetime"));
    }

    #[test]
    fn parse_at_ms_uses_default_timezone_for_naive_datetime() {
        let ts =
            CronTool::parse_at_ms("1970-01-01T08:00:00", Some("Asia/Shanghai")).expect("timestamp");
        assert_eq!(ts, 0);
    }

    #[tokio::test]
    async fn add_is_blocked_inside_cron_execution_guard() {
        let store_path =
            std::env::temp_dir().join(format!("nanobot-rs-cron-tool-{}.json", Uuid::new_v4()));
        let cron = Arc::new(CronService::new(store_path));
        let tool = CronTool::new(cron, "UTC");
        tool.set_context("cli", "direct");

        let mut params = Map::new();
        params.insert("action".to_string(), Value::String("add".to_string()));
        params.insert("message".to_string(), Value::String("test".to_string()));
        params.insert("every_seconds".to_string(), Value::from(60));

        let response = CronTool::with_cron_execution_guard(tool.execute(&params))
            .await
            .expect("cron tool result");
        assert_eq!(
            response,
            "Error: cannot schedule new jobs from within a cron job execution"
        );
    }

    #[tokio::test]
    async fn add_cron_job_defaults_to_tool_timezone() {
        let store_path =
            std::env::temp_dir().join(format!("nanobot-rs-cron-tool-{}.json", Uuid::new_v4()));
        let cron = Arc::new(CronService::new(store_path));
        let tool = CronTool::new(cron.clone(), "Asia/Shanghai");
        tool.set_context("cli", "direct");

        let mut params = Map::new();
        params.insert("action".to_string(), Value::String("add".to_string()));
        params.insert("message".to_string(), Value::String("test".to_string()));
        params.insert(
            "cron_expr".to_string(),
            Value::String("0 0 8 * * ?".to_string()),
        );

        let response = tool.execute(&params).await.expect("cron tool result");
        assert!(response.starts_with("Created job"));
        let jobs = cron.list_jobs(true).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].schedule.tz.as_deref(), Some("Asia/Shanghai"));
    }

    #[tokio::test]
    async fn list_jobs_displays_default_timezone_for_at_schedule() {
        let store_path =
            std::env::temp_dir().join(format!("nanobot-rs-cron-tool-{}.json", Uuid::new_v4()));
        let cron = Arc::new(CronService::new(store_path));
        let tool = CronTool::new(cron.clone(), "Asia/Shanghai");
        cron.add_job(
            "oneshot".to_string(),
            CronSchedule {
                kind: "at".to_string(),
                at_ms: Some(0),
                ..Default::default()
            },
            "ping".to_string(),
            false,
            None,
            None,
            true,
        )
        .await
        .expect("job");

        let rendered = tool.list_jobs().await.expect("list");
        assert!(rendered.contains("Asia/Shanghai"));
        assert!(rendered.contains("1970-01-01T08:00:00"));
    }
}
