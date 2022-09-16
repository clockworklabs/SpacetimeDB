WITH days AS (
  SELECT
    date_trunc('day', dd)::date as day
  FROM generate_series
          ( current_timestamp - '60 day'::interval
          , current_timestamp
          , '1 day'::interval) dd
),
first_player_session_by_date AS (
  SELECT
    days.day,
    player_session.first_sign_in_ts,
    player_session.player_id
  FROM
    days
  JOIN
    (
      SELECT
        player_id,
        MIN(sign_in_ts) as first_sign_in_ts
      FROM
        analytics.player_session
      GROUP BY
        player_id) player_session
  ON
    days.day::timestamp < player_session.first_sign_in_ts AND days.day::timestamp + '1 day'::interval > player_session.first_sign_in_ts
),
days_age AS (
  SELECT
    c1.day as first_day,
    c2.day - c1.day as age
  FROM
    days c1
  JOIN
    days c2
  ON
    c2.day >= c1.day
),
first_sign_in_with_sign_in AS (
  SELECT
    f.day as first_day,
    date_trunc('day', s.sign_in_ts)::date as day,
    s.sign_in_ts,
    s.player_id
  FROM
    first_player_session_by_date f
  JOIN
    analytics.player_session s
  ON
    f.player_id = s.player_id
),
sign_in_age AS (
  SELECT
    first_day,
    day - first_day as age,
    player_id
  FROM
    first_sign_in_with_sign_in a
),
cohort_table AS (
  SELECT
    first_day,
    age,
    COUNT(DISTINCT player_id) as cohort_sign_ins
  FROM
    sign_in_age
  GROUP BY
    age, first_day
),
expanded_cohort_table AS (
  SELECT
    days_age.first_day,
    days_age.age,
    COALESCE(cohort_table.cohort_sign_ins, 0) as cohort_sign_ins
  FROM
    days_age
  LEFT JOIN
    cohort_table
  ON
    days_age.first_day = cohort_table.first_day AND days_age.age = cohort_table.age
),
tmp_retention AS (
  SELECT
    a.first_day,
    a.age,
    a.cohort_sign_ins as sign_ins,
    b.cohort_sign_ins as first_day_sign_ins
  FROM
    expanded_cohort_table a
  JOIN
    expanded_cohort_table b
  ON
    a.first_day = b.first_day AND b.age = 0
  WHERE
    a.age != 0
),
retention AS (
  SELECT
    first_day,
    age,
    CASE first_day_sign_ins > 0 WHEN TRUE THEN sign_ins::decimal / first_day_sign_ins ELSE NULL END AS retention
  FROM
    tmp_retention
),
avg_retention AS (
  SELECT
    age,
    CASE AVG(first_day_sign_ins) > 0 WHEN TRUE THEN AVG(sign_ins::decimal)::decimal / AVG(first_day_sign_ins) ELSE 0 END AS retention
  FROM 
    tmp_retention
  GROUP BY
    age
  ORDER BY
    age
),
oo AS (
  SELECT
    *
  FROM
    avg_retention
  WHERE
    age <= 30
)
SELECT
  MAX(CASE WHEN oo.age = 1 THEN oo.retention ELSE NULL END) AS "Avg Day 1",
  MAX(CASE WHEN oo.age = 2 THEN oo.retention ELSE NULL END) AS "Avg Day 2",
  MAX(CASE WHEN oo.age = 3 THEN oo.retention ELSE NULL END) AS "Avg Day 3",
  MAX(CASE WHEN oo.age = 7 THEN oo.retention ELSE NULL END) AS "Avg Day 7",
  MAX(CASE WHEN oo.age = 14 THEN oo.retention ELSE NULL END) AS "Avg Day 14",
  MAX(CASE WHEN oo.age = 30 THEN oo.retention ELSE NULL END) AS "Avg Day 30"
FROM
  oo;
