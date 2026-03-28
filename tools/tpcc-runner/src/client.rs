use anyhow::{anyhow, bail, Context, Result};
use std::sync::mpsc::sync_channel;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::config::ConnectionConfig;
use crate::module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity};

pub struct ModuleClient {
    conn: DbConnection,
    thread: Option<JoinHandle<()>>,
    timeout: Duration,
}

impl ModuleClient {
    pub fn connect(config: &ConnectionConfig, database_identity: Identity) -> Result<Self> {
        let (ready_tx, ready_rx) = sync_channel(1);
        let success_tx = ready_tx.clone();
        let error_tx = ready_tx;
        let mut builder = DbConnection::builder()
            .with_uri(config.uri.clone())
            .with_database_name(database_identity.to_string())
            .with_confirmed_reads(config.confirmed_reads)
            .on_connect(move |_, _, _| {
                let _ = success_tx.send(Ok::<(), anyhow::Error>(()));
            })
            .on_connect_error(move |_, error| {
                let _ = error_tx.send(Err(anyhow!("connection failed: {error}")));
            });

        if let Some(token) = &config.token {
            builder = builder.with_token(Some(token.clone()));
        }

        let conn = builder.build().context("failed to build database connection")?;
        let thread = conn.run_threaded();
        ready_rx
            .recv_timeout(Duration::from_secs(config.timeout_secs))
            .context("timed out waiting for connection")??;

        Ok(Self {
            conn,
            thread: Some(thread),
            timeout: Duration::from_secs(config.timeout_secs),
        })
    }

