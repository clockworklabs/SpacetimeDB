use anyhow::{anyhow, bail, Context, Result};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::ConnectionConfig;
use crate::module_bindings::*;
use spacetimedb_sdk::DbContext;
use tokio::sync::oneshot;

pub struct ModuleClient {
    conn: DbConnection,
    timeout: Duration,
}

impl ModuleClient {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let ready_tx = Arc::new(Mutex::new(Some(ready_tx)));
        let success_tx = Arc::clone(&ready_tx);
        let error_tx = Arc::clone(&ready_tx);
        let mut builder = DbConnection::builder()
            .with_uri(config.uri.clone())
            .with_database_name(config.database.clone())
            .with_confirmed_reads(config.confirmed_reads)
            .on_connect(move |_, _, _| {
                if let Some(tx) = success_tx.lock().expect("ready mutex poisoned").take() {
                    let _ = tx.send(Ok::<(), anyhow::Error>(()));
                }
            })
            .on_connect_error(move |_, error| {
                if let Some(tx) = error_tx.lock().expect("ready mutex poisoned").take() {
                    let _ = tx.send(Err(anyhow!("connection failed: {error}")));
                }
            });

        if let Some(token) = &config.token {
            builder = builder.with_token(Some(token.clone()));
        }

        let conn = builder.build().context("failed to build database connection")?;
        Self::await_with_conn(&conn, Duration::from_secs(config.timeout_secs), "connection", ready_rx).await??;

