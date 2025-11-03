import type { DbView } from "../server/db_view";
import type { UntypedSchemaDef } from "../server/schema";

/**
 * An untyped database view, where the table names and row types are not known.
 * Each key is a camelCased version of the table name, and each value is an untyped table handle.
 * 
 * For example, a database with tables "user_profile" and "game_stats" would have the type:
 * {
 *   userProfile: TableHandle<"user_profile", any>;
 *   gameStats: TableHandle<"game_stats", any>;
 * }
 */
export type UntypedDbView = DbView<UntypedSchemaDef>;