    pub fn set_spacetimedb_uri(&self, uri: &str) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn
            .reducers
            .set_spacetimedb_uri_then(uri.to_string(), move |_, res| {
                log::debug!("Got response from `set_spacetimedb_uri`: {res:?}");
                let _ = tx.send(res);
            })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("set_spacetimedb_uri failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("set_spacetimedb_uri internal error: {}", err)),
            Err(_) => bail!("timed out waiting for set_spacetimedb_uri"),
        }
    }

    pub fn reset_tpcc(&self) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.reset_tpcc_then(move |_, res| {
            log::debug!("Got response from `reset_tpcc`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("reset_tpcc failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("reset_tpcc internal error: {}", err)),
            Err(_) => bail!("timed out waiting for reset_tpcc"),
        }
    }

    pub fn load_remote_warehouses(&self, rows: Vec<RemoteWarehouse>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_remote_warehouses_then(rows, move |_, res| {
            log::debug!("Got response from `load_remote_warehouse`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_remote_warehouses failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_remote_warehouses internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_remote_warehouses"),
        }
    }

    pub fn load_warehouses(&self, rows: Vec<Warehouse>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_warehouses_then(rows, move |_, res| {
            log::debug!("Got response from `load_warehouses`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_warehouses failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_warehouses internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_warehouses"),
        }
    }

    pub fn load_districts(&self, rows: Vec<District>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_districts_then(rows, move |_, res| {
            log::debug!("Got response from `load_districts`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_districts failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_districts internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_districts"),
        }
    }

    pub fn load_customers(&self, rows: Vec<Customer>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_customers_then(rows, move |_, res| {
            log::debug!("Got response from `load_customers`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_customers failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_customers internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_customers"),
        }
    }

    pub fn load_history(&self, rows: Vec<History>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_history_then(rows, move |_, res| {
            log::debug!("Got response from `load_history`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_history failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_history internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_history"),
        }
    }

    pub fn load_items(&self, rows: Vec<Item>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_items_then(rows, move |_, res| {
            log::debug!("Got response from `load_items`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_items failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_items internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_items"),
        }
    }

    pub fn load_stocks(&self, rows: Vec<Stock>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_stocks_then(rows, move |_, res| {
            log::debug!("Got response from `load_stocks`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_stocks failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_stocks internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_stocks"),
        }
    }

    pub fn load_orders(&self, rows: Vec<OOrder>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_orders_then(rows, move |_, res| {
            log::debug!("Got response from `load_orders`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_orders failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_orders internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_orders"),
        }
    }

    pub fn load_new_orders(&self, rows: Vec<NewOrder>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_new_orders_then(rows, move |_, res| {
            log::debug!("Got response from `load_new_orders`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_new_orders failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_new_orders internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_new_orders"),
        }
    }

    pub fn load_order_lines(&self, rows: Vec<OrderLine>) -> Result<()> {
        let (tx, rx) = sync_channel(1);
        self.conn.reducers.load_order_lines_then(rows, move |_, res| {
            log::debug!("Got response from `load_order_lines`: {res:?}");
            let _ = tx.send(res);
        })?;
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(message))) => bail!("load_order_lines failed: {}", message),
            Ok(Err(err)) => Err(anyhow!("load_order_lines internal error: {}", err)),
            Err(_) => bail!("timed out waiting for load_order_lines"),
        }
    }

    pub fn new_order(
        &self,
        w_id: u16,
        d_id: u8,
        c_id: u32,
        order_lines: Vec<NewOrderLineInput>,
    ) -> Result<Result<NewOrderResult, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn
            .procedures
            .new_order_then(w_id, d_id, c_id, order_lines, move |_, res| {
                log::debug!("Got response from `new_order`: {res:?}");
                let _ = tx.send(res);
            });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("new_order internal error: {}", err)),
            Err(_) => bail!("timed out waiting for new_order"),
        }
    }

    pub fn payment(
        &self,
        w_id: u16,
        d_id: u8,
        c_w_id: u16,
        c_d_id: u8,
        customer: CustomerSelector,
        payment_amount_cents: i64,
    ) -> Result<Result<PaymentResult, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn.procedures.payment_then(
            w_id,
            d_id,
            c_w_id,
            c_d_id,
            customer,
            payment_amount_cents,
            move |_, res| {
                log::debug!("Got response from `payment`: {res:?}");
                let _ = tx.send(res);
            },
        );
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("payment internal error: {}", err)),
            Err(_) => bail!("timed out waiting for payment"),
        }
    }

    pub fn order_status(
        &self,
        w_id: u16,
        d_id: u8,
        customer: CustomerSelector,
    ) -> Result<Result<OrderStatusResult, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn
            .procedures
            .order_status_then(w_id, d_id, customer, move |_, res| {
                log::debug!("Got response from `order_status`: {res:?}");
                let _ = tx.send(res);
            });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("order_status internal error: {}", err)),
            Err(_) => bail!("timed out waiting for order_status"),
        }
    }

    pub fn stock_level(&self, w_id: u16, d_id: u8, threshold: i32) -> Result<Result<StockLevelResult, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn
            .procedures
            .stock_level_then(w_id, d_id, threshold, move |_, res| {
                log::debug!("Got response from `stock_level`: {res:?}");
                let _ = tx.send(res);
            });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("stock_level internal error: {}", err)),
            Err(_) => bail!("timed out waiting for stock_level"),
        }
    }

    pub fn queue_delivery(
        &self,
        run_id: String,
        driver_id: String,
        terminal_id: u32,
        request_id: u64,
        w_id: u16,
        carrier_id: u8,
    ) -> Result<Result<DeliveryQueueAck, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn.procedures.queue_delivery_then(
            run_id,
            driver_id,
            terminal_id,
            request_id,
            w_id,
            carrier_id,
            move |_, res| {
                log::debug!("Got response from `queue_delivery`: {res:?}");
                let _ = tx.send(res);
            },
        );
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("queue_delivery internal error: {}", err)),
            Err(_) => bail!("timed out waiting for queue_delivery"),
        }
    }

    pub fn delivery_progress(&self, run_id: String) -> Result<Result<DeliveryProgress, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn.procedures.delivery_progress_then(run_id, move |_, res| {
            log::debug!("Got response from `delivery_progress`: {res:?}");
            let _ = tx.send(res);
        });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("delivery_progress internal error: {}", err)),
            Err(_) => bail!("timed out waiting for delivery_progress"),
        }
    }

    pub fn fetch_delivery_completions(
        &self,
        run_id: String,
        after_completion_id: u64,
        limit: u32,
    ) -> Result<Result<Vec<DeliveryCompletionView>, String>> {
        let (tx, rx) = sync_channel(1);
        self.conn
            .procedures
            .fetch_delivery_completions_then(run_id, after_completion_id, limit, move |_, res| {
                log::debug!("Got response from `fetch_delivery_completions`: {res:?}");
                let _ = tx.send(res);
            });
        match rx.recv_timeout(self.timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(anyhow!("fetch_delivery_completions internal error: {}", err)),
            Err(_) => bail!("timed out waiting for fetch_delivery_completions"),
        }
    }

    pub fn shutdown(mut self) {
        let _ = self.conn.disconnect();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn expect_ok<T>(operation: &str, result: Result<Result<T, String>>) -> Result<T> {
    match result? {
        Ok(value) => Ok(value),
        Err(message) => bail!("{} failed: {}", operation, message),
    }
}
