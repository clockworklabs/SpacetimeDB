This crate check the conformance of SpacetimeDB to SQL:2016

# Setup PG

Create a database called `TestSpace`

# Issues

List of issues in the execution of the conformance test

## UNSUPPORTED: issue 1

`CHARACTERS/OCTETS` is not implemented by neither PG nor Sqlite
```sql
CREATE TABLE TABLE_E021_01_01_02 ( A CHAR ( 8 CHARACTERS ) )
CREATE TABLE TABLE_E021_01_01_02 ( A CHAR ( 8 OCTETS ) )
```
## UNSUPPORTED: issue 2

`CHAR VARING` is not implemented by neither PG nor Sqlite

```sql
CREATE TABLE TABLE_E021_02_01_02 ( A CHAR VARING ( 8 CHARACTERS ) )
```

## UNSUPPORTED: issue 3

` AS ( C , D )` is a syntax error on PG and Sqlite

```sql
SELECT * AS ( C , D ) FROM TABLE_E051_07_01_01
```
## REPLACED: issue 4

`CURRENT_TIME` and `timetz` type is marked as "Don't use EVER" by PG:
https://wiki.postgresql.org/wiki/Don%27t_Do_This#Don.27t_use_timetz

```sql
SELECT CURRENT_TIME
```

## UNSUPPORTED: issue 5

`CASE 0 WHEN 2 , 2` is a syntax error on PG and Sqlite

```sql
SELECT CASE 0 WHEN 2 , 2 THEN 1 END
```

## WRONG: issue 6

This expression fail in PG by the lack of a timezone and Sqlite spit out a nonsensical value (`1`)
```sql
SELECT CAST ( CAST ( '01:02:03' AS TIME ) AS TIMESTAMP )
```