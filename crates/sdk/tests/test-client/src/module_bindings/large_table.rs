// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::byte_struct::ByteStruct;
use super::enum_with_payload::EnumWithPayload;
use super::every_primitive_struct::EveryPrimitiveStruct;
use super::every_vec_struct::EveryVecStruct;
use super::simple_enum::SimpleEnum;
use super::unit_struct::UnitStruct;
use spacetimedb_sdk::{
    anyhow::{anyhow, Result},
    identity::Identity,
    reducer::{Reducer, ReducerCallbackId, Status},
    sats::{de::Deserialize, i256, ser::Serialize, u256},
    spacetimedb_lib,
    table::{TableIter, TableType, TableWithPrimaryKey},
    Address, ScheduleAt,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct LargeTable {
    pub a: u8,
    pub b: u16,
    pub c: u32,
    pub d: u64,
    pub e: u128,
    pub f: u256,
    pub g: i8,
    pub h: i16,
    pub i: i32,
    pub j: i64,
    pub k: i128,
    pub l: i256,
    pub m: bool,
    pub n: f32,
    pub o: f64,
    pub p: String,
    pub q: SimpleEnum,
    pub r: EnumWithPayload,
    pub s: UnitStruct,
    pub t: ByteStruct,
    pub u: EveryPrimitiveStruct,
    pub v: EveryVecStruct,
}

impl TableType for LargeTable {
    const TABLE_NAME: &'static str = "large_table";
    type ReducerEvent = super::ReducerEvent;
}

impl LargeTable {
    #[allow(unused)]
    pub fn filter_by_a(a: u8) -> TableIter<Self> {
        Self::filter(|row| row.a == a)
    }
    #[allow(unused)]
    pub fn filter_by_b(b: u16) -> TableIter<Self> {
        Self::filter(|row| row.b == b)
    }
    #[allow(unused)]
    pub fn filter_by_c(c: u32) -> TableIter<Self> {
        Self::filter(|row| row.c == c)
    }
    #[allow(unused)]
    pub fn filter_by_d(d: u64) -> TableIter<Self> {
        Self::filter(|row| row.d == d)
    }
    #[allow(unused)]
    pub fn filter_by_e(e: u128) -> TableIter<Self> {
        Self::filter(|row| row.e == e)
    }
    #[allow(unused)]
    pub fn filter_by_f(f: u256) -> TableIter<Self> {
        Self::filter(|row| row.f == f)
    }
    #[allow(unused)]
    pub fn filter_by_g(g: i8) -> TableIter<Self> {
        Self::filter(|row| row.g == g)
    }
    #[allow(unused)]
    pub fn filter_by_h(h: i16) -> TableIter<Self> {
        Self::filter(|row| row.h == h)
    }
    #[allow(unused)]
    pub fn filter_by_i(i: i32) -> TableIter<Self> {
        Self::filter(|row| row.i == i)
    }
    #[allow(unused)]
    pub fn filter_by_j(j: i64) -> TableIter<Self> {
        Self::filter(|row| row.j == j)
    }
    #[allow(unused)]
    pub fn filter_by_k(k: i128) -> TableIter<Self> {
        Self::filter(|row| row.k == k)
    }
    #[allow(unused)]
    pub fn filter_by_l(l: i256) -> TableIter<Self> {
        Self::filter(|row| row.l == l)
    }
    #[allow(unused)]
    pub fn filter_by_m(m: bool) -> TableIter<Self> {
        Self::filter(|row| row.m == m)
    }
    #[allow(unused)]
    pub fn filter_by_n(n: f32) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn filter_by_o(o: f64) -> TableIter<Self> {
        Self::filter(|row| row.o == o)
    }
    #[allow(unused)]
    pub fn filter_by_p(p: String) -> TableIter<Self> {
        Self::filter(|row| row.p == p)
    }
}
