use chrono::NaiveDate;

use crate::db::LlmCallFilters;

pub fn validate_date_range(from: &str, to: &str) -> Result<(), String> {
    let from_date = parse_date("from", from)?;
    let to_date = parse_date("to", to)?;
    if from_date > to_date {
        return Err("from date must be before or equal to to date".to_string());
    }

    Ok(())
}

pub fn validate_call_filters_date_range(filters: &LlmCallFilters) -> Result<(), String> {
    let from = parse_optional_date("from", filters.from.as_deref())?;
    let to = parse_optional_date("to", filters.to.as_deref())?;
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err("from date must be before or equal to to date".to_string());
        }
    }

    Ok(())
}

fn parse_date(name: &str, value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("invalid {name} date: {value}"))
}

fn parse_optional_date(name: &str, value: Option<&str>) -> Result<Option<NaiveDate>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    parse_date(name, value).map(Some)
}

#[cfg(test)]
mod tests {
    use crate::db::LlmCallFilters;

    #[test]
    fn validate_date_range_accepts_same_day_window() {
        assert!(super::validate_date_range("2026-06-10", "2026-06-10").is_ok());
    }

    #[test]
    fn validate_date_range_rejects_reversed_window() {
        let error = super::validate_date_range("2026-06-11", "2026-06-10")
            .expect_err("reversed range should be rejected");

        assert_eq!(error, "from date must be before or equal to to date");
    }

    #[test]
    fn validate_date_range_rejects_invalid_dates() {
        assert_eq!(
            super::validate_date_range("2026-06-31", "2026-07-01")
                .expect_err("invalid from date should be rejected"),
            "invalid from date: 2026-06-31"
        );
        assert_eq!(
            super::validate_date_range("2026-06-10", "not-a-date")
                .expect_err("invalid to date should be rejected"),
            "invalid to date: not-a-date"
        );
    }

    #[test]
    fn validate_call_filters_date_range_allows_open_ended_windows() {
        assert!(
            super::validate_call_filters_date_range(&filters(Some("2026-06-10"), None)).is_ok()
        );
        assert!(
            super::validate_call_filters_date_range(&filters(None, Some("2026-06-10"))).is_ok()
        );
    }

    #[test]
    fn validate_call_filters_date_range_rejects_reversed_window() {
        let error = super::validate_call_filters_date_range(&filters(
            Some("2026-06-11"),
            Some("2026-06-10"),
        ))
        .expect_err("reversed filter range should be rejected");

        assert_eq!(error, "from date must be before or equal to to date");
    }

    fn filters(from: Option<&str>, to: Option<&str>) -> LlmCallFilters {
        LlmCallFilters {
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            provider: None,
            agent_id: None,
            model: None,
            status: None,
            workflow_id: None,
            project_id: None,
            session_id: None,
            limit: 100,
            offset: 0,
        }
    }
}
