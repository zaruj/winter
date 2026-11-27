//! Cost overview from Cost Explorer: this month so far against last month, the month-end
//! forecast, daily spend and the services that cost the most.
//!
//! Cost Explorer charges for each request (USD 0.01 at the time of writing), so one overview is
//! three requests and is only fetched when asked for. Dates are UTC days, as Cost Explorer uses.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_costexplorer::types::{
    DateInterval, Granularity, GroupDefinition, GroupDefinitionType, Metric,
};
use serde::Serialize;

use crate::Result;

/// Cost Explorer's own metric name; the console's default.
const METRIC: &str = "UnblendedCost";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostOverview {
    /// e.g. `USD`.
    pub currency: String,
    /// First day of this month, `YYYY-MM-DD`.
    pub month_start: String,
    /// Today (UTC), `YYYY-MM-DD`.
    pub today: String,
    pub month_to_date: f64,
    /// Last month up to the same day of the month, to compare with `month_to_date`.
    pub last_month_to_date: f64,
    pub last_month: f64,
    /// Month-end estimate: spend up to yesterday plus Cost Explorer's forecast for the rest.
    /// None when it has too little history to forecast.
    pub forecast: Option<f64>,
    /// One per day from the start of last month to today.
    pub daily: Vec<DailyCost>,
    /// Most expensive this month first.
    pub services: Vec<ServiceCost>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCost {
    pub date: String,
    pub amount: f64,
    /// The day is not over, or its figures are not final yet.
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCost {
    pub name: String,
    pub this_month: f64,
    pub last_month: f64,
}

/// A day as days since 1970-01-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Day(i64);

impl Day {
    pub(super) fn today() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Self(secs.div_euclid(86_400))
    }

    /// From a calendar date (proleptic Gregorian).
    fn from_ymd(y: i64, m: u32, d: u32) -> Self {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (i64::from(m) + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Self(era * 146_097 + doe - 719_468)
    }

    fn ymd(self) -> (i64, u32, u32) {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        (
            if m <= 2 {
                yoe + era * 400 + 1
            } else {
                yoe + era * 400
            },
            m,
            d,
        )
    }

    fn month_start(self) -> Self {
        let (y, m, _) = self.ymd();
        Self::from_ymd(y, m, 1)
    }

    fn next_month(self) -> Self {
        let (y, m, _) = self.ymd();
        if m == 12 {
            Self::from_ymd(y + 1, 1, 1)
        } else {
            Self::from_ymd(y, m + 1, 1)
        }
    }

    fn previous_month(self) -> Self {
        let (y, m, _) = self.ymd();
        if m == 1 {
            Self::from_ymd(y - 1, 12, 1)
        } else {
            Self::from_ymd(y, m - 1, 1)
        }
    }

    fn plus(self, days: i64) -> Self {
        Self(self.0 + days)
    }

    fn parse(s: &str) -> Option<Self> {
        let mut parts = s.splitn(3, '-');
        let y = parts.next()?.parse().ok()?;
        let m = parts.next()?.parse().ok()?;
        let d = parts.next()?.parse().ok()?;
        Some(Self::from_ymd(y, m, d))
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

/// The days the overview covers, worked out from today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Period {
    pub today: Day,
    pub month_start: Day,
    pub last_month_start: Day,
    /// The same day of last month as today (or last month's last day), exclusive end of the
    /// period to compare with this month so far.
    pub last_month_same_day_end: Day,
    pub next_month_start: Day,
}

impl Period {
    pub(super) fn new(today: Day) -> Self {
        let month_start = today.month_start();
        let last_month_start = month_start.previous_month();
        let elapsed = today.0 - month_start.0; // days before today in this month
        let same_day_end = last_month_start.plus(elapsed + 1).min(month_start);
        Self {
            today,
            month_start,
            last_month_start,
            last_month_same_day_end: same_day_end,
            next_month_start: today.next_month(),
        }
    }
}

fn interval(start: Day, end: Day) -> DateInterval {
    DateInterval::builder()
        .start(start.to_string())
        .end(end.to_string())
        .build()
        .expect("start and end are set")
}

fn amount(
    metrics: Option<&std::collections::HashMap<String, aws_sdk_costexplorer::types::MetricValue>>,
) -> (f64, Option<String>) {
    let value = metrics.and_then(|m| m.get(METRIC));
    (
        value
            .and_then(|v| v.amount())
            .and_then(|a| a.parse().ok())
            .unwrap_or(0.0),
        value.and_then(|v| v.unit()).map(str::to_owned),
    )
}

pub(super) async fn overview(
    ce: &aws_sdk_costexplorer::Client,
    period: Period,
) -> Result<CostOverview> {
    let tomorrow = period.today.plus(1);
    let daily = async {
        ce.get_cost_and_usage()
            .time_period(interval(period.last_month_start, tomorrow))
            .granularity(Granularity::Daily)
            .metrics(METRIC)
            .send()
            .await
            .map_err(super::aws_error)
    };
    let by_service = async {
        let mut results = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let out = ce
                .get_cost_and_usage()
                .time_period(interval(period.last_month_start, tomorrow))
                .granularity(Granularity::Monthly)
                .metrics(METRIC)
                .group_by(
                    GroupDefinition::builder()
                        .r#type(GroupDefinitionType::Dimension)
                        .key("SERVICE")
                        .build(),
                )
                .set_next_page_token(token.take())
                .send()
                .await
                .map_err(super::aws_error)?;
            results.extend(out.results_by_time().iter().cloned());
            match out.next_page_token() {
                Some(t) if !t.is_empty() => token = Some(t.to_owned()),
                _ => break,
            }
        }
        Ok::<_, crate::Error>(results)
    };
    // Forecasts start today at the earliest; best effort, as new accounts have no history.
    let forecast = async {
        ce.get_cost_forecast()
            .time_period(interval(period.today, period.next_month_start))
            .granularity(Granularity::Monthly)
            .metric(Metric::UnblendedCost)
            .send()
            .await
            .ok()?
            .total()?
            .amount()?
            .parse::<f64>()
            .ok()
    };
    let (daily, by_service, forecast) = tokio::join!(daily, by_service, forecast);
    let (daily, by_service) = (daily?, by_service?);

    let mut currency = None;
    let days: Vec<(Day, DailyCost)> = daily
        .results_by_time()
        .iter()
        .filter_map(|r| {
            let start = Day::parse(r.time_period()?.start())?;
            let (amount, unit) = amount(r.total());
            currency = currency.take().or(unit);
            Some((
                start,
                DailyCost {
                    date: start.to_string(),
                    amount,
                    estimated: r.estimated() || start == period.today,
                },
            ))
        })
        .collect();
    let sum = |from: Day, to: Day| -> f64 {
        days.iter()
            .filter(|(d, _)| *d >= from && *d < to)
            .map(|(_, c)| c.amount)
            .sum()
    };

    let mut services: BTreeMap<String, ServiceCost> = BTreeMap::new();
    for result in &by_service {
        let this_month = result
            .time_period()
            .and_then(|p| Day::parse(p.start()))
            .is_some_and(|d| d >= period.month_start);
        for group in result.groups() {
            let Some(name) = group.keys().first() else {
                continue;
            };
            let (value, unit) = amount(group.metrics());
            currency = currency.take().or(unit);
            let entry = services.entry(name.clone()).or_insert_with(|| ServiceCost {
                name: name.clone(),
                this_month: 0.0,
                last_month: 0.0,
            });
            if this_month {
                entry.this_month += value;
            } else {
                entry.last_month += value;
            }
        }
    }
    let mut services: Vec<ServiceCost> = services
        .into_values()
        .filter(|s| s.this_month.abs() >= 0.005 || s.last_month.abs() >= 0.005)
        .collect();
    services.sort_by(|a, b| {
        b.this_month
            .total_cmp(&a.this_month)
            .then(b.last_month.total_cmp(&a.last_month))
    });

    let until_yesterday = sum(period.month_start, period.today);
    Ok(CostOverview {
        currency: currency.unwrap_or_else(|| "USD".into()),
        month_start: period.month_start.to_string(),
        today: period.today.to_string(),
        month_to_date: sum(period.month_start, tomorrow),
        last_month_to_date: sum(period.last_month_start, period.last_month_same_day_end),
        last_month: sum(period.last_month_start, period.month_start),
        forecast: forecast.map(|rest| until_yesterday + rest),
        daily: days.into_iter().map(|(_, c)| c).collect(),
        services,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_round_trip_through_dates() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2024, 12, 31), (2026, 3, 1)] {
            let day = Day::from_ymd(y, m, d);
            assert_eq!(day.ymd(), (y, m, d));
            assert_eq!(Day::parse(&day.to_string()), Some(day));
        }
        assert_eq!(Day::from_ymd(1970, 1, 1), Day(0));
        assert_eq!(Day::from_ymd(2026, 11, 27).to_string(), "2026-11-27");
    }

    #[test]
    fn period_compares_the_same_days_of_last_month() {
        let p = Period::new(Day::from_ymd(2026, 3, 31));
        assert_eq!(p.month_start.to_string(), "2026-03-01");
        assert_eq!(p.last_month_start.to_string(), "2026-02-01");
        // February has 28 days, so the comparison stops at its end.
        assert_eq!(p.last_month_same_day_end.to_string(), "2026-03-01");
        assert_eq!(p.next_month_start.to_string(), "2026-04-01");

        let p = Period::new(Day::from_ymd(2026, 1, 10));
        assert_eq!(p.last_month_start.to_string(), "2025-12-01");
        assert_eq!(p.last_month_same_day_end.to_string(), "2025-12-11");
        assert_eq!(p.next_month_start.to_string(), "2026-02-01");
    }
}
