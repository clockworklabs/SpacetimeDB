# Incremental Evaluation for Joins in Bitcraft

Bitcraft needs to subscribe to queries that only return entities within a 3x3
chunk of a player’s location. This means it needs to subscribe to queries like
the following

```sql
SELECT ResourceDeposit.*
FROM ResourceDeposit r
JOIN LocationState l
ON r.entity_id = l.entity_id
WHERE l.x > x_lower
    and l.x < x_upper
    and l.z > z_lower
    and l.z < z_upper
```

Note the following

- The bitcraft client maintains a subscription cache which is the
  materialization of the queries of a subscription
- The client will return an error if it receives a duplicate row that is already
  in its cache
- The client will return an error if it receives a delete for a row that is not
  in its cache

Also note the following notation.

Given a table `A` and a transaction `Tx`

- The set of rows inserted into `A` during `Tx` will be represented by `A+`
- The set of rows deleted from `A` during `Tx` will be represented by `A-`
- `U` denotes the set union operator
- `\` denotes the set subtraction operator
- And from this point forward it is understood that the rows of `A` reflect the
  complete set of changes made in `Tx`

Given the above notation, what is the correct way to incrementally evaluate a
join query?

```sql
let inserts = {A+ join B} U {A join B+}
let deletes = {A- join B} U {A join B-} U {A- join B-}
```

Before sending the set of inserted and deleted rows back to the client, we
should remove any rows that are in both the insert and delete sets.

### Cases

Using `Re` and `Lo` as shorthand for `ResourceDeposit` and `LocationState`
respectively

(1) An entity moves from inside a player’s region to somewhere still inside a
player’s region

This means that

1. `Re+` is empty
2. `Re-` is empty
3. `Lo-` has one row
4. `Lo+` has one row
5. The query that returns inserted rows is given by

    ```sql
    SELECT Re.* from Re  join Lo+ ...
    ```

    It returns a single row

6. The query that returns deleted rows is given by

    ```sql
    SELECT Re.* from Re  join Lo- ...
    ```

    It returns that same row

7. The final result is the empty set because the same row is present in both the
   insert and delete sets

(2) An entity moves from outside a player’s region to somewhere still outside a
player’s region

Same as (1) except now both queries return the empty set and so therefor the
final result is the empty set.

(3) An entity moves from inside a player’s region to outside a player’s region

This means that

1. `Re+` is empty
2. `Re-` is empty
3. `Lo-` has one row
4. `Lo+` has one row
5. The query that returns inserted rows is given by

    ```sql
    SELECT Re.* from Re  join Lo+ ...
    ```

    It returns the empty set because the row of `Lo+` is outside the player’s
    region

6. The query that returns deleted rows is given by

    ```sql
    SELECT Re.* from Re  join Lo- ...
    ```

    It returns one row because the row of `Lo-` was inside the player’s region

7. The final result is a row that should be deleted from the client cache

(4) An entity moves from outside a player’s region to inside a player’s region

Same as (3) but this time the final result is a row that should be inserted into
the client cache.

Note that this row will not already be present in the client cache.

(5) A new entity is introduced into the player’s region

This means

1. `Re+` has one row
2. `Re-` is empty
3. `Lo-` is empty
4. `Lo+` has one row
5. The query that returns inserted rows is given by

    ```sql
    SELECT Re.* from Re  join Lo+ ...
    UNION
    SELECT Re.* from Re+ join Lo  ...
    ```

    It returns a single row

6. There are no deleted rows
7. The final result is a row that should be deleted from the client cache

(6) A new entity is introduced outside the player’s region

Same as (5) except the neither query returns any rows and so the final result is
the empty set

(7) An entity is deleted from inside a player’s region

Same as (5) except the query for inserted rows returns the empty set, the query
for deleted rows returns a single row, and the final result is a row that should
be deleted from the client cache.

(8) An entity is deleted from outside a player’s region

Same as (6)

### Is this still correct for multiple types of updates within a transaction?

The key is that we are only focusing on primary foreign key joins for Bitcraft.
This means an updated row from the right table joins with at most one row from
the left table.

Updates to different entities touch non-overlapping rows and therefore the query

```sql
let inserts = {A+ join B} U {A join B+}
let deletes = {A- join B} U {A join B-} U {A- join B-}
```

is just a union of the different cases mentioned above.

Multiple updates to the same entity in the same transaction will be taken care
of by either

1. Set semantics (i.e deduplication)
2. Removing rows that show up in both the insert and delete sets

Hence this will work in general for the case of primary foreign key joins such
as the ones Bitcraft employs.
