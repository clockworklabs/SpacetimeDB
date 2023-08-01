use crate::host::instance_env::InstanceEnv;
use crate::messages::instance_db_trace_log::{InstanceEvent, InstanceEventType};
use flate2::read::GzDecoder;
use imara_diff::intern::InternedInput;
use imara_diff::sink::Counter;
use imara_diff::sources::byte_lines;
use imara_diff::Algorithm;
use serde::Serialize;
use spacetimedb_sats::bsatn;
use std::collections::HashMap;
use std::io::{self, Read};
use std::time::{Duration, SystemTime};
use strum::AsRefStr;

#[derive(Debug, Eq, PartialEq, AsRefStr)]
pub enum ReplayEventType {
    Insert,
    // DeletePk(bool),
    // DeleteValue(bool),
    DeleteEq(u32),
    // DeleteRange(u32),
    // CreateTable(u32),
    Iter(Vec<u8>),
    GetTableId(u32),
    CreateIndex,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReplayEvent {
    pub duration: Duration,
    pub kind: ReplayEventType,
}

impl From<InstanceEvent> for ReplayEvent {
    fn from(event: InstanceEvent) -> Self {
        ReplayEvent {
            duration: Duration::from_micros(event.duration_micros),
            kind: event.r#type.into(),
        }
    }
}

impl From<InstanceEventType> for ReplayEventType {
    fn from(event: InstanceEventType) -> Self {
        match event {
            InstanceEventType::Insert(_) => Self::Insert,
            InstanceEventType::DeleteEq(event) => Self::DeleteEq(event.result_deleted_count),
            /*
            InstanceEventType::DeletePk(event) => Self::DeletePk(event.result_success),
            InstanceEventType::DeleteValue(event) => Self::DeleteValue(event.result_success),
            InstanceEventType::DeleteRange(event) => Self::DeleteRange(event.result_deleted_count),
            InstanceEventType::CreateTable(event) => Self::CreateTable(event.result_table_id),
            */
            InstanceEventType::GetTableId(event) => Self::GetTableId(event.result_table_id),
            InstanceEventType::Iter(event) => Self::Iter(event.result_bytes),
            InstanceEventType::CreateIndex(_) => Self::CreateIndex,
        }
    }
}

/// Take a trace log and replay it into an instance environment.
pub fn replay_tracelog<R>(reader: R, instance_env: &InstanceEnv) -> ReplayTracelog<'_, R> {
    ReplayTracelog { reader, instance_env }
}

pub struct ReplayTracelog<'a, R> {
    reader: R,
    instance_env: &'a InstanceEnv,
}

