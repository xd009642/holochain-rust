/// Provides statistical features over metric data.
use crate::Metric;
use num_traits::float::Float;
/// Extends the metric api with statistical aggregation functions
use stats::{Commute, OnlineStats};
use std::{
    collections::HashMap,
    fmt,
    fmt::{Display, Formatter},
    iter::FromIterator,
};

/// An extension of `OnlineStats` that also incrementally tracks
/// max and min values.
#[derive(Debug, Clone)]
pub struct DescriptiveStats {
    online_stats: OnlineStats,
    max: f64,
    min: f64,
    cnt: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatsRecord {
    pub name: Option<String>,
    pub max: f64,
    pub min: f64,
    pub cnt: u64,
    pub mean: f64,
    pub variance: f64,
    pub stddev: f64,
}

#[derive(Shrinkwrap, Clone)]
pub struct AllValuesLessThan(DescriptiveStats);

impl StatsRecord {
    pub fn new<S: Into<String>>(metric_name: S, desc: DescriptiveStats) -> Self {
        let metric_name = metric_name.into();
        let mut record: Self = desc.into();
        record.name = Some(metric_name);
        record
    }
}

impl From<DescriptiveStats> for StatsRecord {
    fn from(desc_stats: DescriptiveStats) -> Self {
        Self {
            name: None,
            max: desc_stats.max(),
            min: desc_stats.min(),
            stddev: desc_stats.stddev(),
            mean: desc_stats.mean(),
            variance: desc_stats.variance(),
            cnt: desc_stats.count(),
        }
    }
}

impl Copy for DescriptiveStats {}

#[derive(Clone, Debug, Serialize)]
pub enum DescriptiveStatType {
    Mean,
    Max,
    Min,
    StdDev,
    Count,
}

impl Display for DescriptiveStatType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct StatFailure {
    expected: f64,
    actual: f64,
    stat_type: DescriptiveStatType,
}

impl std::fmt::Display for StatFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}: Expected {}, Actual was {}",
            self.stat_type, self.expected, self.actual
        )
    }
}

impl DescriptiveStats {
    /// An initial empty statistic.
    pub fn empty() -> Self {
        Self {
            online_stats: OnlineStats::new(),
            max: f64::min_value(),
            min: f64::max_value(),
            cnt: 0,
        }
    }

    /// Adds a value to the running statistic.
    pub fn add(&mut self, value: f64) {
        self.online_stats.add(value);
        if value > self.max {
            self.max = value
        }
        if value < self.min {
            self.min = value
        }
        self.cnt += 1;
    }

    /// The mean value of the running statistic.
    pub fn mean(&self) -> f64 {
        self.online_stats.mean()
    }

    /// The standard deviation of the running statistic.
    pub fn stddev(&self) -> f64 {
        self.online_stats.stddev()
    }

    /// The variance of the running statistic.
    pub fn variance(&self) -> f64 {
        self.online_stats.variance()
    }

    /// The max of the running statistic.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// The min of the running statistic.
    pub fn min(&self) -> f64 {
        self.min
    }

    /// The number of samples of the running statistic.
    pub fn count(&self) -> u64 {
        self.cnt
    }
}

pub trait StatCheck {
    fn check(
        &self,
        expected: &DescriptiveStats,
        actual: &DescriptiveStats,
    ) -> Result<DescriptiveStats, Vec<StatFailure>>;
}

#[derive(Clone, Debug)]
pub struct LessThanStatCheck;

