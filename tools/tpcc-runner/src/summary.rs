use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::module_bindings::DeliveryCompletionView;

const HISTOGRAM_BUCKETS_MS: [u64; 16] = [
    1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 60_000, 120_000,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TransactionKind {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

impl TransactionKind {
    pub const ALL: [Self; 5] = [
        Self::NewOrder,
        Self::Payment,
        Self::OrderStatus,
        Self::Delivery,
        Self::StockLevel,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewOrder => "new_order",
            Self::Payment => "payment",
            Self::OrderStatus => "order_status",
            Self::Delivery => "delivery",
            Self::StockLevel => "stock_level",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Histogram {
    pub buckets_ms: Vec<u64>,
    pub counts: Vec<u64>,
    pub count: u64,
    pub sum_ms: u64,
    pub max_ms: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            buckets_ms: HISTOGRAM_BUCKETS_MS.to_vec(),
            counts: vec![0; HISTOGRAM_BUCKETS_MS.len() + 1],
            count: 0,
            sum_ms: 0,
            max_ms: 0,
        }
    }
}

impl Histogram {
    pub fn record(&mut self, value_ms: u64) {
        let index = self
            .buckets_ms
            .iter()
            .position(|upper| value_ms <= *upper)
            .unwrap_or(self.buckets_ms.len());
        self.counts[index] += 1;
        self.count += 1;
        self.sum_ms += value_ms;
        self.max_ms = self.max_ms.max(value_ms);
    }

    pub fn merge(&mut self, other: &Histogram) {
        self.count += other.count;
        self.sum_ms += other.sum_ms;
        self.max_ms = self.max_ms.max(other.max_ms);
        for (left, right) in self.counts.iter_mut().zip(&other.counts) {
            *left += right;
        }
    }

    pub fn mean_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum_ms as f64 / self.count as f64
        }
    }

    pub fn percentile_ms(&self, pct: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let wanted = ((self.count as f64) * pct).ceil() as u64;
        let mut seen = 0u64;
        for (idx, count) in self.counts.iter().enumerate() {
            seen += *count;
            if seen >= wanted {
                return if idx < self.buckets_ms.len() {
                    self.buckets_ms[idx]
                } else {
                    self.max_ms
                };
            }
        }
        self.max_ms
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionSummary {
    pub count: u64,
    pub success: u64,
    pub failure: u64,
    pub mean_latency_ms: f64,
    pub p50_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
    pub max_latency_ms: u64,
    pub histogram: Histogram,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConformanceSummary {
    pub new_order_rollbacks: u64,
    pub new_order_total: u64,
    pub new_order_remote_order_lines: u64,
    pub new_order_total_order_lines: u64,
    pub payment_remote: u64,
    pub payment_total: u64,
    pub payment_by_last_name: u64,
    pub order_status_by_last_name: u64,
    pub order_status_total: u64,
    pub delivery_queued: u64,
    pub delivery_completed: u64,
    pub delivery_processed_districts: u64,
    pub delivery_skipped_districts: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeliverySummary {
    pub queued: u64,
    pub completed: u64,
    pub pending: u64,
    pub processed_districts: u64,
    pub skipped_districts: u64,
    pub completion_mean_ms: f64,
    pub completion_p50_ms: u64,
    pub completion_p95_ms: u64,
    pub completion_p99_ms: u64,
    pub completion_max_ms: u64,
    pub completion_histogram: Histogram,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DriverSummary {
    pub run_id: String,
    pub driver_id: String,
    pub uri: String,
    pub database: String,
    pub terminal_start: u32,
    pub terminals: u32,
    pub warehouse_count: u32,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,
    pub generated_at_ms: u64,
    pub total_transactions: u64,
    pub tpmc_like: f64,
    pub transaction_mix: BTreeMap<String, f64>,
    pub conformance: ConformanceSummary,
    pub transactions: BTreeMap<String, TransactionSummary>,
    pub delivery: DeliverySummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregateSummary {
    pub run_id: String,
    pub driver_count: usize,
    pub drivers: Vec<String>,
    pub generated_at_ms: u64,
    pub total_transactions: u64,
    pub tpmc_like: f64,
    pub transaction_mix: BTreeMap<String, f64>,
    pub conformance: ConformanceSummary,
    pub transactions: BTreeMap<String, TransactionSummary>,
    pub delivery: DeliverySummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EventLine {
    timestamp_ms: u64,
    run_id: String,
    driver_id: String,
    terminal_id: u32,
    transaction: String,
    success: bool,
    latency_ms: u64,
    rollback: bool,
    remote: bool,
    by_last_name: bool,
    order_line_count: u32,
    remote_order_line_count: u32,
    detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransactionRecord {
    pub timestamp_ms: u64,
    pub terminal_id: u32,
    pub kind: TransactionKind,
    pub success: bool,
    pub latency_ms: u64,
    pub rollback: bool,
    pub remote: bool,
    pub by_last_name: bool,
    pub order_line_count: u32,
    pub remote_order_line_count: u32,
    pub detail: Option<String>,
}

#[derive(Default)]
struct TransactionAccumulator {
    success: u64,
    failure: u64,
    histogram: Histogram,
}

impl TransactionAccumulator {
    fn record(&mut self, success: bool, latency_ms: u64) {
        if success {
            self.success += 1;
        } else {
            self.failure += 1;
        }
        self.histogram.record(latency_ms);
    }

    fn to_summary(&self) -> TransactionSummary {
        TransactionSummary {
            count: self.success + self.failure,
            success: self.success,
            failure: self.failure,
            mean_latency_ms: self.histogram.mean_ms(),
            p50_latency_ms: self.histogram.percentile_ms(0.50),
            p95_latency_ms: self.histogram.percentile_ms(0.95),
            p99_latency_ms: self.histogram.percentile_ms(0.99),
            max_latency_ms: self.histogram.max_ms,
            histogram: self.histogram.clone(),
        }
    }
}

pub struct MetricsCollector {
    run_id: String,
    driver_id: String,
    writer: BufWriter<File>,
    by_kind: BTreeMap<&'static str, TransactionAccumulator>,
    conformance: ConformanceSummary,
    delivery_completion_histogram: Histogram,
}

#[derive(Clone)]
pub struct SharedMetrics {
    inner: Arc<Mutex<MetricsCollector>>,
}

#[derive(Clone, Debug)]
pub struct DriverSummaryMeta {
    pub run_id: String,
    pub driver_id: String,
    pub uri: String,
    pub database: String,
    pub terminal_start: u32,
    pub terminals: u32,
    pub warehouse_count: u32,
    pub warmup_secs: u64,
    pub measure_secs: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,
}

impl SharedMetrics {
    pub fn create(run_id: &str, driver_id: &str, path: &Path) -> Result<Self> {
        let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
        let collector = MetricsCollector {
            run_id: run_id.to_string(),
            driver_id: driver_id.to_string(),
            writer: BufWriter::new(file),
            by_kind: TransactionKind::ALL
                .into_iter()
                .map(|kind| (kind.as_str(), TransactionAccumulator::default()))
                .collect(),
            conformance: ConformanceSummary::default(),
            delivery_completion_histogram: Histogram::default(),
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(collector)),
        })
    }

    pub fn record(&self, event: TransactionRecord) -> Result<()> {
        let mut collector = self.inner.lock();
        collector.record(event)
    }

    pub fn record_delivery_completion(&self, completion: &DeliveryCompletionView) {
        let mut collector = self.inner.lock();
        collector.record_delivery_completion(completion);
    }

    pub fn delivery_queued(&self) -> u64 {
        self.inner.lock().conformance.delivery_queued
    }

    pub fn finalize(self, meta: DriverSummaryMeta) -> Result<DriverSummary> {
        self.inner.lock().finalize(meta)
    }
}

impl MetricsCollector {
    fn record(&mut self, event: TransactionRecord) -> Result<()> {
        let line = EventLine {
            timestamp_ms: event.timestamp_ms,
            run_id: self.run_id.clone(),
            driver_id: self.driver_id.clone(),
            terminal_id: event.terminal_id,
            transaction: event.kind.as_str().to_string(),
            success: event.success,
            latency_ms: event.latency_ms,
            rollback: event.rollback,
            remote: event.remote,
            by_last_name: event.by_last_name,
            order_line_count: event.order_line_count,
            remote_order_line_count: event.remote_order_line_count,
            detail: event.detail.clone(),
        };
        serde_json::to_writer(&mut self.writer, &line)?;
        self.writer.write_all(b"\n")?;

        let accumulator = self
            .by_kind
            .get_mut(event.kind.as_str())
            .expect("all transaction kinds registered");
        accumulator.record(event.success, event.latency_ms);

        match event.kind {
            TransactionKind::NewOrder => {
                self.conformance.new_order_total += 1;
                self.conformance.new_order_total_order_lines += u64::from(event.order_line_count);
                self.conformance.new_order_remote_order_lines += u64::from(event.remote_order_line_count);
                if event.rollback {
                    self.conformance.new_order_rollbacks += 1;
                }
            }
            TransactionKind::Payment => {
                self.conformance.payment_total += 1;
                if event.remote {
                    self.conformance.payment_remote += 1;
                }
                if event.by_last_name {
                    self.conformance.payment_by_last_name += 1;
                }
            }
            TransactionKind::OrderStatus => {
                self.conformance.order_status_total += 1;
                if event.by_last_name {
                    self.conformance.order_status_by_last_name += 1;
                }
            }
            TransactionKind::Delivery => {
                self.conformance.delivery_queued += 1;
            }
            TransactionKind::StockLevel => {}
        }
        Ok(())
    }

    fn record_delivery_completion(&mut self, completion: &DeliveryCompletionView) {
        self.conformance.delivery_completed += 1;
        self.conformance.delivery_processed_districts += u64::from(completion.processed_districts);
        self.conformance.delivery_skipped_districts += u64::from(completion.skipped_districts);
        let lag_ms = completion
            .completed_at
            .to_micros_since_unix_epoch()
            .saturating_sub(completion.queued_at.to_micros_since_unix_epoch())
            .max(0) as u64
            / 1_000;
        self.delivery_completion_histogram.record(lag_ms);
    }

    fn finalize(&mut self, meta: DriverSummaryMeta) -> Result<DriverSummary> {
        self.writer.flush()?;

        let mut transactions = BTreeMap::new();
        let mut total_transactions = 0u64;
        for kind in TransactionKind::ALL {
            let summary = self
                .by_kind
                .get(kind.as_str())
                .expect("transaction kind exists")
                .to_summary();
            total_transactions += summary.count;
            transactions.insert(kind.as_str().to_string(), summary);
        }

        let mut mix = BTreeMap::new();
        for kind in TransactionKind::ALL {
            let count = transactions
                .get(kind.as_str())
                .map(|summary| summary.count)
                .unwrap_or(0);
            let ratio = if total_transactions == 0 {
                0.0
            } else {
                (count as f64) * 100.0 / (total_transactions as f64)
            };
            mix.insert(kind.as_str().to_string(), ratio);
        }

        let measure_minutes = if meta.measure_secs == 0 {
            0.0
        } else {
            meta.measure_secs as f64 / 60.0
        };
        let new_order_success = transactions
            .get(TransactionKind::NewOrder.as_str())
            .map(|summary| summary.success)
            .unwrap_or(0);
        let tpmc_like = if measure_minutes == 0.0 {
            0.0
        } else {
            new_order_success as f64 / measure_minutes
        };

        let delivery_completed = self.conformance.delivery_completed;
        let delivery_queued = self.conformance.delivery_queued;
        let delivery = DeliverySummary {
            queued: delivery_queued,
            completed: delivery_completed,
            pending: delivery_queued.saturating_sub(delivery_completed),
            processed_districts: self.conformance.delivery_processed_districts,
            skipped_districts: self.conformance.delivery_skipped_districts,
            completion_mean_ms: self.delivery_completion_histogram.mean_ms(),
            completion_p50_ms: self.delivery_completion_histogram.percentile_ms(0.50),
            completion_p95_ms: self.delivery_completion_histogram.percentile_ms(0.95),
            completion_p99_ms: self.delivery_completion_histogram.percentile_ms(0.99),
            completion_max_ms: self.delivery_completion_histogram.max_ms,
            completion_histogram: self.delivery_completion_histogram.clone(),
        };

        Ok(DriverSummary {
            run_id: meta.run_id,
            driver_id: meta.driver_id,
            uri: meta.uri,
            database: meta.database,
            terminal_start: meta.terminal_start,
            terminals: meta.terminals,
            warehouse_count: meta.warehouse_count,
            warmup_secs: meta.warmup_secs,
            measure_secs: meta.measure_secs,
            measure_start_ms: meta.measure_start_ms,
            measure_end_ms: meta.measure_end_ms,
            generated_at_ms: now_millis(),
            total_transactions,
            tpmc_like,
            transaction_mix: mix,
            conformance: self.conformance.clone(),
            transactions,
            delivery,
        })
    }
}

pub fn aggregate_summaries(run_id: String, summaries: &[DriverSummary]) -> AggregateSummary {
    let mut by_kind: BTreeMap<String, TransactionAccumulator> = TransactionKind::ALL
        .into_iter()
        .map(|kind| (kind.as_str().to_string(), TransactionAccumulator::default()))
        .collect();
    let mut total_transactions = 0u64;
    let mut conformance = ConformanceSummary::default();
    let mut delivery_histogram = Histogram::default();
    let mut driver_names = Vec::with_capacity(summaries.len());

    for summary in summaries {
        driver_names.push(summary.driver_id.clone());
        total_transactions += summary.total_transactions;
        conformance.new_order_rollbacks += summary.conformance.new_order_rollbacks;
        conformance.new_order_total += summary.conformance.new_order_total;
        conformance.new_order_remote_order_lines += summary.conformance.new_order_remote_order_lines;
        conformance.new_order_total_order_lines += summary.conformance.new_order_total_order_lines;
        conformance.payment_remote += summary.conformance.payment_remote;
        conformance.payment_total += summary.conformance.payment_total;
        conformance.payment_by_last_name += summary.conformance.payment_by_last_name;
        conformance.order_status_by_last_name += summary.conformance.order_status_by_last_name;
        conformance.order_status_total += summary.conformance.order_status_total;
        conformance.delivery_queued += summary.conformance.delivery_queued;
        conformance.delivery_completed += summary.conformance.delivery_completed;
        conformance.delivery_processed_districts += summary.conformance.delivery_processed_districts;
        conformance.delivery_skipped_districts += summary.conformance.delivery_skipped_districts;
        delivery_histogram.merge(&summary.delivery.completion_histogram);

        for (name, txn) in &summary.transactions {
            let acc = by_kind.get_mut(name).expect("kind exists");
            acc.success += txn.success;
            acc.failure += txn.failure;
            acc.histogram.merge(&txn.histogram);
        }
    }

    let mut transactions = BTreeMap::new();
    let mut mix = BTreeMap::new();
    for (name, acc) in by_kind {
        let summary = acc.to_summary();
        let ratio = if total_transactions == 0 {
            0.0
        } else {
            (summary.count as f64) * 100.0 / (total_transactions as f64)
        };
        mix.insert(name.clone(), ratio);
        transactions.insert(name, summary);
    }

    let measure_secs = summaries.first().map(|summary| summary.measure_secs).unwrap_or(0);
    let measure_minutes = if measure_secs == 0 {
        0.0
    } else {
        measure_secs as f64 / 60.0
    };
    let tpmc_like = if measure_minutes == 0.0 {
        0.0
    } else {
        transactions
            .get(TransactionKind::NewOrder.as_str())
            .map(|summary| summary.success as f64 / measure_minutes)
            .unwrap_or(0.0)
    };

    AggregateSummary {
        run_id,
        driver_count: summaries.len(),
        drivers: driver_names,
        generated_at_ms: now_millis(),
        total_transactions,
        tpmc_like,
        transaction_mix: mix,
        conformance: conformance.clone(),
        transactions,
        delivery: DeliverySummary {
            queued: conformance.delivery_queued,
            completed: conformance.delivery_completed,
            pending: conformance
                .delivery_queued
                .saturating_sub(conformance.delivery_completed),
            processed_districts: conformance.delivery_processed_districts,
            skipped_districts: conformance.delivery_skipped_districts,
            completion_mean_ms: delivery_histogram.mean_ms(),
            completion_p50_ms: delivery_histogram.percentile_ms(0.50),
            completion_p95_ms: delivery_histogram.percentile_ms(0.95),
            completion_p99_ms: delivery_histogram.percentile_ms(0.99),
            completion_max_ms: delivery_histogram.max_ms,
            completion_histogram: delivery_histogram,
        },
    }
}

pub fn log_driver_summary(summary: &DriverSummary, summary_path: &Path, events_path: &Path) {
    log::info!("run_id={}", summary.run_id);
    log::info!("driver_id={}", summary.driver_id);
    log::info!("tpmc_like={:.2}", summary.tpmc_like);
    log::info!("total_transactions={}", summary.total_transactions);
    for (name, txn) in &summary.transactions {
        log::info!(
            "{} count={} success={} failure={} p95_ms={} p99_ms={}",
            name,
            txn.count,
            txn.success,
            txn.failure,
            txn.p95_latency_ms,
            txn.p99_latency_ms
        );
    }
    log::info!(
        "delivery queued={} completed={} pending={}",
        summary.delivery.queued,
        summary.delivery.completed,
        summary.delivery.pending
    );
    log::info!("summary={}", summary_path.display());
    log::info!("events={}", events_path.display());
}

pub fn log_aggregate_summary(summary: &AggregateSummary, summary_path: &Path) {
    log::info!("run_id={}", summary.run_id);
    log::info!("driver_count={}", summary.driver_count);
    log::info!("tpmc_like={:.2}", summary.tpmc_like);
    log::info!("total_transactions={}", summary.total_transactions);
    for (name, txn) in &summary.transactions {
        log::info!(
            "{} count={} success={} failure={} p95_ms={} p99_ms={}",
            name,
            txn.count,
            txn.success,
            txn.failure,
            txn.p95_latency_ms,
            txn.p99_latency_ms
        );
    }
    log::info!(
        "delivery queued={} completed={} pending={}",
        summary.delivery.queued,
        summary.delivery.completed,
        summary.delivery.pending
    );
    log::info!("summary={}", summary_path.display());
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file = File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    serde_json::to_writer_pretty(file, value).with_context(|| format!("failed to write {}", path.display()))
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}