impl<'a, R: Read> ReplayTracelog<'a, R> {
    fn try_next(&mut self) -> anyhow::Result<Option<(ReplayEvent, ReplayEvent)>> {
        let mut len_byte: [u8; 8] = [0; 8];
        let prefix_result = self.reader.read_exact(&mut len_byte[..]);
        match prefix_result {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let compressed_event_len = usize::from_le_bytes(len_byte);

        let mut d = GzDecoder::new(self.reader.by_ref().take(compressed_event_len as u64));
        let mut event_buffer = vec![];
        d.read_to_end(&mut event_buffer)?;

        let old_event: InstanceEvent = bsatn::from_slice(&event_buffer)?;
        let new_event = execute_event(self.instance_env, &old_event.r#type)?;
        Ok(Some((old_event.into(), new_event)))
    }
}

impl<'a, R: Read> Iterator for ReplayTracelog<'a, R> {
    type Item = anyhow::Result<(ReplayEvent, ReplayEvent)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.try_next().transpose()
    }
}

fn execute_event(instance_env: &InstanceEnv, event: &InstanceEventType) -> anyhow::Result<ReplayEvent> {
    let start_time = SystemTime::now();
    let kind = match event {
        InstanceEventType::Insert(insert) => {
            instance_env.insert(insert.table_id, &insert.buffer).unwrap();
            ReplayEventType::Insert
        }
        /*
        InstanceEventType::DeletePk(delete) => {
            let result_success = instance_env.delete_pk(delete.table_id, &delete.buffer).is_ok();
            ReplayEventType::DeletePk(result_success)
        }
        InstanceEventType::DeleteValue(delete) => {
            let result_success = instance_env.delete_value(delete.table_id, &delete.buffer).is_ok();
            ReplayEventType::DeleteValue(result_success)
        }
        */
        InstanceEventType::DeleteEq(delete) => {
            let result_count = instance_env
                .delete_eq(delete.table_id, delete.col_id, &delete.buffer)
                .unwrap();
            ReplayEventType::DeleteEq(result_count)
        }
        /*
        InstanceEventType::DeleteRange(delete) => {
            let result_count = instance_env
                .delete_range(delete.table_id, delete.col_id, &delete.start_buffer, &delete.end_buffer)
                .unwrap();
            ReplayEventType::DeleteRange(result_count)
        }
        InstanceEventType::CreateTable(create) => {
            let result_table_id = instance_env
                .create_table(&create.table_name, &create.schema_buffer)
                .unwrap();
            ReplayEventType::CreateTable(result_table_id)
        }
        */
        InstanceEventType::Iter(iter) => {
            let result_bytes = instance_env.iter(iter.table_id).try_fold(Vec::new(), |mut acc, row| {
                row.map(|row| {
                    acc.extend_from_slice(&row);
                    acc
                })
            })?;
            ReplayEventType::Iter(result_bytes)
        }
        InstanceEventType::GetTableId(gti) => {
            let table_id = instance_env.get_table_id(gti.table_name.clone()).unwrap();
            ReplayEventType::GetTableId(table_id)
        }
        InstanceEventType::CreateIndex(ci) => {
            let col_ids: Vec<u8> = ci.col_ids.iter().map(|id| *id as u8).collect();
            instance_env.create_index(ci.index_name.clone(), ci.table_id, ci.index_type as u8, col_ids)?;
            ReplayEventType::CreateIndex
        }
    };
    Ok(ReplayEvent {
        duration: start_time.elapsed().unwrap(),
        kind,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplayTracelogReport {
    events: HashMap<String, Event>,
    replayed: usize,
    total_replay_time_micros: u128,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    aggregation: EventAggregation,
    event_logs: Vec<EventLog>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventAggregation {
    divergence_avg: f64,
    num_events: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventLog {
    duration_delta_micros: f64,
    old_duration: f64,
    new_duration: f64,
    diff: Option<EventDiff>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventDiff {
    diff_descr: Option<String>,
    buffer_diff: Option<BufferDiff>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BufferDiff {
    total: usize,
    insertions: usize,
    removals: usize,
}

fn diff_for_event(old_event: &ReplayEventType, new_event: &ReplayEventType) -> Option<EventDiff> {
    if old_event != new_event {
        Some(EventDiff {
            diff_descr: if let ReplayEventType::Iter(_) = old_event {
                None
            } else {
                Some(format!(
                    "Replayed event differs; old {:?} new {:?}",
                    old_event, new_event
                ))
            },
            buffer_diff: if let ReplayEventType::Iter(old_iter) = old_event {
                match new_event {
                    ReplayEventType::Iter(ref new_iter) => {
                        let before = byte_lines(old_iter.as_slice());
                        let after = byte_lines(new_iter.as_slice());
                        let diff_input = InternedInput::new(before, after);

                        // TODO: the diff here can be smarter and actually produce a unified diff maybe
                        // but for binary data probably isn't going to be too useful.
                        let diff = imara_diff::diff(Algorithm::Histogram, &diff_input, Counter::default());
                        Some(BufferDiff {
                            total: diff.total(),
                            insertions: diff.insertions as usize,
                            removals: diff.removals as usize,
                        })
                    }
                    _ => None,
                }
            } else {
                None
            },
        })
    } else {
        None
    }
}

pub fn replay_report(iv: &InstanceEnv, reader: &mut impl std::io::Read) -> Result<ReplayTracelogReport, anyhow::Error> {
    let start = SystemTime::now();

    let mut time_divergence = HashMap::new();
    let mut cnt_types = HashMap::new();

    let mut event_logs = HashMap::new();

    let mut replayed = 0;
    for res in replay_tracelog(reader, iv) {
        let (old_event, new_event) = res?;
        replayed += 1;

        let event_type_str: &str = new_event.kind.as_ref();
        let event_type_str = String::from(event_type_str);

        let old_duration = old_event.duration.as_micros() as f64;
        let new_duration = new_event.duration.as_micros() as f64;
        let diff = new_duration - old_duration;

        let diverge_entry = time_divergence.entry(event_type_str.clone());
        diverge_entry.and_modify(|e| *e += diff).or_insert(diff);

        let cnt_entry = cnt_types.entry(event_type_str.clone());
        cnt_entry.and_modify(|e| *e += 1).or_insert(1usize);

        let event_log = EventLog {
            duration_delta_micros: diff,
            old_duration,
            new_duration,
            diff: diff_for_event(&old_event.kind, &new_event.kind),
        };
        event_logs
            .entry(event_type_str)
            .and_modify(|e: &mut Vec<EventLog>| e.push(event_log.clone()))
            .or_insert(vec![event_log]);
    }
    let duration = start.elapsed().unwrap();

    let mut events = HashMap::new();
    for e in time_divergence {
        let event_type = e.0;
        if let Some(event_logs) = event_logs.remove(event_type.as_str()) {
            let numer = e.1;
            let denom = *cnt_types.get(event_type.as_str()).unwrap();
            let aggregation = EventAggregation {
                divergence_avg: numer / (denom as f64),
                num_events: denom,
            };

            let event = Event {
                aggregation,
                event_logs,
            };
            events.insert(event_type, event);
        }
    }

    let resp_body = ReplayTracelogReport {
        events,
        replayed,
        total_replay_time_micros: duration.as_micros(),
    };

    Ok(resp_body)
}
