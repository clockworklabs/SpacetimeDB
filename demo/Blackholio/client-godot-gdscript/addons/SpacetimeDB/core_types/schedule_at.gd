## A SpacetimeDB [code]ScheduleAt[/code] value — the tagged union stored in the
## [code]scheduled_at[/code] column of a [code]#[scheduled][/code] table.
##
## On the wire it is a BSATN sum type [code]Interval(TimeDuration) | Time(Timestamp)[/code];
## both payloads are an [code]i64[/code] microsecond count. [member kind] preserves which
## variant was sent so client code can tell a repeating interval from an absolute time.
@tool
class_name ScheduleAt
extends Resource

## Wire variant tags: INTERVAL = Interval(TimeDuration), TIME = Time(Timestamp).
enum Kind { INTERVAL = 0, TIME = 1 }

## Which variant this value is. Matches the BSATN sum tag.
@export var kind: Kind = Kind.INTERVAL
## The i64 microsecond payload: a duration when [member kind] is INTERVAL, an absolute
## timestamp (micros since the unix epoch) when TIME.
@export var micros: int = 0


## Builds an [code]Interval[/code] schedule firing every [param p_micros] microseconds.
static func interval(p_micros: int) -> ScheduleAt:
	var result: ScheduleAt = ScheduleAt.new()
	result.kind = Kind.INTERVAL
	result.micros = p_micros
	return result


## Builds a [code]Time[/code] schedule firing at absolute [param p_micros] (micros since epoch).
static func at_time(p_micros: int) -> ScheduleAt:
	var result: ScheduleAt = ScheduleAt.new()
	result.kind = Kind.TIME
	result.micros = p_micros
	return result


func is_interval() -> bool:
	return kind == Kind.INTERVAL


func is_time() -> bool:
	return kind == Kind.TIME
