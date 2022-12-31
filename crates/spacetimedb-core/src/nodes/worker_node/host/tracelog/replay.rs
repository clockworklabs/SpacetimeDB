use crate::nodes::worker_node::host::instance_env::InstanceEnv;
use crate::protobuf::instance_db_trace_log::instance_event::Type;
use crate::protobuf::instance_db_trace_log::InstanceEvent;
use flate2::read::GzDecoder;
use imara_diff::intern::InternedInput;
use imara_diff::sink::Counter;
use imara_diff::sources::byte_lines;
use imara_diff::Algorithm;
use prost::Message;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::time::{Duration, SystemTime};
use strum::AsRefStr;

#[derive(Debug, Eq, PartialEq, AsRefStr)]
pub enum ReplayEvent {
    Insert,
    DeletePk(bool),
    DeleteValue(bool),
    DeleteEq(u32),
    DeleteRange(u32),
    CreateTable(u32),
    Iter(Vec<u8>),
    GetTableId(u32),
}

/// Take a trace log and replay it into an instance environment.
pub fn replay_tracelog<SinkFn>(
    reader: &mut dyn Read,
    instance_env: &InstanceEnv,
    mut sink: SinkFn,
) -> Result<usize, anyhow::Error>
where
    SinkFn: FnMut(ReplayEvent, ReplayEvent, Duration, Duration),
{
    let mut event_count = 0;
    loop {
        let mut len_byte: [u8; 8] = [0; 8];
        let prefix_result = reader.read_exact(&mut len_byte[..]);
        if !prefix_result.is_ok() {
            log::info!("Done trace. {} events replayed", event_count);
            return Ok(event_count);
        }
        let event_len = usize::from_le_bytes(len_byte);
        let mut event_buffer_compressed = vec![0; event_len];
        reader.read_exact(&mut event_buffer_compressed)?;
        event_count += 1;

        let event_buffer = {
            let mut d = GzDecoder::new(&event_buffer_compressed[..]);
            let mut event_buffer = vec![];
            d.read_to_end(&mut event_buffer)?;
            event_buffer
        };

        let event: InstanceEvent = InstanceEvent::decode(&mut Cursor::new(event_buffer))?;
        let now = SystemTime::now();
        let old_duration = Duration::from_micros(event.duration_micros);
        if let Some(etype) = event.r#type {
            match etype {
                Type::Insert(insert) => {
                    instance_env
                        .insert(insert.table_id, bytes::Bytes::from(insert.buffer))
                        .unwrap();
                    sink(
                        ReplayEvent::Insert,
                        ReplayEvent::Insert,
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::DeletePk(delete) => {
                    let result_success = instance_env
                        .delete_pk(delete.table_id, bytes::Bytes::from(delete.buffer))
                        .is_ok();
                    sink(
                        ReplayEvent::DeletePk(delete.result_success),
                        ReplayEvent::DeletePk(result_success),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::DeleteValue(delete) => {
                    let result_success = instance_env
                        .delete_value(delete.table_id, bytes::Bytes::from(delete.buffer))
                        .is_ok();
                    sink(
                        ReplayEvent::DeleteValue(delete.result_success),
                        ReplayEvent::DeleteValue(result_success),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::DeleteEq(delete) => {
                    let result_count = instance_env
                        .delete_eq(delete.table_id, delete.col_id, bytes::Bytes::from(delete.buffer))
                        .unwrap();
                    sink(
                        ReplayEvent::DeleteEq(delete.result_deleted_count),
                        ReplayEvent::DeleteEq(result_count),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::DeleteRange(delete) => {
                    let result_count = instance_env
                        .delete_range(delete.table_id, delete.col_id, bytes::Bytes::from(delete.buffer))
                        .unwrap();
                    sink(
                        ReplayEvent::DeleteRange(delete.result_deleted_count),
                        ReplayEvent::DeleteRange(result_count),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::CreateTable(create) => {
                    let result_table_id = instance_env.create_table(bytes::Bytes::from(create.schema_buffer));
                    sink(
                        ReplayEvent::CreateTable(create.result_table_id),
                        ReplayEvent::CreateTable(result_table_id),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::Iter(iter) => {
                    let result_bytes = instance_env.iter(iter.table_id);
                    sink(
                        ReplayEvent::Iter(iter.result_bytes),
                        ReplayEvent::Iter(result_bytes),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
                Type::GetTableId(gti) => {
                    let table_id = instance_env.get_table_id(bytes::Bytes::from(gti.buffer)).unwrap();
                    sink(
                        ReplayEvent::GetTableId(gti.result_table_id),
                        ReplayEvent::GetTableId(table_id),
                        old_duration,
                        now.elapsed().unwrap(),
                    );
                }
            }
        }
    }
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

fn diff_for_event(old_event: &ReplayEvent, new_event: &ReplayEvent) -> Option<EventDiff> {
    if old_event != new_event {
        Some(EventDiff {
            diff_descr: if let ReplayEvent::Iter(_) = old_event {
                None
            } else {
                Some(format!(
                    "Replayed event differs; old {:?} new {:?}",
                    old_event, new_event
                ))
            },
            buffer_diff: if let ReplayEvent::Iter(old_iter) = old_event {
                match new_event {
                    ReplayEvent::Iter(ref new_iter) => {
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
    let replay_sink =
        |old_event: ReplayEvent, new_event: ReplayEvent, old_duration: Duration, new_duration: Duration| {
            let event_type_str: &str = new_event.as_ref();
            let event_type_str = String::from(event_type_str);

            let old_duration = old_duration.as_micros() as f64;
            let new_duration = new_duration.as_micros() as f64;
            let diff = new_duration - old_duration;

            let diverge_entry = time_divergence.entry(event_type_str.clone());
            diverge_entry.and_modify(|e| *e += diff).or_insert(diff);

            let cnt_entry = cnt_types.entry(event_type_str.clone());
            cnt_entry.and_modify(|e| *e += 1).or_insert(1usize);

            let event_log = EventLog {
                duration_delta_micros: diff,
                old_duration,
                new_duration,
                diff: diff_for_event(&old_event, &new_event),
            };
            event_logs
                .entry(event_type_str)
                .and_modify(|e: &mut Vec<EventLog>| e.push(event_log.clone()))
                .or_insert(vec![event_log]);
        };

    let replayed = replay_tracelog(reader, iv, replay_sink).expect("could not replay");
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