        Ok(Self {
            conn,
            timeout: Duration::from_secs(config.timeout_secs),
        })
    }

    pub async fn reset_tpcc(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.reset_tpcc_then(move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("reset_tpcc", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("reset_tpcc failed: {}", message),
            Err(err) => Err(anyhow!("reset_tpcc internal error: {}", err)),
        }
    }

    pub async fn load_warehouses(&self, rows: Vec<Warehouse>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_warehouses_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_warehouses", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_warehouses failed: {}", message),
            Err(err) => Err(anyhow!("load_warehouses internal error: {}", err)),
        }
    }

    pub async fn load_districts(&self, rows: Vec<District>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_districts_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_districts", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_districts failed: {}", message),
            Err(err) => Err(anyhow!("load_districts internal error: {}", err)),
        }
    }

    pub async fn load_customers(&self, rows: Vec<Customer>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_customers_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_customers", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_customers failed: {}", message),
            Err(err) => Err(anyhow!("load_customers internal error: {}", err)),
        }
    }

    pub async fn load_history(&self, rows: Vec<History>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_history_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_history", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_history failed: {}", message),
            Err(err) => Err(anyhow!("load_history internal error: {}", err)),
        }
    }

    pub async fn load_items(&self, rows: Vec<Item>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_items_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_items", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_items failed: {}", message),
            Err(err) => Err(anyhow!("load_items internal error: {}", err)),
        }
    }

    pub async fn load_stocks(&self, rows: Vec<Stock>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_stocks_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_stocks", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_stocks failed: {}", message),
            Err(err) => Err(anyhow!("load_stocks internal error: {}", err)),
        }
    }

    pub async fn load_orders(&self, rows: Vec<OOrder>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_orders_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_orders", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_orders failed: {}", message),
            Err(err) => Err(anyhow!("load_orders internal error: {}", err)),
        }
    }

    pub async fn load_new_orders(&self, rows: Vec<NewOrder>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_new_orders_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_new_orders", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_new_orders failed: {}", message),
            Err(err) => Err(anyhow!("load_new_orders internal error: {}", err)),
        }
    }

    pub async fn load_order_lines(&self, rows: Vec<OrderLine>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.conn.reducers.load_order_lines_then(rows, move |_, res| {
            let _ = tx.send(res);
        })?;
        match self.await_result("load_order_lines", rx).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => bail!("load_order_lines failed: {}", message),
            Err(err) => Err(anyhow!("load_order_lines internal error: {}", err)),
        }
    }

    pub async fn new_order(
        &self,
        w_id: u16,
        d_id: u8,
        c_id: u32,
        order_lines: Vec<NewOrderLineInput>,
    ) -> Result<Result<NewOrderResult, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn
            .procedures
            .new_order_then(w_id, d_id, c_id, order_lines, move |_, res| {
                let _ = tx.send(res);
            });
        match self.await_result("new_order", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("new_order internal error: {}", err)),
        }
    }

    pub async fn payment(
        &self,
        w_id: u16,
        d_id: u8,
        c_w_id: u16,
        c_d_id: u8,
        customer: CustomerSelector,
        payment_amount_cents: i64,
    ) -> Result<Result<PaymentResult, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn.procedures.payment_then(
            w_id,
            d_id,
            c_w_id,
            c_d_id,
            customer,
            payment_amount_cents,
            move |_, res| {
                let _ = tx.send(res);
            },
        );
        match self.await_result("payment", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("payment internal error: {}", err)),
        }
    }

    pub async fn order_status(
        &self,
        w_id: u16,
        d_id: u8,
        customer: CustomerSelector,
    ) -> Result<Result<OrderStatusResult, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn
            .procedures
            .order_status_then(w_id, d_id, customer, move |_, res| {
                let _ = tx.send(res);
            });
        match self.await_result("order_status", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("order_status internal error: {}", err)),
        }
    }

    pub async fn stock_level(&self, w_id: u16, d_id: u8, threshold: i32) -> Result<Result<StockLevelResult, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn
            .procedures
            .stock_level_then(w_id, d_id, threshold, move |_, res| {
                let _ = tx.send(res);
            });
        match self.await_result("stock_level", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("stock_level internal error: {}", err)),
        }
    }

    pub async fn queue_delivery(
        &self,
        run_id: String,
        driver_id: String,
        terminal_id: u32,
        request_id: u64,
        w_id: u16,
        carrier_id: u8,
    ) -> Result<Result<DeliveryQueueAck, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn.procedures.queue_delivery_then(
            run_id,
            driver_id,
            terminal_id,
            request_id,
            w_id,
            carrier_id,
            move |_, res| {
                let _ = tx.send(res);
            },
        );
        match self.await_result("queue_delivery", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("queue_delivery internal error: {}", err)),
        }
    }

    pub async fn delivery_progress(&self, run_id: String) -> Result<Result<DeliveryProgress, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn.procedures.delivery_progress_then(run_id, move |_, res| {
            let _ = tx.send(res);
        });
        match self.await_result("delivery_progress", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("delivery_progress internal error: {}", err)),
        }
    }

    pub async fn fetch_delivery_completions(
        &self,
        run_id: String,
        after_completion_id: u64,
        limit: u32,
    ) -> Result<Result<Vec<DeliveryCompletionView>, String>> {
        let (tx, rx) = oneshot::channel();
        self.conn
            .procedures
            .fetch_delivery_completions_then(run_id, after_completion_id, limit, move |_, res| {
                let _ = tx.send(res);
            });
        match self.await_result("fetch_delivery_completions", rx).await? {
            Ok(value) => Ok(value),
            Err(err) => Err(anyhow!("fetch_delivery_completions internal error: {}", err)),
        }
    }

    pub async fn shutdown(self) {
        if self.conn.is_active() && self.conn.disconnect().is_ok() {
            let _ = tokio::time::timeout(self.timeout, self.conn.advance_one_message_async()).await;
        }
    }

    async fn await_result<T>(&self, operation: &str, rx: oneshot::Receiver<T>) -> Result<T> {
        Self::await_with_conn(&self.conn, self.timeout, operation, rx).await
    }

    async fn await_with_conn<T>(
        conn: &DbConnection,
        timeout: Duration,
        operation: &str,
        mut rx: oneshot::Receiver<T>,
    ) -> Result<T> {
        tokio::time::timeout(timeout, async {
            loop {
                tokio::select! {
                    result = &mut rx => {
                        return result.map_err(|_| anyhow!("{operation} callback dropped"));
                    }
                    message = conn.advance_one_message_async() => {
                        message.with_context(|| format!("{operation} connection loop failed"))?;
                    }
                }
            }
        })
        .await
        .with_context(|| format!("timed out waiting for {operation}"))?
    }
}

pub fn expect_ok<T>(operation: &str, result: Result<Result<T, String>>) -> Result<T> {
    match result? {
        Ok(value) => Ok(value),
        Err(message) => bail!("{} failed: {}", operation, message),
    }
}