impl StatCheck for LessThanStatCheck {
    fn check(
        &self,
        expected: &DescriptiveStats,
        actual: &DescriptiveStats,
    ) -> Result<DescriptiveStats, Vec<StatFailure>> {
        let mut failures = Vec::new();

        if actual.mean() > expected.mean() {
            failures.push(StatFailure {
                expected: expected.mean(),
                actual: actual.mean(),
                stat_type: DescriptiveStatType::Mean,
            })
        }

        if actual.stddev() > expected.stddev() {
            failures.push(StatFailure {
                expected: expected.stddev(),
                actual: actual.stddev(),
                stat_type: DescriptiveStatType::StdDev,
            })
        }

        if actual.max() > expected.max() {
            failures.push(StatFailure {
                expected: expected.max(),
                actual: actual.max(),
                stat_type: DescriptiveStatType::Max,
            })
        }

        if actual.min() > expected.min() {
            failures.push(StatFailure {
                expected: expected.min(),
                actual: actual.min(),
                stat_type: DescriptiveStatType::Min,
            })
        }

        if actual.count() > expected.count() {
            failures.push(StatFailure {
                expected: expected.count() as f64,
                actual: actual.count() as f64,
                stat_type: DescriptiveStatType::Count,
            })
        }

        if failures.is_empty() {
            Ok(*actual)
        } else {
            Err(failures)
        }
    }
}

impl dyn StatCheck {
    pub fn check_all(
        &self,
        expected: &StatsByMetric,
        actual: &StatsByMetric,
    ) -> HashMap<String, Result<DescriptiveStats, Vec<StatFailure>>> {
        HashMap::from_iter(expected.iter().map(|(stat_name, expected_stat)| {
            if let Some(actual_stat) = actual.get(stat_name) {
                (stat_name.clone(), self.check(expected_stat, actual_stat))
            } else {
                (stat_name.clone(), Err(vec![]))
            }
        }))
    }
}

impl Commute for DescriptiveStats {
    fn merge(&mut self, rhs: Self) {
        self.online_stats.merge(rhs.online_stats);
        if rhs.max > self.max {
            self.max = rhs.max
        }
        if rhs.min < self.min {
            self.min = rhs.min
        }
        self.cnt += rhs.cnt;
    }
}

/// All combined descriptive statistics mapped by name of the metric
#[derive(Shrinkwrap, Debug, Clone)]
pub struct StatsByMetric(pub HashMap<String, DescriptiveStats>);

impl StatsByMetric {
    pub fn to_records(&self) -> Box<dyn Iterator<Item = StatsRecord>> {
        let me = self.0.clone();
        Box::new(
            me.into_iter()
                .map(|(name, stat)| StatsRecord::new(name, stat)),
        )
    }

    pub fn print_csv(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut writer = csv::Writer::from_writer(std::io::stdout());
        let records = self.to_records();
        for record in records {
            writer.serialize(record)?;
        }
        writer.flush()?;
        Ok(())
    }
}

impl FromIterator<Metric> for StatsByMetric {
    fn from_iter<I: IntoIterator<Item = Metric>>(source: I) -> StatsByMetric {
        StatsByMetric(source.into_iter().fold(
            HashMap::new(),
            |mut stats_by_metric_name, metric| {
                let entry = stats_by_metric_name.entry(metric.name);

                let online_stats = entry.or_insert_with(DescriptiveStats::empty);
                online_stats.add(metric.value);
                stats_by_metric_name
            },
        ))
    }
}

impl Commute for StatsByMetric {
    fn merge(&mut self, rhs: Self) {
        for (metric_name, online_stats_rhs) in rhs.iter() {
            let entry = self.0.entry(metric_name.to_string());
            let online_stats = entry.or_insert_with(DescriptiveStats::empty);
            online_stats.merge(*online_stats_rhs);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn can_aggregate_stats_from_iterator() {
        let latency_data = vec![50.0, 100.0, 150.0]
            .into_iter()
            .map(|x| Metric::new("latency", x));
        let size_data = vec![1.0, 10.0, 100.0]
            .into_iter()
            .map(|x| Metric::new("size", x));
        let all_data = latency_data.chain(size_data);
        let stats = StatsByMetric::from_iter(all_data);

        let latency_stats = stats.get("latency").expect("latency stats to be present");

        assert_eq!(latency_stats.mean(), 100.0);
        let size_stats = stats.get("size").expect("size stats to be present");

        assert_eq!(size_stats.mean(), 37.0);

        assert_eq!(latency_stats.min(), 50.0);
        assert_eq!(latency_stats.max(), 150.0);

        assert_eq!(size_stats.min(), 1.0);
        assert_eq!(size_stats.max(), 100.0);
    }
}
