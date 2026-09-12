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
        // cron crate uses a seconds field; prefix zero after validating the user-facing five fields.
        Schedule::from_str(&format!("0 {expression}"))
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
        let schedule = Schedule::from_str(&format!("0 {}", self.expression))
            .map_err(|error| format!("invalid cron expression: {error}"))?;
        schedule
            .after(&local_now)
            .next()
            .map(|value| value.with_timezone(&Utc))
            .ok_or_else(|| "cron expression has no future occurrence".to_string())
    }
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
}
