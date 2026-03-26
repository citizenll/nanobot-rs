use chrono::{Local, TimeZone, Utc};
use chrono_tz::Tz;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn ensure_dir(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(path)?;
    Ok(path.to_path_buf())
}

pub fn get_data_path() -> std::io::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| std::io::Error::other("cannot resolve home directory"))?;
    ensure_dir(&home.join(".nanobot"))
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

pub fn get_workspace_path(workspace: Option<&str>) -> std::io::Result<PathBuf> {
    let path = match workspace {
        Some(p) => expand_tilde(p),
        None => {
            let home = dirs::home_dir()
                .ok_or_else(|| std::io::Error::other("cannot resolve home directory"))?;
            home.join(".nanobot").join("workspace")
        }
    };
    ensure_dir(&path)
}

pub fn today_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn timestamp() -> String {
    Local::now().to_rfc3339()
}

pub fn normalize_timezone_value(timezone: Option<&str>) -> Option<String> {
    timezone
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn parse_timezone(timezone: Option<&str>) -> Option<Tz> {
    let timezone = normalize_timezone_value(timezone)?;
    Tz::from_str(&timezone).ok()
}

pub fn current_time_str(timezone: Option<&str>) -> String {
    if let Some(timezone_name) = normalize_timezone_value(timezone)
        && let Ok(tz) = Tz::from_str(&timezone_name)
    {
        let now = Utc::now().with_timezone(&tz);
        let offset = now.format("%z").to_string();
        let offset_fmt = if offset.len() == 5 {
            format!("{}:{}", &offset[..3], &offset[3..])
        } else {
            offset
        };
        return format!(
            "{} ({}, UTC{})",
            now.format("%Y-%m-%d %H:%M (%A)"),
            timezone_name,
            offset_fmt
        );
    }

    let now = Local::now();
    let offset = now.format("%z").to_string();
    let offset_fmt = if offset.len() == 5 {
        format!("{}:{}", &offset[..3], &offset[3..])
    } else {
        offset
    };
    let timezone_name = match now.format("%Z").to_string() {
        value if value.trim().is_empty() => "UTC".to_string(),
        value => value,
    };
    format!(
        "{} ({}, UTC{})",
        now.format("%Y-%m-%d %H:%M (%A)"),
        timezone_name,
        offset_fmt
    )
}

pub fn format_timestamp_ms(ms: i64, timezone: Option<&str>) -> String {
    if let Some(timezone_name) = normalize_timezone_value(timezone)
        && let Ok(tz) = Tz::from_str(&timezone_name)
        && let Some(dt) = Utc.timestamp_millis_opt(ms).single()
    {
        return format!("{} ({})", dt.with_timezone(&tz).to_rfc3339(), timezone_name);
    }

    if let Some(dt) = Local.timestamp_millis_opt(ms).single() {
        let timezone_name = match dt.format("%Z").to_string() {
            value if value.trim().is_empty() => "UTC".to_string(),
            value => value,
        };
        return format!("{} ({})", dt.to_rfc3339(), timezone_name);
    }

    ms.to_string()
}

pub fn safe_filename(name: &str) -> String {
    let mut out = name.to_string();
    for ch in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
        out = out.replace(ch, "_");
    }
    out.trim().to_string()
}

pub fn parse_session_key(key: &str) -> anyhow::Result<(&str, &str)> {
    let (channel, chat_id) = key
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid session key: {key}"))?;
    Ok((channel, chat_id))
}

#[cfg(test)]
mod tests {
    use super::{current_time_str, format_timestamp_ms, normalize_timezone_value, parse_timezone};

    #[test]
    fn normalize_timezone_discards_empty_values() {
        assert_eq!(
            normalize_timezone_value(Some("Asia/Shanghai")),
            Some("Asia/Shanghai".into())
        );
        assert_eq!(normalize_timezone_value(Some("   ")), None);
        assert_eq!(normalize_timezone_value(None), None);
    }

    #[test]
    fn parse_timezone_accepts_iana_names() {
        assert!(parse_timezone(Some("Asia/Shanghai")).is_some());
        assert!(parse_timezone(Some("Not/A-Timezone")).is_none());
    }

    #[test]
    fn current_time_string_includes_configured_timezone() {
        let rendered = current_time_str(Some("Asia/Shanghai"));
        assert!(rendered.contains("Asia/Shanghai"));
        assert!(rendered.contains("UTC+08:00"));
    }

    #[test]
    fn format_timestamp_uses_requested_timezone() {
        let rendered = format_timestamp_ms(0, Some("Asia/Shanghai"));
        assert!(rendered.starts_with("1970-01-01T08:00:00"));
        assert!(rendered.contains("(Asia/Shanghai)"));
    }
}
