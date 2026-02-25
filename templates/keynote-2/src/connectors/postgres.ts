import type {RpcConnector} from '../core/connectors';
import * as console from "node:console";
import {Pool} from "pg";

export function postgres_no_rpc(url: string): RpcConnector {
    return postgres(url);
}

export function postgres(
    url = process.env.PG_URL!
): RpcConnector {
    // Ready or not?
    let ready!: Promise<void>;
    let resolveReady!: () => void;
    let rejectReady!: (e: unknown) => void;

    function armReady() {
        ready = new Promise<void>((res, rej) => {
            resolveReady = res;
            rejectReady = rej;
        });
    }

    let pool: Pool;

    async function connect() {
        if (!url) throw new Error('PG_URL not set');
        armReady()
        pool = new Pool({
            connectionString: process.env.PG_URL,
        });
        try {
            await pool.query("SELECT 1");
            resolveReady()
        } catch (err) {
            let message = "Failed to connect to postgres";
            if (err instanceof Error) {
                message += ": " + err.message;
            }
            rejectReady(new Error(message));
        }
    }

    return {
        name: 'postgres',

        async open() {
            try {
                await connect();
                await ready;
            } catch (err) {
                console.error('[postgres] open() failed', err);
                throw err;
            }
        },

        async close() {
            try {
                await pool.end();
            } catch (err) {
                console.error('[postgres] close() failed', err);
            }
        },

        async call(fn: string, args: Record<string, any>) {
            switch (fn) {
                case 'seed': {
                    try {
                        await pool.query(
                            `DROP TABLE IF EXISTS account;
                            DROP INDEX IF EXISTS account_id_index;
                            CREATE TABLE account
                            (
                                id      INTEGER PRIMARY KEY,
                                balance BIGINT
                            );
                            CREATE INDEX account_id_index ON account USING HASH (id);`
                        );
                        await pool.query(
                            `CREATE OR REPLACE PROCEDURE seed(n INTEGER, balance BIGINT)
                            LANGUAGE plpgsql
                            AS $$
                            BEGIN
                                DELETE FROM account;
                                INSERT INTO account (id, balance)
                                SELECT g, balance
                                FROM generate_series(0, n - 1) AS g;
                            END;
                            $$;`
                        );
                        await pool.query(
                            `CREATE OR REPLACE PROCEDURE transfer(from_id INTEGER, to_id INTEGER, amount BIGINT)
                            LANGUAGE plpgsql
                            AS $$
                            DECLARE
                                from_balance   BIGINT;
                                to_balance     BIGINT;
                            BEGIN
                                SELECT balance INTO STRICT from_balance
                                FROM account
                                WHERE id = from_id;
                                
                                IF from_balance < amount THEN
                                    RAISE EXCEPTION 'insufficient_funds';
                                END IF;
                                
                                SELECT balance INTO STRICT to_balance
                                FROM account
                                WHERE id = to_id;
                            
                                UPDATE account
                                SET balance = balance - amount
                                WHERE id = from_id;
                            
                                UPDATE account
                                SET balance = balance + amount
                                WHERE id = to_id;
                            END;
                            $$;`
                        );
                        await pool.query(`CALL seed(${args.accounts}, ${args.initialBalance})`);
                    } catch (err) {
                        let message = "Failed to seed";
                        if (err instanceof Error) {
                            message += ": " + err.message;
                        }
                        throw new Error(message);
                    }
                    return;
                }
                case 'transfer': {
                    // console.log("transfer: " + args.from + ", " + args.to + ", " + args.amount)
                    await pool.query(`CALL transfer(${args.from}, ${args.to}, ${args.amount})`);
                    return;
                }
                default:
                    throw new Error(`Unknown function: ${fn}`);
            }
        },

        async getAccount(id: number): Promise<{ id: number; balance: bigint; } | null> {
            console.log(id)
            // TODO
            return null;
        },

        async verify(): Promise<void> {
            return Promise.resolve(undefined);
        },
    };
}
