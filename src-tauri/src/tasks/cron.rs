use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronPlan {
    pub expression: String,
    pub timezone: String,
}

impl CronPlan {
    pub fn validate(expression: &str, timezone: &str) -> Result<Self, String> {
        let expression = expression.trim();
        if expression.split_whitespace().count() != 5 {
            return Err("cron expression must contain exactly five fields".to_string());
        }
        let normalized = normalize_expression(expression)?;
        // cron crate uses a seconds field; prefix zero after validating the user-facing five fields.
        Schedule::from_str(&format!("0 {normalized}"))
            .map_err(|error| format!("invalid cron expression: {error}"))?;
        timezone
            .parse::<Tz>()
            .map_err(|_| format!("unknown IANA timezone: {timezone}"))?;
        Ok(Self {
            expression: expression.to_string(),
            timezone: timezone.to_string(),
        })
    }

    pub fn next_after(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
        let timezone = self
            .timezone
            .parse::<Tz>()
            .map_err(|_| format!("unknown IANA timezone: {}", self.timezone))?;
        let local_now = now.with_timezone(&timezone);
        let fields: Vec<&str> = self.expression.split_whitespace().collect();
        let normalized = normalize_expression(&self.expression)?;
        let mut expressions = vec![normalized.clone()];
        if fields[2] != "*" && fields[4] != "*" {
            let mut dom_only = fields.clone();
            dom_only[4] = "*";
            let mut dow_only = fields.clone();
            dow_only[2] = "*";
            expressions = vec![
                normalize_expression(&dom_only.join(" "))?,
                normalize_expression(&dow_only.join(" "))?,
            ];
        }
        expressions
            .into_iter()
            .filter_map(|expression| {
                Schedule::from_str(&format!("0 {expression}"))
                    .map_err(|error| format!("invalid cron expression: {error}"))
                    .ok()
                    .and_then(|schedule| schedule.after(&local_now).next())
            })
            .min()
            .map(|value| value.with_timezone(&Utc))
            .ok_or_else(|| "cron expression has no future occurrence".to_string())
    }
}

fn normalize_expression(expression: &str) -> Result<String, String> {
    let mut fields: Vec<String> = expression.split_whitespace().map(str::to_string).collect();
    if fields.len() != 5 {
        return Err("cron expression must contain exactly five fields".to_string());
    }
    fields[4] = fields[4]
        .split(',')
        .map(normalize_weekday_part)
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    Ok(fields.join(" "))
}

fn normalize_weekday_part(part: &str) -> Result<String, String> {
    let mut pieces = part.split('/');
    let range = pieces.next().unwrap_or_default();
    let step = pieces.next();
    if pieces.next().is_some() {
        return Err("cron weekday step is malformed".to_string());
    }
    let normalized_range = range
        .split('-')
        .map(|value| {
            if value == "*" {
                return Ok(value.to_string());
            }
            let number = value
                .parse::<u8>()
                .map_err(|_| format!("invalid numeric weekday: {value}"))?;
            if number > 7 {
                return Err(format!("weekday must be between 0 and 7: {number}"));
            }
            Ok((if number == 0 || number == 7 {
                1
            } else {
                number + 1
            })
            .to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("-");
    Ok(match step {
        Some(step) => format!("{normalized_range}/{step}"),
        None => normalized_range,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn computes_strictly_future_utc_occurrences_in_iana_timezone() {
        let plan = CronPlan::validate("0 9 * * *", "Asia/Shanghai").unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 1, 0, 0).unwrap();
        assert_eq!(
            plan.next_after(now).unwrap().to_rfc3339(),
            "2026-09-12T01:00:00+00:00"
        );
        assert!(CronPlan::validate("0 9 * *", "UTC").is_err());
        assert!(CronPlan::validate("0 9 * * *", "Not/AZone").is_err());
    }

    #[test]
    fn uses_unix_weekday_numbers_and_dom_dow_or_semantics() {
        let weekdays = CronPlan::validate("0 9 * * 1-5", "Asia/Shanghai").unwrap();
        let friday = Utc.with_ymd_and_hms(2026, 9, 11, 1, 0, 0).unwrap();
        assert_eq!(
            weekdays.next_after(friday).unwrap().to_rfc3339(),
            "2026-09-14T01:00:00+00:00"
        );

        let either = CronPlan::validate("0 9 1 * 1", "Asia/Shanghai").unwrap();
        let june = Utc.with_ymd_and_hms(2026, 6, 7, 2, 0, 0).unwrap();
        assert_eq!(
            either.next_after(june).unwrap().to_rfc3339(),
            "2026-06-08T01:00:00+00:00"
        );
    }
}
